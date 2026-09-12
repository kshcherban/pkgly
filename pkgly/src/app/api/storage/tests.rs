// ABOUTME: Tests storage deletion permissions, cascading cleanup, and retry behavior.
// ABOUTME: Exercises real PostgreSQL, filesystem, and MinIO storage backends.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use super::*;

mod minio;
use minio::TestMinio;

use crate::repository::NewRepository;
use crate::test_support::DB_TEST_LOCK;
use http::StatusCode;
use nr_core::{
    database::{
        DatabaseConfig,
        entities::{
            repository::DBRepository,
            storage::{DBStorage, NewDBStorage, StorageDBType},
        },
        migration::run_migrations,
    },
    storage::StorageName,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    port: u16,
    _container: Container<'static, GenericImage>,
    _docker: &'static Cli,
}

impl TestDb {
    fn pool(&self) -> &PgPool {
        &self.pool
    }
}

async fn build_site(db: &TestDb, storage_root: &std::path::Path) -> Pkgly {
    let cfg = DatabaseConfig {
        user: "postgres".into(),
        password: "password".into(),
        database: "postgres".into(),
        host: "127.0.0.1".into(),
        port: Some(db.port),
    };
    let mut security = crate::app::config::SecuritySettings::default();
    security.egress.allowed_hosts.push("127.0.0.1".into());

    Pkgly::new(
        crate::app::config::Mode::Debug,
        crate::app::config::SiteSetting::default(),
        security,
        crate::app::authentication::session::SessionManagerConfig {
            database_location: storage_root.join("sessions.redb"),
            ..Default::default()
        },
        crate::repository::StagingConfig {
            staging_dir: storage_root.join("staging"),
            ..Default::default()
        },
        None,
        cfg,
        Some(storage_root.join("storages")),
    )
    .await
    .expect("create site")
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
    for _ in 0..60 {
        match PgPoolOptions::new().max_connections(4).connect(&url).await {
            Ok(pool) => {
                return TestDb {
                    pool,
                    port,
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

async fn fresh_db() -> TestDb {
    let db = start_postgres().await;
    run_migrations(db.pool()).await.expect("run migrations");
    db
}

async fn insert_local_storage(pool: &PgPool, name: &str, root: &std::path::Path) -> Uuid {
    let storage_name = StorageName::new(name.to_string()).expect("storage name");
    let storage = NewDBStorage::new(
        "Local".into(),
        storage_name,
        serde_json::json!({
            "type": "Local",
            "settings": {
                "path": root.to_string_lossy()
            }
        }),
    );
    storage
        .insert(pool)
        .await
        .expect("insert storage")
        .expect("storage row")
        .id
}

async fn insert_maven_repo(pool: &PgPool, storage_id: Uuid, name: &str) -> Uuid {
    let repo_id = Uuid::new_v4();
    let repo = NewRepository {
        name: name.into(),
        uuid: repo_id,
        repository_type: "maven".into(),
        configs: ahash::HashMap::from_iter([(
            "maven".to_string(),
            serde_json::json!({ "type": "Hosted" }),
        )]),
    };
    repo.insert(storage_id, pool).await.expect("insert repo").id
}

fn sample_user(
    user_id: i32,
    system_manager: bool,
) -> nr_core::database::entities::user::UserSafeData {
    use chrono::{DateTime, FixedOffset};
    use nr_core::user::{Email, Username, permissions::RepositoryActions};

    let fixed_time: DateTime<FixedOffset> =
        DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
    nr_core::database::entities::user::UserSafeData {
        id: user_id,
        name: "Test User".into(),
        username: Username::new("test_user".into()).expect("username"),
        email: Some(Email::new("user@example.com".into()).expect("email")),
        require_password_change: false,
        active: true,
        admin: false,
        user_manager: false,
        system_manager,
        default_repository_actions: vec![RepositoryActions::Read],
        updated_at: fixed_time,
        created_at: fixed_time,
    }
}

fn sample_auth_token(user_id: i32) -> nr_core::database::entities::user::auth_token::AuthToken {
    use chrono::{DateTime, FixedOffset};

    let fixed_time: DateTime<FixedOffset> =
        DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
    nr_core::database::entities::user::auth_token::AuthToken {
        id: 1,
        user_id,
        name: Some("token".into()),
        description: None,
        token: "token".into(),
        active: true,
        source: "test".into(),
        expires_at: None,
        created_at: fixed_time,
    }
}

fn admin_auth() -> Authentication {
    Authentication::AuthToken(sample_auth_token(1), sample_user(1, true))
}

fn call_delete(
    site: &Pkgly,
    auth: Authentication,
    id: Uuid,
    cascade: bool,
) -> impl std::future::Future<Output = Result<Response, InternalError>> + use<> {
    let site = site.clone();
    async move {
        delete_storage(
            auth,
            axum::extract::State(site),
            axum::extract::Path(id),
            axum::extract::Extension(AccessLogContext::default()),
            axum::extract::Query(DeleteStorageRequest { cascade }),
        )
        .await
    }
}

#[tokio::test]
async fn delete_storage_requires_storage_manager_permission() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let site = build_site(&db, storage_root.path()).await;

    let auth = Authentication::AuthToken(sample_auth_token(1), sample_user(1, false));
    let response = call_delete(&site, auth, storage_id, false)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        DBStorage::get_by_id(storage_id, db.pool())
            .await
            .expect("query")
            .is_some()
    );

    site.close().await;
}

#[tokio::test]
async fn delete_storage_returns_not_found_for_missing_storage() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, storage_root.path()).await;

    let response = call_delete(&site, admin_auth(), Uuid::new_v4(), true)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    site.close().await;
}

#[tokio::test]
async fn delete_storage_deletes_empty_storage_immediately() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let site = build_site(&db, storage_root.path()).await;

    let response = call_delete(&site, admin_auth(), storage_id, false)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        DBStorage::get_by_id(storage_id, db.pool())
            .await
            .expect("query")
            .is_none()
    );
    assert!(site.get_storage(storage_id).is_none());

    site.close().await;
}

