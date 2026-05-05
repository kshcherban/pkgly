#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::app::authentication::{AuthenticationRaw, session::Session};
use crate::app::config::{Mode, OAuth2ProviderKind, PasswordRules, SecuritySettings, SiteSetting};
use crate::test_support::DB_TEST_LOCK;
use axum::body::Body;
use http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use nr_core::{
    database::{DatabaseConfig, entities::user::NewUserRequest, migration::run_migrations},
    user::{Email, Username},
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use tower::ServiceExt;

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

async fn build_site(db: &TestDb, root: &std::path::Path, security: SecuritySettings) -> Pkgly {
    Pkgly::new(
        Mode::Debug,
        SiteSetting::default(),
        security,
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

fn sample_user(system_manager: bool) -> NewUserRequest {
    let suffix = if system_manager { "manager" } else { "regular" };
    NewUserRequest {
        name: "Test User".into(),
        username: Username::new(format!("test_{suffix}")).expect("username"),
        email: Email::new(format!("{suffix}@example.com")).expect("email"),
        password: None,
    }
}

async fn insert_user(pool: &PgPool, system_manager: bool) -> i32 {
    let user = sample_user(system_manager)
        .insert(pool)
        .await
        .expect("insert user");
    sqlx::query("UPDATE users SET system_manager = $1 WHERE id = $2")
        .bind(system_manager)
        .bind(user.id)
        .execute(pool)
        .await
        .expect("update user permissions");
    user.id
}

fn sample_session(user_id: i32) -> Session {
    let fixed_time =
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
    Session {
        user_id,
        session_id: format!("session-{user_id}"),
        user_agent: "test".into(),
        ip_address: "127.0.0.1".into(),
        expires: fixed_time,
        created: fixed_time,
    }
}

fn request_with_auth(method: Method, uri: &str, user_id: i32, body: Body) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(body)
        .expect("request");
    request
        .extensions_mut()
        .insert(AuthenticationRaw::Session(sample_session(user_id)));
    request
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

fn custom_password_rules() -> PasswordRules {
    PasswordRules {
        min_length: 12,
        require_uppercase: true,
        require_lowercase: false,
        require_number: true,
        require_symbol: false,
    }
}

#[test]
fn merge_google_settings_keeps_existing_secret_when_omitted() {
    let current = OAuth2GoogleConfig {
        client_id: "google-client".into(),
        client_secret: "super-secret".into(),
        scopes: vec!["openid".into(), "email".into()],
        redirect_path: None,
    };
    let request = Some(OAuth2ProviderSettingsRequest {
        client_id: "google-client".into(),
        client_secret: None,
        scopes: vec!["openid".into(), "email".into()],
        redirect_path: None,
    });

    let merged = merge_google_settings(Some(&current), request).expect("merge success");
    let new_config = merged.expect("google config");

    assert_eq!(new_config.client_id, "google-client");
    assert_eq!(new_config.client_secret, "super-secret");
    assert_eq!(
        new_config.scopes,
        vec!["openid".to_string(), "email".to_string()]
    );
}

#[test]
fn merge_google_settings_requires_secret_initially() {
    let request = Some(OAuth2ProviderSettingsRequest {
        client_id: "google-client".into(),
        client_secret: None,
        scopes: vec!["openid".into()],
        redirect_path: None,
    });

    let merged = merge_google_settings(None, request);

    assert!(merged.is_err());
}

#[test]
fn merge_oauth2_settings_sanitizes_paths_and_mappings() {
    let current = OAuth2Settings {
        enabled: false,
        login_path: "/api/user/oauth2/login".into(),
        callback_path: "/api/user/oauth2/callback".into(),
        redirect_base_url: None,
        auto_create_users: false,
        google: Some(OAuth2GoogleConfig {
            client_id: "google-client".into(),
            client_secret: "secret".into(),
            scopes: vec!["openid".into()],
            redirect_path: None,
        }),
        microsoft: None,
        casbin: None,
        group_role_mappings: vec![],
    };

    let request = OAuth2SettingsRequest {
        enabled: true,
        login_path: "oauth/login".into(),
        callback_path: "/oauth/callback".into(),
        redirect_base_url: Some("  ".into()),
        auto_create_users: true,
        google: Some(OAuth2ProviderSettingsRequest {
            client_id: "google-client".into(),
            client_secret: None,
            scopes: vec!["OpenID".into(), "email".into(), "".into()],
            redirect_path: Some("callback".into()),
        }),
        microsoft: None,
        casbin: None,
        group_role_mappings: vec![
            OAuth2GroupRoleMapping {
                provider: OAuth2ProviderKind::Google,
                group: "engineering".into(),
                roles: vec!["admin".into(), "admin".into(), " ".into()],
            },
            OAuth2GroupRoleMapping {
                provider: OAuth2ProviderKind::Google,
                group: "   ".into(),
                roles: vec!["ignored".into()],
            },
        ],
    };

    let merged =
        merge_oauth2_settings(Some(&current), request).expect("settings should merge safely");

    assert_eq!(merged.login_path, "/oauth/login");
    assert_eq!(merged.callback_path, "/oauth/callback");
    assert!(merged.redirect_base_url.is_none());
    assert!(merged.google.is_some());
    assert_eq!(merged.group_role_mappings.len(), 1);
    let mapping = &merged.group_role_mappings[0];
    assert_eq!(mapping.group, "engineering");
    assert_eq!(mapping.roles, vec!["admin"]);
    assert_eq!(
        merged
            .google
            .as_ref()
            .and_then(|cfg| cfg.redirect_path.clone())
            .as_deref(),
        Some("callback")
    );
    assert_eq!(
        merged.google.as_ref().map(|cfg| cfg.scopes.clone()),
        Some(vec!["OpenID".into(), "email".into()])
    );
}

#[test]
fn sanitize_sso_settings_trims_and_validates_providers() {
    let settings = SsoSettings {
        enabled: true,
        login_path: "sso/login".into(),
        login_button_text: "   ".into(),
        provider_login_url: Some(" https://login.example.com ".into()),
        provider_redirect_param: Some(" redirect ".into()),
        auto_create_users: true,
        providers: vec![OidcProviderConfig {
            name: " cloudflare ".into(),
            issuer: " https://issuer.example.com/ ".into(),
            audience: " pkgly ".into(),
            jwks_url: Some(" https://issuer.example.com/certs ".into()),
            token_source: TokenSource::Header {
                name: " Cf-Access-Jwt-Assertion ".into(),
                prefix: Some("Bearer ".into()),
            },
            subject_claim: Some(" preferred_username ".into()),
            email_claim: None,
            display_name_claim: Some(" name ".into()),
            role_claims: vec![" roles ".into()],
        }],
        role_claims: vec![" roles ".into(), "".into()],
    };

    let sanitized = sanitize_sso_settings(settings).expect("valid config");
    assert_eq!(sanitized.login_path, "/sso/login");
    assert_eq!(sanitized.login_button_text, "Sign in with SSO");
    assert_eq!(
        sanitized.provider_login_url.as_deref(),
        Some("https://login.example.com")
    );
    assert_eq!(
        sanitized.provider_redirect_param.as_deref(),
        Some("redirect")
    );
    assert_eq!(sanitized.providers.len(), 1);
    let provider = &sanitized.providers[0];
    assert_eq!(provider.name, "cloudflare");
    assert_eq!(provider.issuer, "https://issuer.example.com");
    assert_eq!(provider.audience, "pkgly");
    assert_eq!(
        provider.jwks_url.as_deref(),
        Some("https://issuer.example.com/certs")
    );
    assert_eq!(sanitized.role_claims, vec!["roles"]);
}

#[test]
fn sanitize_sso_settings_rejects_invalid_provider() {
    let settings = SsoSettings {
        enabled: true,
        login_path: "/api/user/sso/login".into(),
        login_button_text: "SSO".into(),
        provider_login_url: None,
        provider_redirect_param: None,
        auto_create_users: false,
        providers: vec![OidcProviderConfig {
            name: "".into(),
            issuer: "".into(),
            audience: "".into(),
            jwks_url: None,
            token_source: TokenSource::Cookie { name: "".into() },
            subject_claim: None,
            email_claim: None,
            display_name_claim: None,
            role_claims: vec![],
        }],
        role_claims: vec![],
    };

    let result = sanitize_sso_settings(settings);
    assert!(result.is_err());
}

#[tokio::test]
async fn get_password_rules_requires_system_manager() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path(), SecuritySettings::default()).await;
    let user_id = insert_user(db.pool(), false).await;

    let response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(request_with_auth(
            Method::GET,
            "/security/password-rules",
            user_id,
            Body::empty(),
        ))
        .await
        .expect("route");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    site.close().await;
}

#[tokio::test]
async fn get_password_rules_returns_configured_rules() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let rules = custom_password_rules();
    let security = SecuritySettings {
        password_rules: Some(rules),
        ..Default::default()
    };
    let site = build_site(&db, root.path(), security).await;
    let user_id = insert_user(db.pool(), true).await;

    let response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(request_with_auth(
            Method::GET,
            "/security/password-rules",
            user_id,
            Body::empty(),
        ))
        .await
        .expect("route");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await,
        json!({
            "min_length": 12,
            "require_uppercase": true,
            "require_lowercase": false,
            "require_number": true,
            "require_symbol": false
        })
    );
    site.close().await;
}

