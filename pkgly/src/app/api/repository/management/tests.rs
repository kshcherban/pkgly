#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use super::*;

use crate::repository::NewRepository;
use crate::test_support::DB_TEST_LOCK;
use http::StatusCode;
use nr_core::{
    database::{DatabaseConfig, entities::storage::NewDBStorage, migration::run_migrations},
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
        email: Email::new("user@example.com".into()).expect("email"),
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

#[tokio::test]
async fn deb_refresh_requires_edit_permission() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");

    let storage_id = insert_local_storage(db.pool(), storage_root.path()).await;
    let repo_id = insert_deb_proxy_repo(db.pool(), storage_id).await;

    let cfg = DatabaseConfig {
        user: "postgres".into(),
        password: "password".into(),
        database: "postgres".into(),
        host: "127.0.0.1".into(),
        port: Some(db.port),
    };

    let site = Pkgly::new(
        crate::app::config::Mode::Debug,
        crate::app::config::SiteSetting::default(),
        crate::app::config::SecuritySettings::default(),
        crate::app::authentication::session::SessionManagerConfig {
            database_location: storage_root.path().join("sessions.redb"),
            ..Default::default()
        },
        crate::repository::StagingConfig {
            staging_dir: storage_root.path().join("staging"),
            ..Default::default()
        },
        None,
        cfg,
        Some(storage_root.path().join("storages")),
    )
    .await
    .expect("create site");

    let auth = Authentication::AuthToken(sample_auth_token(1), sample_user(1, false));
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

    let cfg = DatabaseConfig {
        user: "postgres".into(),
        password: "password".into(),
        database: "postgres".into(),
        host: "127.0.0.1".into(),
        port: Some(db.port),
    };

    let site = Pkgly::new(
        crate::app::config::Mode::Debug,
        crate::app::config::SiteSetting::default(),
        crate::app::config::SecuritySettings::default(),
        crate::app::authentication::session::SessionManagerConfig {
            database_location: storage_root.path().join("sessions.redb"),
            ..Default::default()
        },
        crate::repository::StagingConfig {
            staging_dir: storage_root.path().join("staging"),
            ..Default::default()
        },
        None,
        cfg,
        Some(storage_root.path().join("storages")),
    )
    .await
    .expect("create site");

    let mut conn = sqlx::PgConnection::connect(&db.url).await.expect("connect");
    let key = crate::repository::deb::refresh_status::deb_proxy_refresh_advisory_key(repo_id);
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(key)
        .execute(&mut conn)
        .await
        .expect("lock acquired");

    let auth = Authentication::AuthToken(sample_auth_token(1), sample_user(1, true));
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