#[tokio::test]
async fn delete_storage_attributes_audit_to_the_storage() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let site = build_site(&db, storage_root.path()).await;

    let access_log = AccessLogContext::default();
    let response = delete_storage(
        admin_auth(),
        axum::extract::State(site.clone()),
        axum::extract::Path(storage_id),
        axum::extract::Extension(access_log.clone()),
        axum::extract::Query(DeleteStorageRequest { cascade: false }),
    )
    .await
    .expect("handler ok");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let expected_id = storage_id.to_string();
    let snapshot = access_log.snapshot();
    assert_eq!(snapshot.resource_kind.as_deref(), Some("storage"));
    assert_eq!(snapshot.resource_id.as_deref(), Some(expected_id.as_str()));
    assert_eq!(snapshot.storage_id, Some(storage_id));
    assert_eq!(snapshot.resource_name.as_deref(), Some("primary"));

    site.close().await;
}

#[tokio::test]
async fn delete_storage_conflicts_when_not_empty_without_cascade() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let repo_id = insert_maven_repo(db.pool(), storage_id, "maven-hosted").await;
    let site = build_site(&db, storage_root.path()).await;

    let response = call_delete(&site, admin_auth(), storage_id, false)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["details"]["code"], "storage_not_empty");
    assert_eq!(json["details"]["repository_count"], 1);

    assert!(
        DBStorage::get_by_id(storage_id, db.pool())
            .await
            .expect("query")
            .is_some()
    );
    assert!(
        DBRepository::get_by_id(repo_id, db.pool())
            .await
            .expect("query")
            .is_some()
    );

    site.close().await;
}

#[tokio::test]
async fn delete_storage_cascade_removes_repositories_and_files() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let repo_id = insert_maven_repo(db.pool(), storage_id, "maven-hosted").await;
    let site = build_site(&db, storage_root.path()).await;

    let repo_dir = storage_root.path().join(repo_id.to_string());
    tokio::fs::create_dir_all(&repo_dir)
        .await
        .expect("repo dir");
    tokio::fs::write(repo_dir.join("artifact.jar"), b"data")
        .await
        .expect("artifact");

    let response = call_delete(&site, admin_auth(), storage_id, true)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        DBStorage::get_by_id(storage_id, db.pool())
            .await
            .expect("query")
            .is_none()
    );
    assert!(
        DBRepository::get_by_id(repo_id, db.pool())
            .await
            .expect("query")
            .is_none()
    );
    assert!(site.get_storage(storage_id).is_none());
    assert!(site.get_repository(repo_id).is_none());
    assert!(!repo_dir.exists());

    site.close().await;
}

#[tokio::test]
async fn delete_storage_cascade_fails_before_deletion_when_backend_unavailable() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let repo_id = insert_maven_repo(db.pool(), storage_id, "maven-hosted").await;
    let site = build_site(&db, storage_root.path()).await;

    let repo_dir = storage_root.path().join(repo_id.to_string());
    tokio::fs::create_dir_all(&repo_dir)
        .await
        .expect("repo dir");

    // Simulate an unavailable backend while the database rows remain.
    let storage = site.get_storage(storage_id).expect("runtime storage");
    site.remove_storage(storage_id);

    let response = call_delete(&site, admin_auth(), storage_id, true)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["details"]["code"], "storage_backend_unavailable");
    assert_eq!(json["details"]["repositories_remaining"], 1);
    assert!(
        DBStorage::get_by_id(storage_id, db.pool())
            .await
            .expect("query")
            .is_some()
    );
    assert!(
        DBRepository::get_by_id(repo_id, db.pool())
            .await
            .expect("query")
            .is_some()
    );
    assert!(repo_dir.exists(), "physical contents must be retained");

    // Retry after the backend is available again.
    site.add_storage(storage_id, storage);
    let retry = call_delete(&site, admin_auth(), storage_id, true)
        .await
        .expect("handler ok");
    assert_eq!(retry.status(), StatusCode::NO_CONTENT);
    assert!(!repo_dir.exists());

    site.close().await;
}