#[tokio::test]
async fn update_password_rules_persists_and_reflects_in_info() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path(), SecuritySettings::default()).await;
    let rules = custom_password_rules();
    let user_id = insert_user(db.pool(), true).await;
    let body = Body::from(serde_json::to_vec(&Some(rules)).expect("request body"));

    let response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(request_with_auth(
            Method::PUT,
            "/security/password-rules",
            user_id,
            body,
        ))
        .await
        .expect("route");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let stored: Value = sqlx::query_scalar("SELECT value FROM application_settings WHERE key = $1")
        .bind("security.password_rules")
        .fetch_one(db.pool())
        .await
        .expect("stored password rules");
    assert_eq!(
        stored,
        json!({
            "min_length": 12,
            "require_uppercase": true,
            "require_lowercase": false,
            "require_number": true,
            "require_symbol": false
        })
    );

    let info_response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/info")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("route");
    assert_eq!(info_response.status(), StatusCode::OK);
    let instance = body_json(info_response).await;
    assert_eq!(
        instance["password_rules"],
        json!({
            "min_length": 12,
            "require_uppercase": true,
            "require_lowercase": false,
            "require_number": true,
            "require_symbol": false
        })
    );
    site.close().await;
}

#[tokio::test]
async fn update_password_rules_rejects_empty_ruleset() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path(), SecuritySettings::default()).await;
    let empty_rules = PasswordRules {
        min_length: 0,
        require_uppercase: false,
        require_lowercase: false,
        require_number: false,
        require_symbol: false,
    };
    let user_id = insert_user(db.pool(), true).await;
    let body = Body::from(serde_json::to_vec(&Some(empty_rules)).expect("request body"));

    let response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(request_with_auth(
            Method::PUT,
            "/security/password-rules",
            user_id,
            body,
        ))
        .await
        .expect("route");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let stored: Option<Value> =
        sqlx::query_scalar("SELECT value FROM application_settings WHERE key = $1")
            .bind("security.password_rules")
            .fetch_optional(db.pool())
            .await
            .expect("stored password rules query");
    assert!(stored.is_none());
    site.close().await;
}

