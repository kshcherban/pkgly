#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use super::*;

use crate::repository::NewRepository;
use nr_core::{
    database::{DatabaseConfig, entities::storage::NewDBStorage, migration::run_migrations},
    storage::StorageName,
};
use once_cell::sync::Lazy;
use sha2::Digest;
use sqlx::{Connection, PgPool, postgres::PgPoolOptions};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

static DB_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

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
    for _ in 0..30 {
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

async fn insert_scheduled_deb_proxy_repo(
    pool: &PgPool,
    storage_id: Uuid,
    upstream_url: &str,
    interval_seconds: u64,
) -> Uuid {
    let repo = NewRepository {
        name: "deb-proxy-test".into(),
        uuid: Uuid::new_v4(),
        repository_type: "deb".into(),
        configs: ahash::HashMap::from_iter([(
            "deb".to_string(),
            serde_json::json!({
                "type": "proxy",
                "config": {
                    "upstream_url": upstream_url,
                    "layout": {
                        "type": "flat",
                        "config": {
                            "distribution": "./",
                            "architectures": []
                        }
                    },
                    "refresh": {
                        "enabled": true,
                        "schedule": {
                            "type": "interval_seconds",
                            "config": { "interval_seconds": interval_seconds }
                        }
                    }
                }
            }),
        )]),
    };
    repo.insert(storage_id, pool).await.expect("insert repo").id
}

async fn insert_scheduled_deb_proxy_repo_cron(
    pool: &PgPool,
    storage_id: Uuid,
    upstream_url: &str,
    expression: &str,
) -> Uuid {
    let repo = NewRepository {
        name: "deb-proxy-cron-test".into(),
        uuid: Uuid::new_v4(),
        repository_type: "deb".into(),
        configs: ahash::HashMap::from_iter([(
            "deb".to_string(),
            serde_json::json!({
                "type": "proxy",
                "config": {
                    "upstream_url": upstream_url,
                    "layout": {
                        "type": "flat",
                        "config": {
                            "distribution": "./",
                            "architectures": []
                        }
                    },
                    "refresh": {
                        "enabled": true,
                        "schedule": {
                            "type": "cron",
                            "config": { "expression": expression }
                        }
                    }
                }
            }),
        )]),
    };
    repo.insert(storage_id, pool).await.expect("insert repo").id
}

async fn start_flat_upstream_server(
    packages: bytes::Bytes,
    packages_gz: bytes::Bytes,
    deb_path: String,
    deb_bytes: bytes::Bytes,
    deb_counter: Arc<AtomicUsize>,
) -> anyhow::Result<(String, JoinHandle<()>)> {
    use axum::{Router, routing::get};

    let packages_bytes = packages.clone();
    let packages_gz_bytes = packages_gz.clone();
    let deb_bytes_route = deb_bytes.clone();

    let app = Router::new()
        .route(
            "/Packages",
            get(move || {
                let packages_bytes = packages_bytes.clone();
                async move { packages_bytes }
            }),
        )
        .route(
            "/Packages.gz",
            get(move || {
                let packages_gz_bytes = packages_gz_bytes.clone();
                async move { packages_gz_bytes }
            }),
        )
        .route(
            &format!("/{deb_path}"),
            get(move || {
                let deb_counter = deb_counter.clone();
                let deb_bytes_route = deb_bytes_route.clone();
                async move {
                    deb_counter.fetch_add(1, Ordering::SeqCst);
                    deb_bytes_route
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("upstream server error: {err}");
        }
    });
    Ok((format!("http://{addr}"), server))
}

fn build_minimal_deb(package: &str, version: &str, arch: &str) -> Vec<u8> {
    use std::io::Write;

    let control = format!(
        "Package: {package}\nVersion: {version}\nArchitecture: {arch}\nDescription: test package\n"
    );

    let mut control_tar = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut control_tar);
        let mut header = tar::Header::new_gnu();
        header.set_size(control.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "control", control.as_bytes())
            .expect("append control");
        builder.finish().expect("finish tar");
    }

    let mut control_gz = Vec::new();
    {
        let mut encoder =
            flate2::write::GzEncoder::new(&mut control_gz, flate2::Compression::fast());
        encoder.write_all(&control_tar).expect("write tar");
        encoder.finish().expect("finish gz");
    }

    let mut deb_bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut deb_bytes);
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

