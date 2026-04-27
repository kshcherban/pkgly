#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use super::*;

use crate::repository::NewRepository;
use nr_core::{database::entities::storage::NewDBStorage, storage::StorageName};
use once_cell::sync::Lazy;
use sqlx::Row;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

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
        "TRUNCATE TABLE package_retention_status, package_files, repository_configs, project_versions, projects, repositories, storages RESTART IDENTITY CASCADE",
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

async fn insert_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
    let repo = NewRepository {
        name: "retention-test".into(),
        uuid: Uuid::new_v4(),
        repository_type: "maven".into(),
        configs: ahash::HashMap::with_hasher(Default::default()),
    };
    repo.insert(storage_id, pool).await.expect("insert repo").id
}

async fn insert_package_file(
    pool: &PgPool,
    repository_id: Uuid,
    package: &str,
    name: &str,
    path: &str,
    modified_at: chrono::DateTime<chrono::Utc>,
) -> i64 {
    sqlx::query_scalar(
        r#"
        INSERT INTO package_files (
            repository_id,
            package,
            name,
            path,
            size_bytes,
            modified_at
        )
        VALUES ($1, $2, $3, $4, 1, $5)
        RETURNING id
        "#,
    )
    .bind(repository_id)
    .bind(package)
    .bind(name)
    .bind(path)
    .bind(modified_at)
    .fetch_one(pool)
    .await
    .expect("insert package file")
}

#[tokio::test]
async fn try_mark_retention_started_is_exclusive() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_repository(db.pool(), storage_id).await;

    let first = try_mark_package_retention_started(db.pool(), repo_id)
        .await
        .expect("start ok");
    let PackageRetentionLockOutcome::Acquired(lock) = first else {
        panic!("expected acquired");
    };

    let second = try_mark_package_retention_started(db.pool(), repo_id)
        .await
        .expect("start ok");
    assert!(matches!(
        second,
        PackageRetentionLockOutcome::AlreadyRunning
    ));

    lock.release().await.expect("release");
}

#[tokio::test]
async fn mark_succeeded_clears_in_progress_and_stores_deleted_count() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_repository(db.pool(), storage_id).await;

    let lock = match try_mark_package_retention_started(db.pool(), repo_id)
        .await
        .expect("start ok")
    {
        PackageRetentionLockOutcome::Acquired(lock) => lock,
        PackageRetentionLockOutcome::AlreadyRunning => panic!("expected acquired"),
    };

    mark_package_retention_succeeded(db.pool(), repo_id, 7)
        .await
        .expect("mark ok");
    lock.release().await.expect("release");

    let row = sqlx::query(
        r#"
        SELECT in_progress, last_error, last_deleted_count
        FROM package_retention_status
        WHERE repository_id = $1
        "#,
    )
    .bind(repo_id)
    .fetch_one(db.pool())
    .await
    .expect("fetch row");

    let in_progress: bool = row.try_get("in_progress").expect("in_progress");
    let last_error: Option<String> = row.try_get("last_error").expect("last_error");
    let last_deleted_count: Option<i32> = row
        .try_get("last_deleted_count")
        .expect("last_deleted_count");

    assert!(!in_progress);
    assert!(last_error.is_none());
    assert_eq!(last_deleted_count, Some(7));
}

#[tokio::test]
async fn mark_failed_sets_last_error_and_clears_in_progress() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_repository(db.pool(), storage_id).await;

    let lock = match try_mark_package_retention_started(db.pool(), repo_id)
        .await
        .expect("start ok")
    {
        PackageRetentionLockOutcome::Acquired(lock) => lock,
        PackageRetentionLockOutcome::AlreadyRunning => panic!("expected acquired"),
    };

    mark_package_retention_failed(db.pool(), repo_id, "boom")
        .await
        .expect("mark ok");
    lock.release().await.expect("release");

    let row = sqlx::query(
        r#"
        SELECT in_progress, last_error
        FROM package_retention_status
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
async fn retention_candidates_respect_age_keep_count_and_ties() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_repository(db.pool(), storage_id).await;
    let now = chrono::Utc::now();
    let old = now - chrono::Duration::days(40);
    let newer = now - chrono::Duration::days(20);

    insert_package_file(db.pool(), repo_id, "alpha", "oldest", "alpha/oldest", old).await;
    insert_package_file(db.pool(), repo_id, "alpha", "old", "alpha/old", old).await;
    let latest_old_id = insert_package_file(
        db.pool(),
        repo_id,
        "alpha",
        "tie-newer-id",
        "alpha/tie-newer",
        old,
    )
    .await;
    insert_package_file(db.pool(), repo_id, "alpha", "new", "alpha/new", newer).await;
    insert_package_file(db.pool(), repo_id, "beta", "only-new", "beta/new", newer).await;

    let candidates =
        nr_core::database::entities::package_file::DBPackageFile::retention_candidates(
            db.pool(),
            repo_id,
            (now - chrono::Duration::days(30)).into(),
            2,
        )
        .await
        .expect("query candidates");

    let paths = candidates
        .iter()
        .map(|row| row.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["alpha/oldest", "alpha/old"]);
    assert!(
        candidates.iter().all(|row| row.id != latest_old_id),
        "higher id in a modified_at tie should be kept first"
    );
}

#[tokio::test]
async fn retention_candidates_zero_keep_can_delete_all_old_files() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repo_id = insert_repository(db.pool(), storage_id).await;
    let now = chrono::Utc::now();
    let old = now - chrono::Duration::days(40);

    insert_package_file(db.pool(), repo_id, "alpha", "one", "alpha/one", old).await;
    insert_package_file(db.pool(), repo_id, "alpha", "two", "alpha/two", old).await;

    let candidates =
        nr_core::database::entities::package_file::DBPackageFile::retention_candidates(
            db.pool(),
            repo_id,
            (now - chrono::Duration::days(30)).into(),
            0,
        )
        .await
        .expect("query candidates");

    assert_eq!(candidates.len(), 2);
}
