#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::borrow::Cow;

use serde_json::{Value, json};
use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

use super::super::load_tags;
use crate::test_support::DB_TEST_LOCK;
use nr_core::database::{entities::project::npm::DBNpmProjectMetadata, migration::run_migrations};

const TAG_MIGRATION: i64 = 20261016120000;
pub(super) const PROJECT: Uuid = Uuid::from_u128(3);
const PRERELEASE_ONLY_PROJECT: Uuid = Uuid::from_u128(4);
static MIGRATOR: Migrator = sqlx::migrate!("../crates/core/migrations");

#[tokio::test]
async fn upgrade_preserves_existing_packages_and_stored_tags_on_restart() {
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    assert!(!tag_column_exists(&pool).await);
    let before = package_snapshot(&pool).await;

    run_migrations(&pool)
        .await
        .expect("upgrade existing database");
    assert!(tag_column_exists(&pool).await);
    assert_eq!(package_snapshot(&pool).await, before);
    assert_eq!(stored_tags(&pool).await, None);
    let fallback = load_tags(PROJECT, &pool).await.unwrap();
    assert_eq!(fallback.get("latest").map(String::as_str), Some("1.0.0"));
    assert!(
        load_tags(PRERELEASE_ONLY_PROJECT, &pool)
            .await
            .unwrap()
            .is_empty()
    );

    let beta = super::HashMap::from_iter([("beta".into(), "2.0.0-beta.1".into())]);
    DBNpmProjectMetadata::merge_tags(PROJECT, beta.clone(), fallback.clone(), &pool)
        .await
        .unwrap();
    run_migrations(&pool)
        .await
        .expect("repeat startup migrations");
    let expected = fallback.into_iter().chain(beta).collect();
    assert_eq!(load_tags(PROJECT, &pool).await.unwrap(), expected);
    assert_eq!(package_snapshot(&pool).await, before);
    assert_eq!(
        stored_tags(&pool).await,
        Some(json!({"latest":"1.0.0", "beta":"2.0.0-beta.1"}))
    );
}

#[tokio::test]
async fn rollback_removes_only_tag_storage_and_reupgrade_preserves_packages() {
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    let before = package_snapshot(&pool).await;
    run_migrations(&pool).await.unwrap();
    let tags = super::HashMap::from_iter([("beta".into(), "2.0.0-beta.1".into())]);
    DBNpmProjectMetadata::merge_tags(
        PROJECT,
        tags,
        load_tags(PROJECT, &pool).await.unwrap(),
        &pool,
    )
    .await
    .unwrap();
    assert!(stored_tags(&pool).await.is_some());

    let previous_version = MIGRATOR
        .iter()
        .filter(|migration| migration.version < TAG_MIGRATION)
        .map(|migration| migration.version)
        .max()
        .unwrap();
    MIGRATOR
        .undo(&pool, previous_version)
        .await
        .expect("explicit rollback");
    assert!(!tag_column_exists(&pool).await);
    assert_eq!(package_snapshot(&pool).await, before);

    run_migrations(&pool).await.expect("upgrade after rollback");
    assert!(tag_column_exists(&pool).await);
    assert_eq!(package_snapshot(&pool).await, before);
    assert_eq!(stored_tags(&pool).await, None);
    assert_eq!(
        load_tags(PROJECT, &pool)
            .await
            .unwrap()
            .get("latest")
            .map(String::as_str),
        Some("1.0.0")
    );
}

pub(super) async fn previous_database(docker: &Cli) -> (Container<'_, GenericImage>, PgPool) {
    let image = GenericImage::new("postgres", "18-alpine")
        .with_env_var("POSTGRES_PASSWORD", "password")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres");
    let container = docker.run(image);
    let url = format!(
        "postgres://postgres:password@127.0.0.1:{}/postgres",
        container.get_host_port_ipv4(5432)
    );
    let pool = connect_database(&url).await;
    let previous = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version < TAG_MIGRATION)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    previous.run(&pool).await.expect("create previous schema");
    seed_existing_packages(&pool).await;
    (container, pool)
}

