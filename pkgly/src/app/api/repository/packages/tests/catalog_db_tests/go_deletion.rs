// ABOUTME: Verifies Go package deletion against real PostgreSQL and local storage.
// ABOUTME: Covers artifact bundles, module directories, and stale catalog recovery.
use super::*;
use crate::repository::go::{GoRepositoryConfig, GoRepositoryConfigType};

#[test]
fn go_artifact_bundles_require_a_version_directory() {
    for suffix in ["zip", "mod", "info"] {
        let path = format!("example.com/module/@v/v0.1.0.{suffix}");
        assert_eq!(
            go_related_paths(&path),
            Some(vec![
                "example.com/module/@v/v0.1.0.zip".into(),
                "example.com/module/@v/v0.1.0.mod".into(),
                "example.com/module/@v/v0.1.0.info".into(),
            ])
        );
    }
    for directory in [
        "example.com/module.zip",
        "example.com/module",
        "example.com/module/@v",
    ] {
        assert!(go_related_paths(directory).is_none(), "{directory}");
    }
}

async fn insert_go_repository(pool: &PgPool, storage_id: Uuid, proxy: bool) -> Uuid {
    let config = if proxy {
        GoRepositoryConfig::Proxy(Default::default())
    } else {
        GoRepositoryConfig::Hosted
    };
    NewRepository {
        name: if proxy {
            "go-proxy-test"
        } else {
            "go-hosted-test"
        }
        .into(),
        uuid: Uuid::new_v4(),
        repository_type: "go".into(),
        configs: HashMap::from_iter([(
            GoRepositoryConfigType::get_type_static().into(),
            serde_json::to_value(config).expect("go config"),
        )]),
    }
    .insert(storage_id, pool)
    .await
    .expect("insert go repository")
    .id
}

async fn seed_module(site: &Pkgly, repository_id: Uuid, module: &str, files: bool) {
    let storage = site
        .get_repository(repository_id)
        .expect("go repository")
        .get_storage();
    for version in ["v0.1.0", "v0.2.0"] {
        let base = format!("{module}/@v/{version}");
        insert_maven_version(
            &site.database,
            repository_id,
            module,
            version,
            &format!("{base}.zip"),
        )
        .await;
        if files {
            for suffix in GO_FILE_SUFFIXES {
                storage
                    .save_file(
                        repository_id,
                        FileContent::from(b"module bytes".as_slice()),
                        &StoragePath::from(format!("{base}{suffix}")),
                    )
                    .await
                    .expect("save artifact");
            }
        }
    }
    if files {
        storage
            .save_file(
                repository_id,
                FileContent::from(b"v0.1.0\nv0.2.0\n".as_slice()),
                &StoragePath::from(format!("{module}/@v/list")),
            )
            .await
            .expect("save version list");
    }
}

async fn version_paths(pool: &PgPool, repository_id: Uuid, module: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT pv.path FROM project_versions pv JOIN projects p ON p.id = pv.project_id WHERE pv.repository_id = $1 AND p.key = $2 ORDER BY pv.path")
        .bind(repository_id).bind(module).fetch_all(pool).await.expect("version paths")
}

async fn check_deletion(site: &Pkgly, auth: &Authentication, repository_id: Uuid, scenario: &str) {
    let module = format!("example.com/{repository_id}/dummy-{scenario}");
    let sibling = format!("{module}-other");
    seed_module(
        site,
        repository_id,
        &module,
        !scenario.starts_with("missing"),
    )
    .await;
    seed_module(site, repository_id, &sibling, true).await;
    if scenario == "missing" {
        sqlx::query("UPDATE package_files SET project_version_id = NULL WHERE repository_id = $1 AND package = $2")
            .bind(repository_id).bind(&module).execute(&site.database).await.expect("unlink legacy file rows");
    }
    let path = match scenario {
        "directory" | "missing-directory" => format!("{module}/"),
        "mod" => format!("{module}/@v/v0.1.0.mod"),
        _ => format!("{module}/@v/v0.1.0.zip"),
    };
    let response = delete_cached_packages(
        State(site.clone()),
        auth.clone(),
        Path(repository_id),
        Json(PackageDeleteRequest { paths: vec![path] }),
    )
    .await
    .expect("delete module");
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_catalog_deleted(site, repository_id, &module, scenario).await;
    assert_eq!(
        version_paths(&site.database, repository_id, &sibling)
            .await
            .len(),
        2
    );
    assert_storage_deleted(site, repository_id, &module, scenario).await;
}

async fn assert_catalog_deleted(site: &Pkgly, repository_id: Uuid, module: &str, scenario: &str) {
    let remaining = version_paths(&site.database, repository_id, &module).await;
    let expected = if scenario.ends_with("directory") {
        vec![]
    } else {
        vec![format!("{module}/@v/v0.2.0.zip")]
    };
    assert_eq!(
        remaining, expected,
        "{scenario}: deleted version remains searchable"
    );
    let files: Vec<String> = sqlx::query_scalar("SELECT path FROM package_files WHERE repository_id = $1 AND package = $2 AND deleted_at IS NULL ORDER BY path")
        .bind(repository_id).bind(&module).fetch_all(&site.database).await.expect("package files");
    assert_eq!(
        files, expected,
        "{scenario}: deleted version remains listed"
    );
}

async fn assert_storage_deleted(site: &Pkgly, repository_id: Uuid, module: &str, scenario: &str) {
    let storage = site
        .get_repository(repository_id)
        .expect("go repository")
        .get_storage();
    for suffix in GO_FILE_SUFFIXES {
        assert!(
            !storage
                .file_exists(
                    repository_id,
                    &StoragePath::from(format!("{module}/@v/v0.1.0{suffix}"))
                )
                .await
                .expect("artifact existence")
        );
    }
    if matches!(scenario, "zip" | "mod") {
        let Some(StorageFile::File { content, .. }) = storage
            .open_file(
                repository_id,
                &StoragePath::from(format!("{module}/@v/list")),
            )
            .await
            .expect("version list")
        else {
            panic!("version list missing")
        };
        assert_eq!(
            content.read_to_vec(32).await.expect("read versions"),
            b"v0.2.0\n"
        );
    }
}

#[tokio::test]
async fn go_deletion_cleans_catalog_and_preserves_other_versions_and_repositories() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_pool().await;
    let root = tempfile::tempdir().expect("storage root");
    let storage_id = insert_storage_at(db.pool(), root.path()).await;
    let hosted = insert_go_repository(db.pool(), storage_id, false).await;
    let proxy = insert_go_repository(db.pool(), storage_id, true).await;
    let site = build_site(&db, root.path()).await;
    let auth = sample_auth(db.pool()).await;
    let protected_module = format!("example.com/{hosted}/dummy-zip");
    seed_module(&site, proxy, &protected_module, true).await;
    for repository_id in [hosted, proxy] {
        for scenario in ["zip", "mod", "directory", "missing", "missing-directory"] {
            check_deletion(&site, &auth, repository_id, scenario).await;
            if repository_id == hosted {
                assert_eq!(
                    version_paths(db.pool(), proxy, &protected_module)
                        .await
                        .len(),
                    2
                );
            }
        }
    }
    site.close().await;
}