async fn build_site(db: &TestDb, root: &std::path::Path) -> crate::app::Pkgly {
    let cfg = DatabaseConfig {
        user: "postgres".into(),
        password: "password".into(),
        database: "postgres".into(),
        host: "127.0.0.1".into(),
        port: Some(db.port),
    };
    crate::app::Pkgly::new(
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
        cfg,
        Some(root.join("storages")),
    )
    .await
    .expect("create site")
}

#[tokio::test]
async fn scheduler_triggers_interval_refresh_for_due_repo() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");

    let deb_bytes = bytes::Bytes::from(build_minimal_deb("hello", "1.0.0", "amd64"));
    let deb_sha256 = format!("{:x}", sha2::Sha256::digest(&deb_bytes));
    let deb_path = "hello_1.0.0_amd64.deb".to_string();
    let packages = format!(
        "Package: hello\nVersion: 1.0.0\nArchitecture: amd64\nFilename: {deb_path}\nSize: {}\nSHA256: {deb_sha256}\nDescription: test\n\n",
        deb_bytes.len()
    );
    let packages_bytes = bytes::Bytes::from(packages);
    let packages_gz = {
        use std::io::Write;
        let mut gz = Vec::new();
        let mut encoder = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
        encoder.write_all(&packages_bytes).expect("write gz");
        encoder.finish().expect("finish gz");
        bytes::Bytes::from(gz)
    };

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream_url, _server) = start_flat_upstream_server(
        packages_bytes,
        packages_gz,
        deb_path.clone(),
        deb_bytes.clone(),
        counter.clone(),
    )
    .await
    .expect("start upstream");

    let storage_id = insert_local_storage(db.pool(), root.path()).await;
    let repo_id = insert_scheduled_deb_proxy_repo(db.pool(), storage_id, &upstream_url, 3600).await;

    let site = build_site(&db, root.path()).await;
    let now = Utc::now();
    let summary = deb_proxy_scheduler_tick(site.clone(), now)
        .await
        .expect("tick ok");

    assert_eq!(summary.due_repositories, 1);
    assert_eq!(summary.succeeded, 1);
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let row = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::FixedOffset>>>(
        "SELECT last_success_at FROM deb_proxy_refresh_status WHERE repository_id = $1",
    )
    .bind(repo_id)
    .fetch_one(&site.database)
    .await
    .expect("fetch status");
    assert!(row.is_some());

    site.close().await;
}

#[tokio::test]
async fn scheduler_skips_refresh_when_advisory_lock_held() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");

    let deb_bytes = bytes::Bytes::from(build_minimal_deb("hello", "1.0.0", "amd64"));
    let deb_sha256 = format!("{:x}", sha2::Sha256::digest(&deb_bytes));
    let deb_path = "hello_1.0.0_amd64.deb".to_string();
    let packages = format!(
        "Package: hello\nVersion: 1.0.0\nArchitecture: amd64\nFilename: {deb_path}\nSize: {}\nSHA256: {deb_sha256}\nDescription: test\n\n",
        deb_bytes.len()
    );
    let packages_bytes = bytes::Bytes::from(packages);
    let packages_gz = {
        use std::io::Write;
        let mut gz = Vec::new();
        let mut encoder = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
        encoder.write_all(&packages_bytes).expect("write gz");
        encoder.finish().expect("finish gz");
        bytes::Bytes::from(gz)
    };

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream_url, _server) = start_flat_upstream_server(
        packages_bytes,
        packages_gz,
        deb_path.clone(),
        deb_bytes.clone(),
        counter.clone(),
    )
    .await
    .expect("start upstream");

    let storage_id = insert_local_storage(db.pool(), root.path()).await;
    let repo_id = insert_scheduled_deb_proxy_repo(db.pool(), storage_id, &upstream_url, 3600).await;

    let mut conn = sqlx::PgConnection::connect(&db.url).await.expect("connect");
    let key = crate::repository::deb::refresh_status::deb_proxy_refresh_advisory_key(repo_id);
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(key)
        .execute(&mut conn)
        .await
        .expect("lock acquired");

    let site = build_site(&db, root.path()).await;
    let now = Utc::now();
    let summary = deb_proxy_scheduler_tick(site.clone(), now)
        .await
        .expect("tick ok");

    assert_eq!(summary.due_repositories, 1);
    assert_eq!(summary.succeeded, 0);
    assert_eq!(summary.skipped_running, 1);
    assert_eq!(counter.load(Ordering::SeqCst), 0);

    let status: Option<bool> = sqlx::query_scalar(
        "SELECT in_progress FROM deb_proxy_refresh_status WHERE repository_id = $1",
    )
    .bind(repo_id)
    .fetch_optional(&site.database)
    .await
    .expect("query status");
    assert!(status.is_none(), "status row should not be created");

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
fn cron_due_evaluation_respects_last_started_at() {
    let schedule = DebProxyRefreshSchedule::Cron(super::super::configs::DebProxyCronSchedule {
        expression: "0 3 * * *".into(),
    });
    let last_started_at =
        chrono::DateTime::parse_from_rfc3339("2025-12-12T04:00:00+00:00").expect("parse");

    let now_before = chrono::DateTime::parse_from_rfc3339("2025-12-13T02:00:00+00:00").unwrap();
    assert!(!is_due(
        now_before.with_timezone(&Utc),
        &schedule,
        Some(last_started_at)
    ));

    let now_after = chrono::DateTime::parse_from_rfc3339("2025-12-13T04:00:00+00:00").unwrap();
    assert!(is_due(
        now_after.with_timezone(&Utc),
        &schedule,
        Some(last_started_at)
    ));
}