async fn connect_database(url: &str) -> PgPool {
    for _ in 0..60 {
        if let Ok(pool) = PgPoolOptions::new().max_connections(4).connect(url).await {
            return pool;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    panic!("postgres container did not become ready");
}

async fn seed_existing_packages(pool: &PgPool) {
    sqlx::raw_sql(r#"
        INSERT INTO storages (id, name, storage_type, config)
        VALUES ('00000000-0000-0000-0000-000000000001', 'npm-migration', 'Local', '{}');
        INSERT INTO repositories (id, storage_id, name, repository_type)
        VALUES ('00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000001', 'npm-hosted', 'npm');
        INSERT INTO projects (id, repository_id, scope, key, name, path, description)
        VALUES
            ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000002', 'scope', '@scope/demo', 'demo', '@scope/demo', 'Existing package'),
            ('00000000-0000-0000-0000-000000000004', '00000000-0000-0000-0000-000000000002', NULL, 'beta-only', 'beta-only', 'beta-only', 'Existing prerelease');
        INSERT INTO project_versions (project_id, repository_id, version, release_type, path, extra, created_at)
        VALUES
            ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000002', '1.0.0', 'stable', '@scope/demo/1.0.0', '{"extra":{"name":"@scope/demo","version":"1.0.0","dist":{"tarball":"https://registry.test/demo-1.0.0.tgz"}}}', '2026-01-01T00:00:00Z'),
            ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000002', '2.0.0-beta.1', 'stable', '@scope/demo/2.0.0-beta.1', '{"extra":{"name":"@scope/demo","version":"2.0.0-beta.1","dist":{"tarball":"https://registry.test/demo-2.0.0-beta.1.tgz"}}}', '2026-02-01T00:00:00Z'),
            ('00000000-0000-0000-0000-000000000004', '00000000-0000-0000-0000-000000000002', '1.0.0-beta.1', 'stable', 'beta-only/1.0.0-beta.1', '{"extra":{"name":"beta-only","version":"1.0.0-beta.1"}}', '2026-01-01T00:00:00Z');
    "#).execute(pool).await.expect("seed existing packages before upgrade");
}

async fn package_snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
        'projects', (SELECT jsonb_agg(to_jsonb(p) - 'npm_dist_tags' ORDER BY id) FROM projects p),
        'versions', (SELECT jsonb_agg(to_jsonb(v) ORDER BY id) FROM project_versions v))"#,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn tag_column_exists(pool: &PgPool) -> bool {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'projects' AND column_name = 'npm_dist_tags')")
        .fetch_one(pool).await.unwrap()
}

async fn stored_tags(pool: &PgPool) -> Option<Value> {
    sqlx::query_scalar("SELECT npm_dist_tags FROM projects WHERE id = $1")
        .bind(PROJECT)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn legacy_latest_follows_upload_time_and_ignores_prereleases() {
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    run_migrations(&pool).await.unwrap();
    sqlx::query(
        "UPDATE project_versions SET created_at = NOW() - INTERVAL '1 day' WHERE project_id = $1",
    )
    .bind(PROJECT)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO project_versions (project_id, repository_id, version, release_type, path, created_at) SELECT id, repository_id, '0.9.0+Build.X', 'stable', 'demo/0.9.0', NOW() FROM projects WHERE id=$1")
        .bind(PROJECT).execute(&pool).await.unwrap();
    let tags = load_tags(PROJECT, &pool).await.unwrap();
    assert_eq!(
        tags.get("latest").map(String::as_str),
        Some("0.9.0+Build.X")
    );
}

#[tokio::test]
async fn npm_upgrade_retains_shared_case_insensitive_catalog_paths() {
    use nr_core::database::entities::project::versions::DBProjectVersion;
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    let before = package_snapshot(&pool).await;
    run_migrations(&pool).await.unwrap();
    assert_eq!(package_snapshot(&pool).await, before);
    let case_columns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.columns WHERE table_name='project_versions' AND column_name IN ('npm_catalog_key','case_sensitive_path')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(case_columns, 0, "npm must retain the shared catalog schema");
    let found =
        DBProjectVersion::find_by_version_directory("@SCOPE/DEMO/1.0.0", Uuid::from_u128(2), &pool)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(found.version, "1.0.0");
    let case_version: Uuid = sqlx::query_scalar("INSERT INTO project_versions (project_id,repository_id,version,release_type,path) VALUES ($1,$2,'1.0.0-RC.1','release_candidate','@scope/demo/1.0.0-RC.1') RETURNING id")
        .bind(PROJECT).bind(Uuid::from_u128(2)).fetch_one(&pool).await.unwrap();
    let collision = sqlx::query("INSERT INTO project_versions (project_id,repository_id,version,release_type,path) VALUES ($1,$2,'1.0.0-rc.1','release_candidate','@scope/demo/1.0.0-rc.1')")
        .bind(PROJECT).bind(Uuid::from_u128(2)).execute(&pool).await.unwrap_err();
    assert_eq!(
        collision.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
    sqlx::query("DELETE FROM project_versions WHERE id=$1")
        .bind(case_version)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(package_snapshot(&pool).await, before);
}
