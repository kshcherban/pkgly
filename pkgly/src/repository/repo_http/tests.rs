// ABOUTME: Tests repository HTTP authentication, routing, and audit behavior.
// ABOUTME: Exercises repository permission checks against real database records.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::app::authentication::session::SessionManagerConfig;
use crate::app::config::{Mode, SecuritySettings, SiteSetting};
use crate::repository::StagingConfig;
use nr_core::{
    database::DatabaseConfig,
    database::entities::storage::NewDBStorage,
    database::entities::user::NewUserRequest,
    database::{
        entities::user::permissions::NewUserRepositoryPermissions, migration::run_migrations,
    },
    repository::Visibility,
    repository::config::RepositoryConfigType,
    storage::StorageName,
    user::{
        Email, Username,
        permissions::{HasPermissions, InitialUserPermissions, RepositoryActions},
    },
};
use once_cell::sync::Lazy;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

static DB_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

struct TestDb {
    pool: PgPool,
    port: u16,
    _container: Container<'static, GenericImage>,
    _docker: &'static Cli,
}

async fn fresh_db() -> TestDb {
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
                run_migrations(&pool).await.expect("run migrations");
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

#[test]
fn docker_v2_ok_response_sets_headers() {
    let response = docker_v2_ok_response();
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("Docker-Distribution-API-Version")
            .and_then(|value| value.to_str().ok()),
        Some(DOCKER_API_VERSION)
    );
    assert_eq!(
        headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some(DOCKER_JSON_CONTENT_TYPE)
    );
}

#[test]
fn docker_v2_unauthorized_response_sets_challenge() {
    let response = docker_v2_unauthorized_response("Bearer realm=\"test\"", "{}");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("WWW-Authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer realm=\"test\"")
    );
}

#[test]
fn www_authenticate_response_sets_header_and_body() {
    let response = RepoResponse::www_authenticate("Basic realm=\"Pkgly\"").into_response_default();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("WWW-Authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Pkgly\"")
    );
}

