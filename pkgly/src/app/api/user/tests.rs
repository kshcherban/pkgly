#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use axum_extra::extract::cookie::Cookie;
use chrono::{DateTime, FixedOffset};
use http_body_util::BodyExt;
use nr_core::{
    database::{
        DatabaseConfig,
        entities::user::{NewUserRequest, auth_token::AuthToken},
        migration::run_migrations,
    },
    user::{Email, Username, permissions::RepositoryActions},
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};

use crate::test_support::DB_TEST_LOCK;

fn fixed_time() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").unwrap()
}

fn sample_user() -> UserSafeData {
    UserSafeData {
        id: 1,
        name: "Test User".into(),
        username: Username::new("test_user".into()).unwrap(),
        email: Email::new("user@example.com".into()).unwrap(),
        require_password_change: false,
        active: true,
        admin: false,
        user_manager: false,
        system_manager: false,
        default_repository_actions: vec![RepositoryActions::Read],
        updated_at: fixed_time(),
        created_at: fixed_time(),
    }
}

fn sample_session() -> Session {
    Session {
        user_id: 1,
        session_id: "session-id".into(),
        user_agent: "agent".into(),
        ip_address: "127.0.0.1".into(),
        expires: fixed_time(),
        created: fixed_time(),
    }
}

fn sample_session_for_user(user_id: i32) -> Session {
    Session { user_id, ..sample_session() }
}

fn sample_auth_token() -> AuthToken {
    AuthToken {
        id: 1,
        user_id: 1,
        name: Some("token".into()),
        description: None,
        token: "token".into(),
        active: true,
        source: "test".into(),
        expires_at: None,
        created_at: fixed_time(),
    }
}

fn sample_me_with_session() -> MeWithSession {
    MeWithSession::from((sample_session(), sample_user()))
}

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

async fn test_site(db: &TestDb, root: &std::path::Path) -> Pkgly {
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

async fn insert_user(
    pool: &PgPool,
    name: &str,
    username: &str,
    email: &str,
) -> nr_core::database::entities::user::UserSafeData {
    NewUserRequest {
        name: name.to_string(),
        username: Username::new(username.to_string()).expect("username"),
        email: Email::new(email.to_string()).expect("email"),
        password: None,
    }
    .insert(pool)
    .await
    .expect("insert user")
    .into()
}

#[tokio::test]
async fn me_returns_bad_request_for_auth_token() {
    let response = me(Authentication::AuthToken(
        sample_auth_token(),
        sample_user(),
    ))
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        "Use whoami instead of me for Auth Tokens"
    );
}

#[tokio::test]
async fn login_response_sets_cookie_and_body() {
    let cookie = Cookie::build(("session", "abc123"))
        .secure(true)
        .path("/")
        .build();
    let me = sample_me_with_session();
    let cookie_value = cookie.encoded().to_string();

    let response = login_success_response(cookie.clone(), me.clone());

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get(SET_COOKIE)
            .and_then(|value| value.to_str().ok()),
        Some(cookie_value.as_str())
    );
    assert_eq!(
        headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );

    let collected = response.into_body().collect().await.unwrap();
    let body = collected.to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload, json!(me));
}

#[tokio::test]
async fn change_email_updates_current_session_user() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = test_site(&db, root.path()).await;
    let user = insert_user(db.pool(), "Test User", "test_user", "user@example.com").await;

    let response = change_email(
        Authentication::Session(sample_session_for_user(user.id), user.clone()),
        axum::extract::State(site.clone()),
        Json(ChangeEmailRequest {
            email: "updated@example.com".into(),
        }),
    )
    .await
    .expect("handler ok");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response.into_body().collect().await.expect("body").to_bytes();
    let updated: UserSafeData = serde_json::from_slice(&payload).expect("updated user");
    assert_eq!(updated.email.to_string(), "updated@example.com");

    let stored = UserSafeData::get_by_id(user.id, db.pool())
        .await
        .expect("lookup")
        .expect("user");
    assert_eq!(stored.email.to_string(), "updated@example.com");

    site.close().await;
}

#[tokio::test]
async fn change_email_returns_conflict_when_email_is_taken() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = test_site(&db, root.path()).await;
    let user = insert_user(db.pool(), "Test User", "test_user", "user@example.com").await;
    let _other = insert_user(db.pool(), "Other User", "other_user", "taken@example.com").await;

    let response = change_email(
        Authentication::Session(sample_session_for_user(user.id), user),
        axum::extract::State(site.clone()),
        Json(ChangeEmailRequest {
            email: "taken@example.com".into(),
        }),
    )
    .await
    .expect("handler ok");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let payload: serde_json::Value = serde_json::from_slice(
        &response.into_body().collect().await.expect("body").to_bytes(),
    )
    .expect("json");
    assert_eq!(payload["details"], "email");

    site.close().await;
}

#[tokio::test]
async fn change_email_rejects_auth_token_requests() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = test_site(&db, root.path()).await;

    let response = change_email(
        Authentication::AuthToken(sample_auth_token(), sample_user()),
        axum::extract::State(site.clone()),
        Json(ChangeEmailRequest {
            email: "updated@example.com".into(),
        }),
    )
    .await
    .expect("handler ok");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.expect("body").to_bytes();
    assert_eq!(std::str::from_utf8(&body).expect("utf8"), "Must be a session");

    site.close().await;
}
