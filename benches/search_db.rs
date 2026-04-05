use std::{collections::HashMap, sync::Arc};

use criterion::{criterion_group, criterion_main, Criterion};
use pkgly::{
    app::api::search::query_parser::SearchQuery,
    repository::repo_type::NewRepository,
    search::PackageSearchRepository,
};
use nr_core::{
    database::entities::{
        project::{
            NewProject,
            versions::NewVersion,
        },
        storage::NewDBStorage,
    },
    repository::project::{ReleaseType, VersionData},
    storage::StorageName,
};
use serde_json::json;
use sqlx::PgPool;
use tokio::runtime::Runtime;
use uuid::Uuid;

const BENCH_ROW_COUNT: usize = 10_000;

fn search_db_query(c: &mut Criterion) {
    let dsn = match std::env::var("PKGLY_SEARCH_BENCH_DSN") {
        Ok(value) => value,
        Err(_) => {
            c.bench_function("search_db_query/skipped", |b| b.iter(|| ()));
            return;
        }
    };

    let runtime = Runtime::new().expect("start tokio runtime");
    let (pool, repository_id) = runtime.block_on(async {
        let pool = PgPool::connect(&dsn)
            .await
            .expect("connect benchmark database");
        let repository_id = seed_dataset(&pool)
            .await
            .expect("seed benchmark dataset");
        (pool, repository_id)
    });
    let pool = Arc::new(pool);
    let query = SearchQuery::default();

    c.bench_function("search_db_query", |b| {
        b.to_async(&runtime).iter(|| {
            let pool = pool.clone();
            let query = query.clone();
            async move {
                let searcher = PackageSearchRepository::new(pool.as_ref());
                let rows = searcher
                    .fetch_repository_rows(repository_id, &query, 25)
                    .await
                    .expect("query repository");
                criterion::black_box(rows.len());
            }
        });
    });
}

async fn seed_dataset(pool: &PgPool) -> Result<Uuid, sqlx::Error> {
    reset_database(pool).await?;
    let storage_id = insert_storage(pool).await?;
    let repository_id = insert_repository(pool, storage_id).await?;
    for idx in 0..BENCH_ROW_COUNT {
        let package = format!("package-{idx}");
        let version = format!("1.0.{idx}");
        insert_package(pool, repository_id, &package, &version).await?;
    }
    Ok(repository_id)
}

async fn insert_storage(pool: &PgPool) -> Result<Uuid, sqlx::Error> {
    let name = StorageName::new(format!("bench-storage-{}", Uuid::new_v4().simple()))
        .expect("valid storage name");
    let storage = NewDBStorage::new("Local".into(), name, json!({ "path": "/tmp" }));
    let inserted = storage.insert(pool).await?.expect("insert storage row");
    Ok(inserted.id)
}

async fn insert_repository(pool: &PgPool, storage_id: Uuid) -> Result<Uuid, sqlx::Error> {
    let name = format!("bench-repo-{}", Uuid::new_v4().simple());
    let repo = NewRepository {
        name,
        uuid: Uuid::new_v4(),
        repository_type: "npm".into(),
        configs: HashMap::with_hasher(Default::default()),
    };
    let row = repo.insert(storage_id, pool).await?;
    Ok(row.id)
}

async fn insert_package(
    pool: &PgPool,
    repository_id: Uuid,
    package: &str,
    version: &str,
) -> Result<(), sqlx::Error> {
    let project = NewProject {
        scope: None,
        project_key: package.to_string(),
        name: package.to_string(),
        description: None,
        repository: repository_id,
        storage_path: format!("packages/{package}"),
    }
    .insert(pool)
    .await?;

    let mut extra = VersionData::default();
    extra.extra = Some(json!({
        "size": 512,
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
    .await?;
    Ok(())
}

async fn reset_database(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP TABLE IF EXISTS project_versions, projects, repositories, storages CASCADE")
        .execute(pool)
        .await?;
    ensure_schema(pool).await?;
    Ok(())
}

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#)
        .execute(pool)
        .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS storages (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            storage_type TEXT NOT NULL,
            name TEXT NOT NULL,
            config JSONB NOT NULL,
            active BOOLEAN NOT NULL DEFAULT TRUE,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CONSTRAINT storages_name_key UNIQUE (name)
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS repositories (
            id UUID PRIMARY KEY,
            storage_id UUID NOT NULL,
            name TEXT NOT NULL,
            repository_type TEXT NOT NULL,
            visibility VARCHAR(64) NOT NULL DEFAULT 'Public',
            active BOOLEAN NOT NULL DEFAULT TRUE,
            storage_usage_bytes BIGINT,
            storage_usage_updated_at TIMESTAMPTZ,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            scope TEXT,
            key TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            repository_id UUID NOT NULL,
            path TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS project_versions (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            project_id UUID NOT NULL,
            repository_id UUID NOT NULL,
            version TEXT NOT NULL,
            release_type TEXT NOT NULL,
            path TEXT NOT NULL,
            publisher INTEGER,
            version_page TEXT,
            extra JSONB NOT NULL DEFAULT '{}'::jsonb,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

criterion_group!(benches, search_db_query);
criterion_main!(benches);