#[tokio::test]
async fn update_password_rules_allows_null_to_disable() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path(), SecuritySettings::default()).await;
    let user_id = insert_user(db.pool(), true).await;
    let body =
        Body::from(serde_json::to_vec(&Option::<PasswordRules>::None).expect("request body"));

    let response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(request_with_auth(
            Method::PUT,
            "/security/password-rules",
            user_id,
            body,
        ))
        .await
        .expect("route");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let stored: Value = sqlx::query_scalar("SELECT value FROM application_settings WHERE key = $1")
        .bind("security.password_rules")
        .fetch_one(db.pool())
        .await
        .expect("stored password rules");
    assert_eq!(stored, Value::Null);

    let get_response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(request_with_auth(
            Method::GET,
            "/security/password-rules",
            user_id,
            Body::empty(),
        ))
        .await
        .expect("route");
    assert_eq!(get_response.status(), StatusCode::OK);
    assert_eq!(body_json(get_response).await, Value::Null);

    let info_response = crate::app::api::api_routes()
        .with_state(site.clone())
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/info")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("route");
    assert_eq!(info_response.status(), StatusCode::OK);
    assert_eq!(
        body_json(info_response).await["password_rules"],
        Value::Null
    );

    let reloaded_root = tempfile::tempdir().expect("tempdir");
    let reloaded_site = build_site(&db, reloaded_root.path(), SecuritySettings::default()).await;
    let reloaded_response = crate::app::api::api_routes()
        .with_state(reloaded_site.clone())
        .oneshot(request_with_auth(
            Method::GET,
            "/security/password-rules",
            user_id,
            Body::empty(),
        ))
        .await
        .expect("route");
    assert_eq!(reloaded_response.status(), StatusCode::OK);
    assert_eq!(body_json(reloaded_response).await, Value::Null);

    site.close().await;
    reloaded_site.close().await;
}