#[test]
fn forbidden_response_returns_expected_status_and_message() {
    let response = RepoResponse::forbidden().into_response_default();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn unsupported_method_response_mentions_method() {
    let response =
        RepoResponse::unsupported_method_response(Method::POST, "docker").into_response_default();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[test]
fn npm_proxy_reads_do_not_require_auth_even_when_enabled() {
    let auth = RepositoryAuthConfig { enabled: true };
    let requires = super::should_require_auth(&auth, true, false, true);
    assert!(
        !requires,
        "proxy/virtual npm GET should allow anonymous access"
    );
}

#[test]
fn npm_proxy_writes_still_require_auth() {
    let auth = RepositoryAuthConfig { enabled: true };
    let requires = super::should_require_auth(&auth, false, false, true);
    assert!(requires, "non-read operations must remain protected");
}

#[test]
fn non_npm_repos_honor_auth_enabled_for_reads() {
    let auth = RepositoryAuthConfig { enabled: true };
    let requires = super::should_require_auth(&auth, true, false, false);
    assert!(
        requires,
        "other repositories should respect auth toggle for reads"
    );
}

#[test]
fn npm_login_paths_bypass_auth() {
    let auth = RepositoryAuthConfig { enabled: true };
    let requires = super::should_require_auth(&auth, true, true, false);
    assert!(!requires, "npm login endpoints must remain open");
}

#[test]
fn classify_repo_audit_action_marks_write_operations() {
    let action =
        super::classify_repo_audit_action(&Method::PUT, &StoragePath::from("crate/file.tgz"), None);
    assert_eq!(action, "package.upload");
}

#[test]
fn classify_repo_audit_action_marks_delete_operations() {
    let action = super::classify_repo_audit_action(
        &Method::DELETE,
        &StoragePath::from("crate/file.tgz"),
        None,
    );
    assert_eq!(action, "package.delete");
}

#[test]
fn classify_repo_audit_action_marks_directory_reads_as_list() {
    let action =
        super::classify_repo_audit_action(&Method::GET, &StoragePath::from("simple/"), None);
    assert_eq!(action, "package.list");
}

#[test]
fn classify_repo_audit_action_marks_file_reads_as_download() {
    let action = super::classify_repo_audit_action(
        &Method::GET,
        &StoragePath::from("crates/foo-1.0.0.crate"),
        None,
    );
    assert_eq!(action, "package.download");
}

#[tokio::test]
async fn virtual_repository_auth_checks_read_permission_on_virtual_repo() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;

    let virtual_repo_id = Uuid::new_v4();
    let member_repo_id = Uuid::new_v4();
    let user = NewUserRequest {
        name: "Virtual Test".to_string(),
        username: Username::new("virtual-test".to_string()).expect("username"),
        email: Some(Email::new("virtual-test@example.invalid".to_string()).expect("email")),
        password: Some("password".to_string()),
        permissions: None,
    }
    .insert(&db.pool)
    .await
    .expect("insert user");

    let auth = RepositoryAuthentication::Basic(None, user.into());

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let storage_name = StorageName::new("primary".to_string()).expect("storage name");
    let storage = NewDBStorage::new(
        "Local".into(),
        storage_name,
        serde_json::json!({
            "type": "Local",
            "settings": {
                "path": temp_dir.path().to_string_lossy()
            }
        }),
    )
    .insert(&db.pool)
    .await
    .expect("insert storage")
    .expect("storage row");

    sqlx::query(
        r#"INSERT INTO repositories (id, storage_id, name, repository_type, active)
           VALUES ($1, $2, $3, $4, true)"#,
    )
    .bind(virtual_repo_id)
    .bind(storage.id)
    .bind("virtual")
    .bind("python")
    .execute(&db.pool)
    .await
    .expect("insert virtual repository");

    sqlx::query(
        r#"INSERT INTO repositories (id, storage_id, name, repository_type, active)
           VALUES ($1, $2, $3, $4, true)"#,
    )
    .bind(member_repo_id)
    .bind(storage.id)
    .bind("member")
    .bind("python")
    .execute(&db.pool)
    .await
    .expect("insert member repository");

    NewUserRepositoryPermissions {
        user_id: auth.get_user_id().expect("user id"),
        repository_id: virtual_repo_id,
        actions: vec![RepositoryActions::Read],
    }
    .insert(&db.pool)
    .await
    .expect("insert repo permissions");

    let plain = crate::repository::utils::can_read_repository(
        &auth,
        Visibility::Private,
        member_repo_id,
        &db.pool,
    )
    .await
    .expect("permission check");
    assert!(
        !plain,
        "expected member repo read to be denied without explicit member permissions"
    );

    let wrapped = auth.wrap_for_virtual_reads(virtual_repo_id);
    let allowed = crate::repository::utils::can_read_repository(
        &wrapped,
        Visibility::Private,
        member_repo_id,
        &db.pool,
    )
    .await
    .expect("permission check");
    assert!(
        allowed,
        "expected read permission to be granted via virtual repository permissions"
    );
}

#[tokio::test]
async fn default_repository_actions_grants_basic_auth_read() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;

    let repo_id = Uuid::new_v4();
    let user = NewUserRequest {
        name: "Default Read User".to_string(),
        username: Username::new("default-read-user".to_string()).expect("username"),
        email: Some(Email::new("default-read@example.invalid".to_string()).expect("email")),
        password: Some("password".to_string()),
        permissions: Some(InitialUserPermissions {
            admin: false,
            user_manager: false,
            system_manager: false,
            default_repository_actions: vec![RepositoryActions::Read],
        }),
    }
    .insert(&db.pool)
    .await
    .expect("insert user");

    let auth = RepositoryAuthentication::Basic(None, user.into());

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let storage_name = StorageName::new("primary".to_string()).expect("storage name");
    let storage = NewDBStorage::new(
        "Local".into(),
        storage_name,
        serde_json::json!({
            "type": "Local",
            "settings": {
                "path": temp_dir.path().to_string_lossy()
            }
        }),
    )
    .insert(&db.pool)
    .await
    .expect("insert storage")
    .expect("storage row");

    sqlx::query(
        "INSERT INTO repositories (id, storage_id, name, repository_type, active) VALUES ($1, $2, $3, $4, true)",
    )
    .bind(repo_id)
    .bind(storage.id)
    .bind("private-repo")
    .bind("npm")
    .execute(&db.pool)
    .await
    .expect("insert repository");

    let can_read = crate::repository::utils::can_read_repository(
        &auth,
        Visibility::Private,
        repo_id,
        &db.pool,
    )
    .await
    .expect("permission check");
    assert!(
        can_read,
        "default Read should grant read access on private repo"
    );

    let can_write = auth
        .has_action(RepositoryActions::Write, repo_id, &db.pool)
        .await
        .expect("permission check");
    assert!(!can_write, "default Read should not grant write access");

    NewUserRepositoryPermissions {
        user_id: auth.get_user_id().expect("user id"),
        repository_id: repo_id,
        actions: vec![RepositoryActions::Write],
    }
    .insert(&db.pool)
    .await
    .expect("insert repo permissions");

    let can_read_after_explicit = crate::repository::utils::can_read_repository(
        &auth,
        Visibility::Private,
        repo_id,
        &db.pool,
    )
    .await
    .expect("permission check");
    assert!(
        !can_read_after_explicit,
        "explicit Write-only should override default Read"
    );

    let can_write_after_explicit = auth
        .has_action(RepositoryActions::Write, repo_id, &db.pool)
        .await
        .expect("permission check");
    assert!(
        can_write_after_explicit,
        "explicit Write should grant write access"
    );
}

