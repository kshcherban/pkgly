#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use nr_core::{
    database::entities::storage::NewDBStorage,
    database::entities::user::NewUserRequest,
    database::{
        entities::user::permissions::NewUserRepositoryPermissions, migration::run_migrations,
    },
    repository::Visibility,
    storage::StorageName,
    user::{Email, Username, permissions::RepositoryActions},
};
use once_cell::sync::Lazy;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

static DB_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

struct TestDb {
    pool: PgPool,
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
        email: Email::new("virtual-test@example.invalid".to_string()).expect("email"),
        password: Some("password".to_string()),
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