#[test]
fn password_rules_enforces_min_length() {
    let rules = PasswordRules {
        min_length: 8,
        require_uppercase: false,
        require_lowercase: false,
        require_number: false,
        require_symbol: false,
    };

    assert!(rules.validate("12345678"));
    assert!(!rules.validate("1234567"));
    assert!(!rules.validate(""));
}

#[test]
fn password_rules_enforces_uppercase() {
    let rules = PasswordRules {
        min_length: 1,
        require_uppercase: true,
        require_lowercase: false,
        require_number: false,
        require_symbol: false,
    };

    assert!(rules.validate("Abc"));
    assert!(!rules.validate("abc"));
}

#[test]
fn password_rules_enforces_lowercase() {
    let rules = PasswordRules {
        min_length: 1,
        require_uppercase: false,
        require_lowercase: true,
        require_number: false,
        require_symbol: false,
    };

    assert!(rules.validate("aBC"));
    assert!(!rules.validate("ABC"));
}

#[test]
fn password_rules_enforces_number() {
    let rules = PasswordRules {
        min_length: 1,
        require_uppercase: false,
        require_lowercase: false,
        require_number: true,
        require_symbol: false,
    };

    assert!(rules.validate("a1"));
    assert!(!rules.validate("ab"));
}

#[test]
fn password_rules_enforces_symbol() {
    let rules = PasswordRules {
        min_length: 1,
        require_uppercase: false,
        require_lowercase: false,
        require_number: false,
        require_symbol: true,
    };

    assert!(rules.validate("a!"));
    assert!(!rules.validate("ab"));
}

#[test]
fn password_rules_enforces_all_flags() {
    let rules = PasswordRules {
        min_length: 4,
        require_uppercase: true,
        require_lowercase: true,
        require_number: true,
        require_symbol: true,
    };

    assert!(rules.validate("Ab1!"));
    assert!(rules.validate("Longer1!"));
    assert!(!rules.validate("short"));
    assert!(!rules.validate("NoNumbers!"));
}

#[test]
fn password_rules_accepts_any_when_no_constraints() {
    let rules = PasswordRules {
        min_length: 0,
        require_uppercase: false,
        require_lowercase: false,
        require_number: false,
        require_symbol: false,
    };

    assert!(rules.validate(""));
    assert!(rules.validate("anything"));
}