#[tokio::test]
async fn delete_storage_cascade_leaves_other_storage_contents_intact() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root_one = tempfile::tempdir().expect("tempdir one");
    let root_two = tempfile::tempdir().expect("tempdir two");
    let storage_one = insert_local_storage(db.pool(), "primary", root_one.path()).await;
    let storage_two = insert_local_storage(db.pool(), "secondary", root_two.path()).await;
    let repo_one = insert_maven_repo(db.pool(), storage_one, "one").await;
    let repo_two = insert_maven_repo(db.pool(), storage_two, "two").await;
    let site = build_site(&db, root_one.path()).await;

    let dir_one = root_one.path().join(repo_one.to_string());
    let dir_two = root_two.path().join(repo_two.to_string());
    tokio::fs::create_dir_all(&dir_one).await.expect("dir one");
    tokio::fs::create_dir_all(&dir_two).await.expect("dir two");

    let response = call_delete(&site, admin_auth(), storage_one, true)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(!dir_one.exists());
    assert!(dir_two.exists(), "unrelated storage contents must remain");
    assert!(
        DBStorage::get_by_id(storage_two, db.pool())
            .await
            .expect("query")
            .is_some()
    );
    assert!(
        DBRepository::get_by_id(repo_two, db.pool())
            .await
            .expect("query")
            .is_some()
    );

    site.close().await;
}

#[tokio::test]
async fn delete_storage_cascade_removes_package_records() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let repo_id = insert_maven_repo(db.pool(), storage_id, "maven-hosted").await;
    let site = build_site(&db, storage_root.path()).await;

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (key, repository_id, path) VALUES ('demo', $1, 'demo') RETURNING id",
    )
    .bind(repo_id)
    .fetch_one(db.pool())
    .await
    .expect("insert project");
    let version_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_versions (project_id, repository_id, version, path) VALUES ($1, $2, '1.0.0', 'demo/1.0.0') RETURNING id",
    )
    .bind(project_id)
    .bind(repo_id)
    .fetch_one(db.pool())
    .await
    .expect("insert version");
    sqlx::query(
        "INSERT INTO package_files (repository_id, project_id, project_version_id, package, name, path, size_bytes, modified_at) \
         VALUES ($1, $2, $3, 'demo', '1.0.0', 'demo/1.0.0/demo-1.0.0.jar', 4, NOW())",
    )
    .bind(repo_id)
    .bind(project_id)
    .bind(version_id)
    .execute(db.pool())
    .await
    .expect("insert package file");

    let response = call_delete(&site, admin_auth(), storage_id, true)
        .await
        .expect("handler ok");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    for (table, id) in [
        ("projects", project_id),
        ("project_versions", version_id),
        ("package_files", version_id),
    ] {
        let column = if table == "package_files" {
            "project_version_id"
        } else {
            "id"
        };
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {column} = $1"))
                .bind(id)
                .fetch_one(db.pool())
                .await
                .expect("count");
        assert_eq!(count, 0, "{table} rows must be removed by cascade");
    }

    site.close().await;
}

