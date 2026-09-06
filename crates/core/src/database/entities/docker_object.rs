// ABOUTME: Persists Docker object sizes and direct manifest reference paths.
// ABOUTME: Provides deduplicated cached-byte totals and deletion bookkeeping.
use sqlx::PgPool;
use uuid::Uuid;

pub struct DBDockerObject;

impl DBDockerObject {
    /// Backfills an observed object without overwriting a concurrent recorded write.
    /// Propagates size conversion and database errors.
    pub async fn insert_missing(
        database: &PgPool,
        repository: Uuid,
        path: &str,
        size: u64,
        references: &[String],
    ) -> Result<(), sqlx::Error> {
        let size = i64::try_from(size).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        sqlx::query(
            "INSERT INTO docker_objects (repository_id, path, size_bytes, references_paths)
            VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(repository)
        .bind(path)
        .bind(size)
        .bind(references)
        .execute(database)
        .await?;
        Ok(())
    }
    /// Records a successful object write and replaces its manifest references.
    /// Returns database errors, including sizes outside PostgreSQL's BIGINT range.
    pub async fn upsert(
        database: &PgPool,
        repository: Uuid,
        path: &str,
        size: u64,
        references: &[String],
    ) -> Result<(), sqlx::Error> {
        let size = i64::try_from(size).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        sqlx::query(
            "INSERT INTO docker_objects (repository_id, path, size_bytes, references_paths)
            VALUES ($1, $2, $3, $4) ON CONFLICT (repository_id, path) DO UPDATE
            SET size_bytes = EXCLUDED.size_bytes, references_paths = EXCLUDED.references_paths
            WHERE (docker_objects.size_bytes, docker_objects.references_paths)
                IS DISTINCT FROM (EXCLUDED.size_bytes, EXCLUDED.references_paths)",
        )
        .bind(repository)
        .bind(path)
        .bind(size)
        .bind(references)
        .execute(database)
        .await?;
        Ok(())
    }

    /// Returns distinct reachable stored bytes, or None for an unindexed root.
    /// Propagates database errors; recursion terminates even for cyclic references.
    pub async fn referenced_size(
        database: &PgPool,
        repository: Uuid,
        path: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar("SELECT docker_referenced_size($1, $2)")
            .bind(repository)
            .bind(path)
            .fetch_one(database)
            .await
    }

    /// Removes successfully deleted objects; references from other manifests remain.
    /// Returns database errors without suppressing bookkeeping failures.
    pub async fn delete_paths(
        database: &PgPool,
        repository: Uuid,
        paths: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM docker_objects WHERE repository_id = $1 AND path = ANY($2)")
            .bind(repository)
            .bind(paths)
            .execute(database)
            .await?;
        Ok(())
    }
}
