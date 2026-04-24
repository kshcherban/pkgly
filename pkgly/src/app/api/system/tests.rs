#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::test_support::DB_TEST_LOCK;
use axum::extract::State;
use http::StatusCode;
use http_body_util::BodyExt;
use nr_core::{
    database::{DatabaseConfig, migration::run_migrations},
    user::{Email, Username, permissions::RepositoryActions},
};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};

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

async fn start_postgres() -> TestDb {
    let docker: &'static Cli = Box::leak(Box::new(Cli::default()));
    let image = GenericImage::new("postgres", "18-alpine")
        .with_env_var("POSTGRES_PASSWORD", "password")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres");
    let container = docker.run(image);
    let port = container.get_host_port_ipv4(5432);
    let url = format!("postgres://postgres:password@127.0.0.1:{port}/postgres");

    for _ in 0..60 {
        if let Ok(pool) = PgPoolOptions::new().max_connections(4).connect(&url).await {
            return TestDb {
                pool,
                port,
                _container: container,
                _docker: docker,
            };
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    panic!("postgres container did not become ready");
}

async fn fresh_db() -> TestDb {
    let db = start_postgres().await;
    run_migrations(db.pool()).await.expect("run migrations");
    db
}

async fn build_site(db: &TestDb, root: &std::path::Path) -> Pkgly {
    Pkgly::new(
        crate::app::config::Mode::Debug,
        crate::app::config::SiteSetting::default(),
        crate::app::config::SecuritySettings::default(),
        crate::app::authentication::session::SessionManagerConfig {
            database_location: root.join("sessions.redb"),
            ..Default::default()
        },
        crate::repository::StagingConfig {
            staging_dir: root.join("staging"),
            ..Default::default()
        },
        None,
        DatabaseConfig {
            user: "postgres".into(),
            password: "password".into(),
            database: "postgres".into(),
            host: "127.0.0.1".into(),
            port: Some(db.port),
        },
        Some(root.join("storages")),
    )
    .await
    .expect("create site")
}

fn sample_user(
    user_id: i32,
    system_manager: bool,
) -> nr_core::database::entities::user::UserSafeData {
    let fixed_time =
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
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

fn sample_auth(user_id: i32, system_manager: bool) -> Authentication {
    let fixed_time =
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
    let token = nr_core::database::entities::user::auth_token::AuthToken {
        id: 1,
        user_id,
        name: Some("token".into()),
        description: None,
        token: "token".into(),
        active: true,
        source: "test".into(),
        expires_at: None,
        created_at: fixed_time,
    };
    Authentication::AuthToken(token, sample_user(user_id, system_manager))
}

async fn body_json(response: Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn list_webhooks_requires_system_manager() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path()).await;

    let response = list_webhooks(sample_auth(1, false), State(site.clone()))
        .await
        .expect("handler");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    site.close().await;
}

#[tokio::test]
async fn webhook_crud_redacts_headers_and_preserves_secret_on_update() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path()).await;
    let auth = sample_auth(1, true);

    let create_response = create_webhook(
        auth.clone(),
        State(site.clone()),
        Json(WebhookUpsertRequest {
            name: "packages".into(),
            enabled: true,
            target_url: "https://example.com/hooks".into(),
            events: vec![WebhookEventType::PackagePublished],
            headers: vec![WebhookHeaderRequest {
                name: "X-Token".into(),
                value: Some("secret-value".into()),
                configured: false,
            }],
        }),
    )
    .await
    .expect("create handler");

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = body_json(create_response).await;
    let webhook_id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["headers"][0]["name"], "X-Token");
    assert_eq!(created["headers"][0]["configured"], true);
    assert!(created["headers"][0].get("value").is_none());

    let get_response = get_webhook(
        auth.clone(),
        State(site.clone()),
        Path(Uuid::parse_str(&webhook_id).expect("uuid")),
    )
    .await
    .expect("get handler");
    let fetched = body_json(get_response).await;
    assert_eq!(fetched["headers"][0]["name"], "X-Token");
    assert!(fetched["headers"][0].get("value").is_none());

    let update_response = update_webhook(
        auth.clone(),
        State(site.clone()),
        Path(Uuid::parse_str(&webhook_id).expect("uuid")),
        Json(WebhookUpsertRequest {
            name: "packages-updated".into(),
            enabled: true,
            target_url: "https://example.com/hooks/v2".into(),
            events: vec![
                WebhookEventType::PackagePublished,
                WebhookEventType::PackageDeleted,
            ],
            headers: vec![WebhookHeaderRequest {
                name: "X-Token".into(),
                value: None,
                configured: true,
            }],
        }),
    )
    .await
    .expect("update handler");

    assert_eq!(update_response.status(), StatusCode::OK);
    let stored_headers: Value = sqlx::query_scalar("SELECT headers FROM webhooks WHERE id = $1")
        .bind(Uuid::parse_str(&webhook_id).expect("uuid"))
        .fetch_one(db.pool())
        .await
        .expect("headers query");
    assert_eq!(stored_headers["X-Token"], "secret-value");

    let delete_response = delete_webhook(
        auth,
        State(site.clone()),
        Path(Uuid::parse_str(&webhook_id).expect("uuid")),
    )
    .await
    .expect("delete handler");

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);
    site.close().await;
}

#[test]
fn openapi_registers_webhook_routes() {
    let document = crate::app::open_api::ApiDoc::openapi();
    assert!(document.paths.paths.contains_key("/api/system/webhooks"));
    assert!(
        document
            .paths
            .paths
            .contains_key("/api/system/webhooks/{id}")
    );
}
