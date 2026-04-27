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
        deb::DebRepository, repo_tracing::RepositoryRequestTracing, test_helpers::test_storage,
    },
    test_support::DB_TEST_LOCK,
};
use chrono::DateTime;
use flate2::{Compression, write::GzEncoder};
use nr_core::{
    database::{
        DatabaseConfig,
        entities::{repository::DBRepository, user::UserSafeData},
        migration::run_migrations,
    },
    repository::Visibility,
    storage::StoragePath,
    user::{Email, Username, permissions::RepositoryActions},
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::io::{Cursor, Write};
use tar::Builder;
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
    let fixed_time = DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
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

async fn insert_admin_user(pool: &PgPool) {
    sqlx::query(
        r#"
        INSERT INTO users (
            id,
            name,
            username,
            email,
            active,
            admin,
            user_manager,
            system_manager,
            default_repository_actions
        )
        VALUES (
            1,
            'Test Admin',
            'test_admin',
            'admin@example.com',
            TRUE,
            TRUE,
            FALSE,
            TRUE,
            ARRAY['Read', 'Write']::text[]
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("insert admin user");
}

#[tokio::test]
async fn hosted_upload_enqueues_package_published_webhook() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let site = build_site(&db, root.path()).await;
    let storage = test_storage().await;
    let storage_id = Uuid::new_v4();
    let repository_id = Uuid::new_v4();
    insert_admin_user(&site.database).await;

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
        VALUES ($1, $2, 'deb-hosted', 'deb', $3)
        RETURNING *
        "#,
    )
    .bind(repository_id)
    .bind(storage_id)
    .bind(Visibility::Private)
    .fetch_one(&site.database)
    .await
    .expect("insert repository");

    create_webhook(
        &site.database,
        UpsertWebhookInput {
            name: "deb publishes".into(),
            enabled: true,
            target_url: "http://127.0.0.1:9/webhook".into(),
            events: vec![WebhookEventType::PackagePublished],
            headers: Vec::<WebhookHeaderInput>::new(),
        },
    )
    .await
    .expect("create webhook");

    let hosted = DebHostedRepository::load(
        site.clone(),
        storage,
        repository,
        DebHostedConfig::default(),
    )
    .await
    .expect("load deb hosted repository");
    let dyn_repository = DynRepository::Deb(DebRepository::Hosted(hosted.clone()));
    let boundary = "pkgly-deb-upload-boundary";
    let body = multipart_body(boundary, &build_minimal_deb("sample", "1.0.0", "amd64"));
    let (parts, _) = http::Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(
            CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(())
        .expect("request")
        .into_parts();
    let request = RepositoryRequest {
        parts,
        body: RepositoryRequestBody::from_bytes(Bytes::from(body)),
        path: StoragePath::from(""),
        authentication: RepositoryAuthentication::Basic(None, admin_user()),
        auth_config: RepositoryAuthConfig::default(),
        trace: RepositoryRequestTracing::new(
            &dyn_repository,
            &tracing::Span::none(),
            Default::default(),
        ),
    };

    hosted.handle_post(request).await.expect("upload succeeds");

    let payloads: Vec<Value> = sqlx::query_scalar(
        r#"
        SELECT payload
        FROM webhook_deliveries
        WHERE event_type = 'package.published'
        "#,
    )
    .fetch_all(&site.database)
    .await
    .expect("fetch deliveries");

    assert_eq!(payloads.len(), 1);
    let payload = &payloads[0];
    assert_eq!(payload["data"]["repository"]["format"], "deb");
    assert_eq!(payload["data"]["package"]["name"], "sample");
    assert_eq!(payload["data"]["package"]["version"], "1.0.0");
    assert_eq!(
        payload["data"]["package"]["canonical_path"],
        "pool/main/s/sample/sample_1.0.0_amd64.deb"
    );
    site.close().await;
}

fn multipart_body(boundary: &str, deb_bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    write!(
        body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"package\"; filename=\"sample_1.0.0_amd64.deb\"\r\nContent-Type: application/vnd.debian.binary-package\r\n\r\n"
    )
    .expect("write part headers");
    body.extend_from_slice(deb_bytes);
    write!(body, "\r\n--{boundary}--\r\n").expect("write closing boundary");
    body
}

fn build_minimal_deb(package: &str, version: &str, arch: &str) -> Vec<u8> {
    let control = format!(
        "Package: {package}\nVersion: {version}\nArchitecture: {arch}\nDescription: test package\n"
    );
    let mut control_tar = Vec::new();
    {
        let cursor = Cursor::new(&mut control_tar);
        let mut builder = Builder::new(cursor);
        let mut header = tar::Header::new_gnu();
        header.set_size(control.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, "control", control.as_bytes())
            .expect("append control");
        builder.finish().expect("finish tar");
    }

    let mut control_gz = Vec::new();
    {
        let mut encoder = GzEncoder::new(&mut control_gz, Compression::default());
        encoder.write_all(&control_tar).expect("write tar");
        encoder.finish().expect("finish gzip");
    }

    let mut deb_bytes = Vec::new();
    {
        let cursor = Cursor::new(&mut deb_bytes);
        let mut builder = ar::Builder::new(cursor);
        builder
            .append(
                &ar::Header::new(b"debian-binary".to_vec(), 4),
                &b"2.0\n"[..],
            )
            .expect("append debian-binary");
        builder
            .append(
                &ar::Header::new(b"control.tar.gz".to_vec(), control_gz.len() as u64),
                &control_gz[..],
            )
            .expect("append control");
        builder
            .append(&ar::Header::new(b"data.tar.gz".to_vec(), 0), &[][..])
            .expect("append data");
    }
    deb_bytes
}