#[tokio::test]
async fn docker_v2_catchall_401_uses_request_host_for_realm() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");

    // Create a storage
    let storage_name = StorageName::new("default".to_string()).expect("storage name");
    let storage = NewDBStorage::new(
        "Local".into(),
        storage_name,
        serde_json::json!({
            "type": "Local",
            "settings": {
                "path": root.path().join("storages").to_string_lossy()
            }
        }),
    )
    .insert(&db.pool)
    .await
    .expect("insert storage")
    .expect("storage row");

    // Create a Docker repository with auth enabled (default) and hosted config
    use crate::repository::NewRepository;
    use crate::repository::docker::configs::{DockerRegistryConfig, DockerRegistryConfigType};
    use ahash::HashMap;
    let mut configs = HashMap::with_hasher(Default::default());
    configs.insert(
        DockerRegistryConfigType::get_type_static().to_string(),
        serde_json::to_value(DockerRegistryConfig::Hosted).expect("serialize docker config"),
    );
    let repo = NewRepository {
        name: "test".into(),
        uuid: Uuid::new_v4(),
        repository_type: "docker".into(),
        configs,
    };
    let _repo_id = repo
        .insert(storage.id, &db.pool)
        .await
        .expect("insert docker repository");

    // Create the Pkgly instance (loads storages and repos from DB)
    let site = Pkgly::new(
        Mode::Debug,
        SiteSetting {
            app_url: None, // Force fallback to Host header
            ..Default::default()
        },
        SecuritySettings::default(),
        SessionManagerConfig {
            database_location: root.path().join("sessions.redb"),
            ..Default::default()
        },
        StagingConfig {
            staging_dir: root.path().join("staging"),
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
        Some(root.path().join("storages")),
    )
    .await
    .expect("create Pkgly instance");

    // Build a request with a non-default Host header
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri("http://test.example.com:8080/v2/default/test/manifests/latest")
        .header("Host", "test.example.com:8080")
        .body(axum::body::Body::empty())
        .expect("build request");

    // Call the Docker V2 handler directly
    let response = handle_docker_v2_any_path(
        axum::extract::Path("default/test/manifests/latest".to_string()),
        axum::extract::State(site),
        None,
        RepositoryAuthentication::NoIdentification,
        request,
    )
    .await
    .expect("handler returned error");

    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);

    let www_auth = response
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .expect("www-authenticate header");

    assert!(
        www_auth.contains("realm=\"http://test.example.com:8080/v2/token\""),
        "expected realm to use Host header, got: {www_auth}"
    );
    assert!(
        www_auth.contains("service=\"test.example.com:8080\""),
        "expected service to use Host header, got: {www_auth}"
    );
}
