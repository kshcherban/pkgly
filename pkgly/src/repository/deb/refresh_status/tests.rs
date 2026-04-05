#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use super::*;

use crate::repository::NewRepository;
use once_cell::sync::Lazy;
use sqlx::Row;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

use nr_core::{database::entities::storage::NewDBStorage, storage::StorageName};

static DB_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

struct TestDb {
    pool: PgPool,
    _container: Container<'static, GenericImage>,
    _docker: &'static Cli,
}

impl TestDb {
    fn pool(&self) -> &PgPool {
        &self.pool
    }
}

async fn start_postgres() -> TestDb {
    let docker: &'static Cli = Box::leak(Box::new(Cli::default()));
    let image = GenericImage::new("postgres", "18-alpine")
        .with_env_var("POSTGRES_PASSWORD", "password")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres");
    let container = docker.run(image);
    let port = container.get_host_port_ipv4(5432);
    let url = format!("postgres://postgres:password@127.0.0.1:{port}/postgres");

    let mut last_err: Option<anyhow::Error> = None;
    for _ in 0..30 {
        match PgPoolOptions::new().max_connections(4).connect(&url).await {
            Ok(pool) => {
                return TestDb {
                    pool,
                    _container: container,
                    _docker: docker,
                };
            }
            Err(err) => {
                last_err = Some(err.into());
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }
    }

    panic!(
        "postgres container did not become ready: {}",
        last_err.unwrap_or_else(|| anyhow::anyhow!("unknown error"))
    );
}

async fn fresh_pool() -> TestDb {
    let db = start_postgres().await;
    nr_core::database::migration::run_migrations(db.pool())
        .await
        .expect("run migrations");
    db
}

async fn reset_database(db: &TestDb) {
    sqlx::query(
        "TRUNCATE TABLE deb_proxy_refresh_status, project_versions, projects, repositories, storages RESTART IDENTITY CASCADE",
    )
    .execute(db.pool())
    .await
    .expect("truncate tables");
}

async fn insert_storage(pool: &PgPool) -> Uuid {
    let storage_name = StorageName::new("primary".to_string()).expect("storage name");
    let storage = NewDBStorage::new(
        "Local".into(),
        storage_name,
        serde_json::json!({ "path": "/tmp" }),
    );
    storage
        .insert(pool)
        .await
        .expect("insert storage")
        .expect("storage row")
        .id
}

async fn insert_deb_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
    let repo = NewRepository {
        name: "deb-proxy-test".into(),
        uuid: Uuid::new_v4(),
        repository_type: "deb".into(),
        configs: ahash::HashMap::with_hasher(Default::default()),
    };
    repo.insert(storage_id, pool).await.expect("insert repo").id
}

#[tokio::test]
async fn try_mark_refresh_started_is_exclusive() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_deb_repository(db.pool(), storage_id).await;

    let first = try_mark_deb_proxy_refresh_started(db.pool(), repo_id)
        .await
        .expect("start ok");
    let DebProxyRefreshLockOutcome::Acquired(lock) = first else {
        panic!("expected acquired");
    };

    let second = try_mark_deb_proxy_refresh_started(db.pool(), repo_id)
        .await
        .expect("start ok");
    assert!(matches!(second, DebProxyRefreshLockOutcome::AlreadyRunning));

    lock.release().await.expect("release");
}

#[tokio::test]
async fn mark_succeeded_clears_in_progress_and_stores_counts() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_deb_repository(db.pool(), storage_id).await;

    let lock = match try_mark_deb_proxy_refresh_started(db.pool(), repo_id)
        .await
        .expect("start ok")
    {
        DebProxyRefreshLockOutcome::Acquired(lock) => lock,
        DebProxyRefreshLockOutcome::AlreadyRunning => panic!("expected acquired"),
    };

    let summary = DebProxyRefreshSummary {
        downloaded_packages: 3,
        downloaded_files: 5,
    };
    mark_deb_proxy_refresh_succeeded(db.pool(), repo_id, summary)
        .await
        .expect("mark ok");
    lock.release().await.expect("release");

    let row = sqlx::query(
        r#"
        SELECT in_progress, last_error, last_downloaded_packages, last_downloaded_files
        FROM deb_proxy_refresh_status
        WHERE repository_id = $1
        "#,
    )
    .bind(repo_id)
    .fetch_one(db.pool())
    .await
    .expect("fetch row");

    let in_progress: bool = row.try_get("in_progress").expect("in_progress");
    let last_error: Option<String> = row.try_get("last_error").expect("last_error");
    let last_downloaded_packages: Option<i32> = row
        .try_get("last_downloaded_packages")
        .expect("last_downloaded_packages");
    let last_downloaded_files: Option<i32> = row
        .try_get("last_downloaded_files")
        .expect("last_downloaded_files");

    assert!(!in_progress);
    assert!(last_error.is_none());
    assert_eq!(last_downloaded_packages, Some(3));
    assert_eq!(last_downloaded_files, Some(5));
}

#[tokio::test]
async fn mark_failed_sets_last_error_and_clears_in_progress() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_deb_repository(db.pool(), storage_id).await;

    let lock = match try_mark_deb_proxy_refresh_started(db.pool(), repo_id)
        .await
        .expect("start ok")
    {
        DebProxyRefreshLockOutcome::Acquired(lock) => lock,
        DebProxyRefreshLockOutcome::AlreadyRunning => panic!("expected acquired"),
    };

    mark_deb_proxy_refresh_failed(db.pool(), repo_id, "boom")
        .await
        .expect("mark ok");
    lock.release().await.expect("release");

    let row = sqlx::query(
        r#"
        SELECT in_progress, last_error
        FROM deb_proxy_refresh_status
        WHERE repository_id = $1
        "#,
    )
    .bind(repo_id)
    .fetch_one(db.pool())
    .await
    .expect("fetch row");

    let in_progress: bool = row.try_get("in_progress").expect("in_progress");
    let last_error: Option<String> = row.try_get("last_error").expect("last_error");

    assert!(!in_progress);
    assert_eq!(last_error.as_deref(), Some("boom"));
}

#[tokio::test]
async fn try_mark_refresh_started_allows_reclaiming_stale_in_progress() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_deb_repository(db.pool(), storage_id).await;

    // Simulate a crash that left in_progress = TRUE.
    ensure_deb_proxy_refresh_status_row(db.pool(), repo_id)
        .await
        .expect("ensure row");
    sqlx::query(
        r#"
        UPDATE deb_proxy_refresh_status
        SET in_progress = TRUE,
            last_started_at = NOW()
        WHERE repository_id = $1
        "#,
    )
    .bind(repo_id)
    .execute(db.pool())
    .await
    .expect("set in_progress");

    // A new refresh attempt should be able to reclaim the status (crash-recovery friendly).
    let outcome = try_mark_deb_proxy_refresh_started(db.pool(), repo_id)
        .await
        .expect("start ok");
    let DebProxyRefreshLockOutcome::Acquired(lock) = outcome else {
        panic!("expected acquired");
    };
    lock.release().await.expect("release");
}
