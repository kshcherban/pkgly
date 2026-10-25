// ABOUTME: Exercises repository management against real PostgreSQL and local storage.
// ABOUTME: Verifies repository creation, scoped permissions, and refresh locking.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use super::*;

use crate::repository::NewRepository;
use crate::test_support::DB_TEST_LOCK;
use http::StatusCode;
use nr_core::{
    database::{
        DatabaseConfig,
        entities::{
            repository::DBRepository,
            storage::{DBStorage, NewDBStorage},
        },
        migration::run_migrations,
    },
    storage::StorageName,
};
use sqlx::{Connection, PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    url: String,
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

    Pkgly::new(
        crate::app::config::Mode::Debug,
        crate::app::config::SiteSetting::default(),
        crate::app::config::SecuritySettings::default(),
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
                    url,
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

async fn insert_local_storage(pool: &PgPool, root: &std::path::Path) -> Uuid {
    let storage_name = StorageName::new("primary".to_string()).expect("storage name");
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

async fn insert_deb_proxy_repo(pool: &PgPool, storage_id: Uuid) -> Uuid {
    let repo_id = Uuid::new_v4();
    let repo = NewRepository {
        name: "deb-proxy-test".into(),
        uuid: repo_id,
        repository_type: "deb".into(),
        configs: ahash::HashMap::from_iter([(
            "deb".to_string(),
            serde_json::json!({
                "type": "proxy",
                "config": {
                    "upstream_url": "http://example.invalid",
                    "layout": {
                        "type": "flat",
                        "config": {
                            "distribution": "./",
                            "architectures": []
                        }
                    }
                }
            }),
        )]),
    };
    repo.insert(storage_id, pool).await.expect("insert repo").id
}

async fn sample_auth(pool: &PgPool, system_manager: bool) -> Authentication {
    use nr_core::{
        database::entities::user::{
            UserSafeData,
            auth_token::{AuthToken, NewAuthToken},
        },
        user::scopes::NRScope,
    };
    let user: UserSafeData = sqlx::query_as("INSERT INTO users (name,username,email,active,system_manager,default_repository_actions) VALUES ('Test User','test_user','user@example.com',true,$1,ARRAY['Read']::text[]) RETURNING *")
        .bind(system_manager).fetch_one(pool).await.expect("insert user");
    let (_, raw) = NewAuthToken {
        user_id: user.id,
        source: "test".into(),
        scopes: vec![
            NRScope::ReadRepository,
            NRScope::WriteRepository,
            NRScope::EditRepository,
        ],
        ..Default::default()
    }
    .insert(pool)
    .await
    .expect("insert scoped token");
    let token = AuthToken::get_by_token(&raw, pool)
        .await
        .expect("load token")
        .expect("token exists");
    Authentication::AuthToken(token, user)
}

#[tokio::test]
async fn deb_refresh_requires_edit_permission() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");

    let storage_id = insert_local_storage(db.pool(), storage_root.path()).await;
    let repo_id = insert_deb_proxy_repo(db.pool(), storage_id).await;

    let site = build_site(&db, storage_root.path()).await;

    let auth = sample_auth(db.pool(), false).await;
    let response = deb_refresh(
        axum::extract::State(site.clone()),
        auth,
        axum::extract::Path(repo_id),
    )
    .await
    .expect("handler ok");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    site.close().await;
}

#[tokio::test]
async fn deb_refresh_returns_conflict_when_advisory_lock_is_held() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");

    let storage_id = insert_local_storage(db.pool(), storage_root.path()).await;
    let repo_id = insert_deb_proxy_repo(db.pool(), storage_id).await;

    let site = build_site(&db, storage_root.path()).await;

    let mut conn = sqlx::PgConnection::connect(&db.url).await.expect("connect");
    let key = crate::repository::deb::refresh_status::deb_proxy_refresh_advisory_key(repo_id);
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(key)
        .execute(&mut conn)
        .await
        .expect("lock acquired");

    let auth = sample_auth(db.pool(), true).await;
    let response = deb_refresh(
        axum::extract::State(site.clone()),
        auth,
        axum::extract::Path(repo_id),
    )
    .await
    .expect("handler ok");

    // Expected once deb refresh uses an advisory lock. Without it this will attempt a refresh and
    // likely fail upstream fetch with 5xx.
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(key)
        .fetch_one(&mut conn)
        .await
        .expect("unlock");
    assert!(unlocked);
    conn.close().await.expect("close");
    site.close().await;
}

#[test]
fn new_repository_request_deserializes_storage_name() {
    let request: NewRepositoryRequest = serde_json::from_value(serde_json::json!({
        "name": "maven-hosted",
        "storage_name": "primary",
        "configs": {
            "maven": {
                "type": "Hosted"
            }
        }
    }))
    .expect("request should deserialize");

    assert_eq!(request.storage, None);
    assert_eq!(
        request.storage_name,
        Some(StorageName::new("primary".to_string()).expect("storage name"))
    );
}

#[tokio::test]
async fn new_repository_accepts_storage_name_and_defaults_to_hosted() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");

    let storage_id = insert_local_storage(db.pool(), storage_root.path()).await;
    let site = build_site(&db, storage_root.path()).await;

    let auth = sample_auth(db.pool(), true).await;
    let request = NewRepositoryRequest {
        name: "maven-hosted".into(),
        storage: None,
        storage_name: Some(StorageName::new("primary".to_string()).expect("storage name")),
        configs: ahash::HashMap::default(),
    };

    let response = new_repository(
        axum::extract::State(site.clone()),
        auth,
        axum::extract::Path("maven".to_string()),
        axum::Json(request),
    )
    .await
    .expect("handler ok");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .expect("creation body")
        .to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).expect("creation JSON");
    assert_eq!(created["storage_name"], "primary");

    let repository =
        DBRepository::get_id_from_storage_and_name("primary", "maven-hosted", db.pool())
            .await
            .expect("lookup repository")
            .expect("repository inserted");
    assert_eq!(repository.storage_id, storage_id);

    let storage = DBStorage::get_by_id(storage_id, db.pool())
        .await
        .expect("query storage")
        .expect("storage exists");
    assert_eq!(storage.name.as_ref(), "primary");

    site.close().await;
}
