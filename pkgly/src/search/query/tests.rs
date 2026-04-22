#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use std::iter::FromIterator;

use ahash::{HashMap, HashSet};
use nr_core::{
    database::{
        entities::{
            project::{NewProject, versions::NewVersion},
            storage::NewDBStorage,
        },
        migration::run_migrations,
    },
    repository::project::{ReleaseType, VersionData},
    storage::StorageName,
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

use crate::{
    app::api::search::{Operator, SearchQuery},
    repository::NewRepository,
    search::PackageSearchRepository,
    test_support::DB_TEST_LOCK,
};

struct TestDb {
    pool: PgPool,
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

async fn fresh_pool() -> TestDb {
    let db = start_postgres().await;

    let migrate_pool = db.pool().clone();
    run_migrations(&migrate_pool)
        .await
        .expect("run database migrations");

    db
}

async fn reset_database(db: &TestDb) {
    sqlx::query(
        "TRUNCATE TABLE package_files, project_versions, projects, repositories, storages RESTART IDENTITY CASCADE",
    )
    .execute(db.pool())
    .await
    .expect("truncate test tables");
}

fn unique_name(prefix: &str) -> String {
    let raw = Uuid::new_v4().simple().to_string();
    format!("{prefix}-{}", &raw[..12])
}

async fn insert_storage(pool: &PgPool) -> Uuid {
    let storage_name = StorageName::new(unique_name("storage")).expect("valid storage name");
    let storage = NewDBStorage::new("Local".into(), storage_name, json!({ "path": "/tmp" }));
    storage
        .insert(pool)
        .await
        .expect("insert storage")
        .expect("storage row")
        .id
}

async fn insert_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
    let repo_name = unique_name("repo");
    let repo = NewRepository {
        name: repo_name,
        uuid: Uuid::new_v4(),
        repository_type: "npm".into(),
        configs: HashMap::with_hasher(Default::default()),
    };
    repo.insert(storage_id, pool)
        .await
        .expect("insert repository")
        .id
}

async fn insert_package(pool: &PgPool, repository_id: Uuid, package: &str, version: &str) {
    let project = NewProject {
        scope: None,
        project_key: package.to_string(),
        name: package.to_string(),
        description: None,
        repository: repository_id,
        storage_path: format!("packages/{package}"),
    }
    .insert(pool)
    .await
    .expect("insert project");

    let mut extra = VersionData::default();
    extra.extra = Some(json!({
        "size": 1024,
        "filename": format!("packages/{package}/{package}-{version}.tgz"),
    }));

    NewVersion {
        project_id: project.id,
        repository_id,
        version: version.to_string(),
        release_type: ReleaseType::Stable,
        version_path: format!("packages/{package}/{version}/{package}-{version}.tgz"),
        publisher: None,
        version_page: None,
        extra,
    }
    .insert(pool)
    .await
    .expect("insert version");
}

async fn insert_package_with_digest(
    pool: &PgPool,
    repository_id: Uuid,
    package: &str,
    version: &str,
    digest: &str,
) {
    let project = NewProject {
        scope: None,
        project_key: package.to_string(),
        name: package.to_string(),
        description: None,
        repository: repository_id,
        storage_path: format!("packages/{package}"),
    }
    .insert(pool)
    .await
    .expect("insert project");

    let mut extra = VersionData::default();
    extra.extra = Some(json!({
        "size": 1024,
        "filename": format!("packages/{package}/{package}-{version}.tgz"),
        "sha256": digest,
    }));

    NewVersion {
        project_id: project.id,
        repository_id,
        version: version.to_string(),
        release_type: ReleaseType::Stable,
        version_path: format!("packages/{package}/{version}/{package}-{version}.tgz"),
        publisher: None,
        version_page: None,
        extra,
    }
    .insert(pool)
    .await
    .expect("insert version");
}

#[tokio::test]
async fn fetch_repository_rows_filters_by_package() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repository_id = insert_repository(db.pool(), storage_id).await;
    insert_package(db.pool(), repository_id, "alpha", "1.0.0").await;
    insert_package(db.pool(), repository_id, "beta", "1.0.0").await;

    let repository = PackageSearchRepository::new(db.pool());
    let query = SearchQuery {
        package_filter: Some((Operator::Equals, "alpha".into())),
        ..SearchQuery::default()
    };
    let rows = repository
        .fetch_repository_rows(repository_id, &query, 10)
        .await
        .expect("query rows");

    let names: HashSet<_> = rows.iter().map(|row| row.package_name.as_str()).collect();
    assert_eq!(names, HashSet::from_iter(["alpha"]));
}

#[tokio::test]
async fn fetch_repository_rows_filters_by_terms() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repository_id = insert_repository(db.pool(), storage_id).await;
    insert_package(db.pool(), repository_id, "core-lib", "2.0.0").await;
    insert_package(db.pool(), repository_id, "support-lib", "1.1.0").await;

    let repository = PackageSearchRepository::new(db.pool());
    let mut query = SearchQuery::default();
    query.terms = vec!["support".into()];

    let rows = repository
        .fetch_repository_rows(repository_id, &query, 5)
        .await
        .expect("query rows");

    let names: Vec<_> = rows.iter().map(|row| row.package_name.as_str()).collect();
    assert_eq!(names, vec!["support-lib"]);
}

#[tokio::test]
async fn repository_has_index_rows_detects_catalog_state() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repository_id = insert_repository(db.pool(), storage_id).await;
    let repository = PackageSearchRepository::new(db.pool());

    assert!(
        !repository
            .repository_has_index_rows(repository_id)
            .await
            .expect("query flag")
    );

    insert_package(db.pool(), repository_id, "delta", "0.1.0").await;

    assert!(
        repository
            .repository_has_index_rows(repository_id)
            .await
            .expect("query flag")
    );
}

#[tokio::test]
async fn fetch_repository_rows_filters_by_digest() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_pool().await;
    reset_database(&db).await;

    let storage_id = insert_storage(db.pool()).await;
    let repository_id = insert_repository(db.pool(), storage_id).await;
    insert_package_with_digest(
        db.pool(),
        repository_id,
        "alpha",
        "1.0.0",
        "sha256:deadbeef",
    )
    .await;
    insert_package_with_digest(db.pool(), repository_id, "beta", "1.0.0", "sha256:cafebabe").await;

    let repository = PackageSearchRepository::new(db.pool());
    let query = SearchQuery {
        digest_filter: Some((Operator::Equals, "sha256:deadbeef".into())),
        ..SearchQuery::default()
    };
    let rows = repository
        .fetch_repository_rows(repository_id, &query, 10)
        .await
        .expect("query rows");

    let names: Vec<_> = rows.iter().map(|row| row.package_name.as_str()).collect();
    assert_eq!(names, vec!["alpha"]);
}