#[tokio::test]
async fn delete_storage_cascade_reports_partial_failure_and_can_retry() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let docker = Cli::default();
    let minio = TestMinio::start(&docker).await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = NewDBStorage::new(
        "s3".into(),
        StorageName::new("primary".into()).expect("storage name"),
        minio.config(),
    )
    .insert(db.pool())
    .await
    .expect("insert S3 storage")
    .expect("storage row")
    .id;
    // Membership is processed in name order, so "a" is cleaned up before "b" fails.
    let repo_a = insert_maven_repo(db.pool(), storage_id, "a").await;
    let repo_b = insert_maven_repo(db.pool(), storage_id, "b").await;
    let site = build_site(&db, storage_root.path()).await;
    let storage = site.get_storage(storage_id).expect("runtime storage");
    let path = nr_core::storage::StoragePath::from("artifact.jar");
    for repository in [repo_a, repo_b] {
        storage
            .save_file(
                repository,
                nr_storage::FileContent::from(b"artifact"),
                &path,
            )
            .await
            .expect("upload artifact");
    }
    minio.deny_deletion(repo_b);

    let response = call_delete(&site, admin_auth(), storage_id, true)
        .await
        .expect("handler ok");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["details"]["code"], "storage_cleanup_failed");
    assert_eq!(json["details"]["repositories_remaining"], 2);
    assert_eq!(json["details"]["repository_id"], repo_b.to_string());
    assert!(
        json["details"]["detail"]
            .as_str()
            .expect("backend detail")
            .contains("AccessDenied")
    );
    assert!(
        json["message"]
            .as_str()
            .expect("message")
            .contains("retried"),
        "failure must explain that deletion can be retried"
    );

    // Cleanup ran for the first repository before the failure.
    assert_eq!(minio.objects(), vec![format!("{repo_b}/artifact.jar")]);

    // Database records and runtime registrations are retained for retry.
    assert!(
        DBStorage::get_by_id(storage_id, db.pool())
            .await
            .expect("query")
            .is_some()
    );
    assert!(
        DBRepository::get_by_id(repo_a, db.pool())
            .await
            .expect("query")
            .is_some()
    );
    assert!(
        DBRepository::get_by_id(repo_b, db.pool())
            .await
            .expect("query")
            .is_some()
    );
    assert!(site.get_storage(storage_id).is_some());
    assert!(site.get_repository(repo_a).is_some());
    assert!(site.get_repository(repo_b).is_some());

    // Fix the backend and retry.
    minio.allow_deletion();
    let retry = call_delete(&site, admin_auth(), storage_id, true)
        .await
        .expect("handler ok");
    assert_eq!(retry.status(), StatusCode::NO_CONTENT);
    assert!(minio.objects().is_empty());
    storage.unload().await.expect("unload S3 storage");
    site.close().await;
}

#[tokio::test]
async fn minio_deletion_fixture_is_removed_on_unwind() {
    let docker = Cli::default();
    let minio = TestMinio::start(&docker).await;
    minio.deny_deletion(Uuid::new_v4());
    let container_id = minio.id().to_string();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _fixture = minio;
        // Resume unwinding without invoking the panic hook for this expected panic.
        std::panic::resume_unwind(Box::new("exercise fixture cleanup"));
    }));
    assert!(result.is_err());

    let output = std::process::Command::new("docker")
        .args([
            "ps",
            "--all",
            "--quiet",
            "--filter",
            &format!("id={container_id}"),
        ])
        .output()
        .expect("inspect fixture cleanup");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(
        output.stdout.is_empty(),
        "MinIO must be removed during unwinding"
    );
}

#[tokio::test]
async fn minio_deletion_policy_only_blocks_the_selected_repository() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let docker = Cli::default();
    let minio = TestMinio::start(&docker).await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, storage_root.path()).await;
    let factory = site.get_storage_factory("s3").expect("S3 factory");
    let storage = factory
        .create_storage(StorageConfig {
            storage_config: StorageConfigInner::test_config(),
            type_config: serde_json::from_value(minio.config()).expect("S3 config"),
        })
        .await
        .expect("S3 storage");
    let repo_a = Uuid::new_v4();
    let repo_b = Uuid::new_v4();
    let path = nr_core::storage::StoragePath::from("artifact.jar");
    for repository in [repo_a, repo_b] {
        storage
            .save_file(
                repository,
                nr_storage::FileContent::from(b"artifact"),
                &path,
            )
            .await
            .expect("upload artifact");
    }
    minio.deny_deletion(repo_b);

    storage
        .delete_repository(repo_a)
        .await
        .expect("delete allowed repository");
    let error = storage
        .delete_repository(repo_b)
        .await
        .expect_err("denied repository deletion");
    assert!(error.to_string().contains("AccessDenied"), "{error}");
    assert_eq!(minio.objects(), vec![format!("{repo_b}/artifact.jar")]);

    minio.allow_deletion();
    storage
        .delete_repository(repo_b)
        .await
        .expect("retry repository deletion");
    assert!(minio.objects().is_empty());
    storage.unload().await.expect("unload S3 storage");
    site.close().await;
}

#[tokio::test]
async fn delete_storage_waits_for_management_lock() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), "primary", storage_root.path()).await;
    let site = build_site(&db, storage_root.path()).await;

    // Hold the management lock to emulate a concurrent creation/config update.
    let management = site.management_lock.lock().await;
    let handle = tokio::spawn(call_delete(&site, admin_auth(), storage_id, false));

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !handle.is_finished(),
        "deletion must wait for the management lock"
    );

    drop(management);
    let response = handle.await.expect("join").expect("handler ok");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(site.get_storage(storage_id).is_none());

    site.close().await;
}
