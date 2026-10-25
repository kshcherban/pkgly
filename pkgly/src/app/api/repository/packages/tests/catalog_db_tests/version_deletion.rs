// ABOUTME: Verifies version catalog cleanup for package files and version directories.
// ABOUTME: Covers Python, Maven, and NuGet deletion while preserving sibling versions.
use super::*;

async fn catalog_versions(site: &Pkgly, repository_id: Uuid, package: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT pv.version FROM project_versions pv JOIN projects p ON p.id = pv.project_id WHERE pv.repository_id = $1 AND p.key = $2 ORDER BY pv.version")
        .bind(repository_id).bind(package).fetch_all(&site.database).await.expect("catalog versions")
}

async fn check_version_deletion(
    site: &Pkgly,
    auth: &Authentication,
    repo: Uuid,
    format: &str,
    scenario: &str,
) {
    let package = format!("delete-{format}-{scenario}");
    let base = match format {
        "maven" => format!("dev/pkgly/{package}"),
        "nuget" => format!("v3/flatcontainer/{package}"),
        _ => package.clone(),
    };
    let suffix = if format == "python" { "" } else { "/" };
    let storage = site.get_repository(repo).expect("repository").get_storage();
    for version in ["0.1.0", "0.2.0"] {
        let path = format!("{base}/{version}{suffix}");
        insert_maven_version(&site.database, repo, &package, version, &path).await;
        if version != "0.1.0" || scenario != "missing" {
            storage
                .save_file(
                    repo,
                    FileContent::from(b"artifact bytes".as_slice()),
                    &StoragePath::from(format!("{base}/{version}/artifact.bin")),
                )
                .await
                .expect("save artifact");
        }
    }
    let path = if scenario == "metadata" {
        let path = format!("{base}/metadata.bin");
        storage
            .save_file(
                repo,
                FileContent::from(b"metadata".as_slice()),
                &StoragePath::from(path.as_str()),
            )
            .await
            .expect("save metadata");
        path
    } else if scenario == "file" {
        format!("{base}/0.1.0/artifact.bin")
    } else {
        format!("{base}/0.1.0{suffix}")
    };
    let response = delete_cached_packages(
        State(site.clone()),
        auth.clone(),
        Path(repo),
        Json(PackageDeleteRequest {
            paths: vec![path.clone()],
        }),
    )
    .await
    .expect("delete version");
    assert_eq!(response.status(), http::StatusCode::OK);
    if scenario == "metadata" {
        assert_eq!(
            catalog_versions(site, repo, &package).await,
            vec!["0.1.0", "0.2.0"],
            "metadata deletion must preserve package versions"
        );
        return;
    }
    assert_eq!(
        catalog_versions(site, repo, &package).await,
        vec!["0.2.0"],
        "{format}/{scenario}: stale version prevents republishing"
    );
    let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM package_files WHERE repository_id = $1 AND package = $2 AND deleted_at IS NULL")
        .bind(repo).bind(&package).fetch_one(&site.database).await.expect("active inventory");
    assert_eq!(
        active, 1,
        "{format}/{scenario}: sibling version must remain listed"
    );
    assert!(
        !storage
            .file_exists(repo, &StoragePath::from(path))
            .await
            .expect("deleted path")
    );
    assert!(
        storage
            .file_exists(
                repo,
                &StoragePath::from(format!("{base}/0.2.0/artifact.bin"))
            )
            .await
            .expect("sibling path")
    );
    insert_maven_version(
        &site.database,
        repo,
        &package,
        "0.1.0",
        &format!("{base}/0.1.0{suffix}"),
    )
    .await;
    assert_eq!(
        catalog_versions(site, repo, &package).await,
        vec!["0.1.0", "0.2.0"]
    );
}

#[tokio::test]
async fn version_deletion_cleans_file_directory_and_missing_metadata_for_republishing() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_pool().await;
    let root = tempfile::tempdir().expect("storage root");
    let storage_id = insert_storage_at(db.pool(), root.path()).await;
    let mut repositories = Vec::new();
    for format in ["python", "maven", "nuget"] {
        let repo = NewRepository {
            name: format!("delete-{format}"),
            uuid: Uuid::new_v4(),
            repository_type: format.into(),
            configs: HashMap::from_iter([(format.to_string(), json!({"type":"Hosted"}))]),
        }
        .insert(storage_id, db.pool())
        .await
        .expect("repository");
        repositories.push((format, repo.id));
    }
    let site = build_site(&db, root.path()).await;
    let auth = sample_auth(db.pool()).await;
    for (format, repository) in repositories {
        for scenario in ["directory", "file", "missing", "metadata"] {
            check_version_deletion(&site, &auth, repository, format, scenario).await;
        }
    }
    site.close().await;
}
