// ABOUTME: Verifies Helm deletion reports removals independently of webhook snapshots.
// ABOUTME: Exercises real catalog rows, storage files, and the shared package deletion API.
use super::*;

#[tokio::test]
async fn helm_deletion_reports_removed_chart_without_webhook_snapshots() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_pool().await;
    let root = tempfile::tempdir().expect("storage root");
    let storage_id = insert_storage_at(db.pool(), root.path()).await;
    let repo = NewRepository {
        name: "helm-delete-test".into(),
        uuid: Uuid::new_v4(),
        repository_type: "helm".into(),
        configs: HashMap::default(),
    }
    .insert(storage_id, db.pool())
    .await
    .expect("insert Helm repository");
    let path = "charts/test-chart/test-chart-0.1.0.tgz";
    insert_maven_version(db.pool(), repo.id, "test-chart", "0.1.0", path).await;
    let site = build_site(&db, root.path()).await;
    let storage = site
        .get_repository(repo.id)
        .expect("Helm repository")
        .get_storage();
    storage
        .save_file(
            repo.id,
            FileContent::from(b"chart archive".as_slice()),
            &StoragePath::from(path),
        )
        .await
        .expect("save chart");
    let response = delete_cached_packages(
        State(site.clone()),
        sample_auth(db.pool()).await,
        Path(repo.id),
        Json(PackageDeleteRequest {
            paths: vec![path.into()],
        }),
    )
    .await
    .expect("delete chart");
    assert_eq!(response.status(), http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("delete response");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("delete JSON");
    assert_eq!(payload, json!({"deleted":1,"missing":[],"rejected":[]}));
    assert!(
        !storage
            .file_exists(repo.id, &StoragePath::from(path))
            .await
            .expect("chart existence")
    );
    assert!(
        !DBPackageFile::repository_has_rows(db.pool(), repo.id)
            .await
            .expect("package rows")
    );
    site.close().await;
}