#[test]
fn next_run_at_interval_is_now_when_never_started() {
    let schedule =
        DebProxyRefreshSchedule::IntervalSeconds(super::super::configs::DebProxyIntervalSchedule {
            interval_seconds: 60,
        });
    let now = chrono::DateTime::parse_from_rfc3339("2025-12-13T12:00:00+00:00")
        .unwrap()
        .with_timezone(&Utc);
    let next = next_run_at(now, &schedule, None).expect("next");
    assert_eq!(next, now);
}

#[test]
fn next_run_at_cron_returns_future_time() {
    let schedule = DebProxyRefreshSchedule::Cron(super::super::configs::DebProxyCronSchedule {
        expression: "0 3 * * *".into(),
    });
    let now = chrono::DateTime::parse_from_rfc3339("2025-12-13T12:00:00+00:00")
        .unwrap()
        .with_timezone(&Utc);
    let next = next_run_at(now, &schedule, None).expect("next");
    assert!(next > now);
}

#[tokio::test]
async fn scheduler_triggers_cron_refresh_when_due() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");

    let deb_bytes = bytes::Bytes::from(build_minimal_deb("hello", "1.0.0", "amd64"));
    let deb_sha256 = format!("{:x}", sha2::Sha256::digest(&deb_bytes));
    let deb_path = "hello_1.0.0_amd64.deb".to_string();
    let packages = format!(
        "Package: hello\nVersion: 1.0.0\nArchitecture: amd64\nFilename: {deb_path}\nSize: {}\nSHA256: {deb_sha256}\nDescription: test\n\n",
        deb_bytes.len()
    );
    let packages_bytes = bytes::Bytes::from(packages);
    let packages_gz = {
        use std::io::Write;
        let mut gz = Vec::new();
        let mut encoder = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
        encoder.write_all(&packages_bytes).expect("write gz");
        encoder.finish().expect("finish gz");
        bytes::Bytes::from(gz)
    };

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream_url, _server) = start_flat_upstream_server(
        packages_bytes,
        packages_gz,
        deb_path.clone(),
        deb_bytes.clone(),
        counter.clone(),
    )
    .await
    .expect("start upstream");

    let storage_id = insert_local_storage(db.pool(), root.path()).await;
    let repo_id =
        insert_scheduled_deb_proxy_repo_cron(db.pool(), storage_id, &upstream_url, "*/1 * * * *")
            .await;

    // Seed last_started_at to 2 minutes ago so the next scheduled time is in the past and due.
    sqlx::query(
        r#"
        INSERT INTO deb_proxy_refresh_status (repository_id, in_progress, last_started_at, updated_at)
        VALUES ($1, FALSE, NOW() - INTERVAL '2 minutes', NOW())
        "#,
    )
    .bind(repo_id)
    .execute(db.pool())
    .await
    .expect("insert status");

    let site = build_site(&db, root.path()).await;
    let now = Utc::now();
    let summary = deb_proxy_scheduler_tick(site.clone(), now)
        .await
        .expect("tick ok");

    assert_eq!(summary.due_repositories, 1);
    assert_eq!(summary.succeeded, 1);
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    site.close().await;
}
