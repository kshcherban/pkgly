#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::{
    app::{
        authentication::session::SessionManagerConfig,
        config::{Mode, SecuritySettings, SiteSetting},
        webhooks::{UpsertWebhookInput, WebhookEventType, WebhookHeaderInput, create_webhook},
    },
    repository::{
        DynRepository, RepositoryAuthConfig, RepositoryAuthentication, RepositoryRequestBody,
        RepositoryType, repo_tracing::RepositoryRequestTracing, test_helpers::test_storage,
    },
    test_support::DB_TEST_LOCK,
};
use ahash::HashMap;
use bytes::Bytes;
use nr_core::{
    database::{
        DatabaseConfig,
        entities::{
            project::{NewProject, versions::NewVersion},
            repository::DBRepository,
            user::UserSafeData,
        },
        migration::run_migrations,
    },
    repository::{
        Visibility,
        project::{ReleaseType, VersionData},
    },
    storage::StoragePath,
    user::{Email, Username, permissions::RepositoryActions},
};
use nr_storage::{FileContent, Storage};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    port: u16,
    _container: Container<'static, GenericImage>,
    _docker: &'static Cli,
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
    run_migrations(&db.pool).await.expect("run migrations");
    db
}

async fn build_site(db: &TestDb, root: &std::path::Path) -> Pkgly {
    Pkgly::new(
        Mode::Debug,
        SiteSetting::default(),
        SecuritySettings::default(),
        SessionManagerConfig {
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

fn admin_user() -> UserSafeData {
    let fixed_time =
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
    UserSafeData {
        id: 1,
        name: "Test Admin".into(),
        username: Username::new("test_admin".into()).expect("username"),
        email: Email::new("admin@example.com".into()).expect("email"),
        require_password_change: false,
        active: true,
        admin: true,
        user_manager: false,
        system_manager: true,
        default_repository_actions: vec![RepositoryActions::Read, RepositoryActions::Write],
        updated_at: fixed_time,
        created_at: fixed_time,
    }
}

#[tokio::test]
async fn create_new_ruby_hosted_accepts_default_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        RubyRepositoryConfigType::get_type_static().to_string(),
        json!({ "type": "Hosted" }),
    );

    let result = RubyRepositoryType::default()
        .create_new("ruby-hosted".into(), Uuid::new_v4(), configs, storage)
        .await;

    let repository = result.expect("hosted repository to be created");
    assert_eq!(repository.repository_type, REPOSITORY_TYPE_ID);
    assert!(
        repository
            .configs
            .contains_key(RubyRepositoryConfigType::get_type_static()),
        "expected ruby config to be persisted",
    );
}

#[tokio::test]
async fn create_new_ruby_proxy_accepts_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        RubyRepositoryConfigType::get_type_static().to_string(),
        json!({
            "type": "Proxy",
            "config": {
                "upstream_url": "https://rubygems.org"
            }
        }),
    );

    let result = RubyRepositoryType::default()
        .create_new("ruby-proxy".into(), Uuid::new_v4(), configs, storage)
        .await;

    let repository = result.expect("proxy repository to be created");
    assert_eq!(repository.repository_type, REPOSITORY_TYPE_ID);
}

#[tokio::test]
async fn create_new_ruby_rejects_config_missing_type() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        RubyRepositoryConfigType::get_type_static().to_string(),
        json!({}),
    );

    let result = RubyRepositoryType::default()
        .create_new("ruby-invalid".into(), Uuid::new_v4(), configs, storage)
        .await;

    match result {
        Err(super::RepositoryFactoryError::InvalidConfig(repository, message)) => {
            assert_eq!(repository, REPOSITORY_TYPE_ID);
            assert!(
                message.contains("type"),
                "expected error message to mention missing type, got: {message}"
            );
        }
        other => panic!("expected invalid config error, got: {other:?}"),
    }
}

#[tokio::test]
async fn ruby_yank_enqueues_delete_webhook_before_catalog_row_is_removed() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path()).await;
    let storage = test_storage().await;
    let storage_id = Uuid::new_v4();
    let repository_id = Uuid::new_v4();
    let gem_path = StoragePath::from("gems/example-1.0.0.gem");

    sqlx::query(
        r#"
        INSERT INTO storages (id, name, storage_type, config)
        VALUES ($1, 'primary', 'Local', $2)
        "#,
    )
    .bind(storage_id)
    .bind(json!({ "Local": { "path": root.path().join("db-storage").to_string_lossy() } }))
    .execute(&site.database)
    .await
    .expect("insert storage");

    let repository: DBRepository = sqlx::query_as(
        r#"
        INSERT INTO repositories (id, storage_id, name, repository_type, visibility)
        VALUES ($1, $2, 'ruby-hosted', 'ruby', $3)
        RETURNING *
        "#,
    )
    .bind(repository_id)
    .bind(storage_id)
    .bind(Visibility::Private)
    .fetch_one(&site.database)
    .await
    .expect("insert repository");

    let project = NewProject {
        scope: None,
        project_key: "example".into(),
        name: "example".into(),
        description: None,
        repository: repository_id,
        storage_path: "gems/example".into(),
    }
    .insert(&site.database)
    .await
    .expect("insert project");

    NewVersion {
        project_id: project.id,
        repository_id,
        version: "1.0.0".into(),
        release_type: ReleaseType::Stable,
        version_path: gem_path.to_string(),
        publisher: None,
        version_page: None,
        extra: VersionData::default(),
    }
    .insert(&site.database)
    .await
    .expect("insert version");

    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(b"test gem")),
            &gem_path,
        )
        .await
        .expect("save gem");

    create_webhook(
        &site.database,
        UpsertWebhookInput {
            name: "ruby deletes".into(),
            enabled: true,
            target_url: "http://127.0.0.1:9/webhook".into(),
            events: vec![WebhookEventType::PackageDeleted],
            headers: Vec::<WebhookHeaderInput>::new(),
        },
    )
    .await
    .expect("create webhook");

    let hosted = hosted::RubyHosted::load(site.clone(), storage, repository)
        .await
        .expect("load ruby hosted repository");
    let dyn_repository = DynRepository::Ruby(RubyRepository::Hosted(hosted.clone()));
    let (parts, _) = http::Request::builder()
        .method(http::Method::DELETE)
        .uri("/api/v1/gems/yank")
        .body(())
        .expect("request")
        .into_parts();
    let request = RepositoryRequest {
        parts,
        body: RepositoryRequestBody::from_bytes(Bytes::from_static(
            b"gem_name=example&version=1.0.0",
        )),
        path: StoragePath::from("api/v1/gems/yank"),
        authentication: RepositoryAuthentication::Basic(None, admin_user()),
        auth_config: RepositoryAuthConfig::default(),
        trace: RepositoryRequestTracing::new(
            &dyn_repository,
            &tracing::Span::none(),
            Default::default(),
        ),
    };

    hosted.handle_delete(request).await.expect("yank succeeds");

    let deliveries: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM webhook_deliveries
        WHERE event_type = 'package.deleted'
          AND payload #>> '{data,package,name}' = 'example'
          AND payload #>> '{data,package,version}' = '1.0.0'
        "#,
    )
    .fetch_one(&site.database)
    .await
    .expect("count deliveries");

    assert_eq!(deliveries, 1);
    site.close().await;
}
