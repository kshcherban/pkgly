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
            SET size_bytes = EXCLUDED.size_bytes,
                references_paths = EXCLUDED.references_paths,
                revision = docker_objects.revision + 1
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

    /// Reports whether an indexed manifest graph is missing its root or a reachable object row.
    pub async fn needs_backfill(
        database: &PgPool,
        repository: Uuid,
        root: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "WITH RECURSIVE reachable(path) AS (
                 SELECT $2::text
                 UNION
                 SELECT unnest(object.references_paths)
                 FROM docker_objects AS object
                 JOIN reachable ON reachable.path = object.path
                 WHERE object.repository_id = $1
             )
             SELECT NOT EXISTS (
                 SELECT 1 FROM docker_objects
                 WHERE repository_id = $1 AND path = $2
             ) OR EXISTS (
                 SELECT 1
                 FROM reachable
                 LEFT JOIN docker_objects AS object
                   ON object.repository_id = $1 AND object.path = reachable.path
                 WHERE object.path IS NULL
             )",
        )
        .bind(repository)
        .bind(root)
        .fetch_one(database)
        .await
    }

    /// Returns the accounting revision for one object, if it is indexed.
    pub async fn revision(
        database: &PgPool,
        repository: Uuid,
        path: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT revision FROM docker_objects WHERE repository_id = $1 AND path = $2",
        )
        .bind(repository)
        .bind(path)
        .fetch_optional(database)
        .await
    }

    /// Returns every recorded object path with its stored size for reconciliation.
    pub async fn list_rows(
        database: &PgPool,
        repository: Uuid,
    ) -> Result<Vec<(String, i64, i64)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT path, size_bytes, revision FROM docker_objects WHERE repository_id = $1",
        )
        .bind(repository)
        .fetch_all(database)
        .await
    }

    /// Corrects a size only when the row has not changed since reconciliation observed it.
    pub async fn update_size_if_revision(
        database: &PgPool,
        repository: Uuid,
        path: &str,
        size: u64,
        expected_revision: i64,
    ) -> Result<bool, sqlx::Error> {
        let size = i64::try_from(size).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        let result = sqlx::query(
            "UPDATE docker_objects
             SET size_bytes = $1, revision = revision + 1
             WHERE repository_id = $2 AND path = $3 AND revision = $4",
        )
        .bind(size)
        .bind(repository)
        .bind(path)
        .bind(expected_revision)
        .execute(database)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Rewrites an observed object graph only when its accounting revision is unchanged.
    pub async fn upsert_if_revision(
        database: &PgPool,
        repository: Uuid,
        path: &str,
        size: u64,
        references: &[String],
        expected_revision: i64,
    ) -> Result<bool, sqlx::Error> {
        let size = i64::try_from(size).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        let result = sqlx::query(
            "UPDATE docker_objects
             SET size_bytes = $1, references_paths = $2, revision = revision + 1
             WHERE repository_id = $3 AND path = $4 AND revision = $5",
        )
        .bind(size)
        .bind(references)
        .bind(repository)
        .bind(path)
        .bind(expected_revision)
        .execute(database)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Removes rows only when their revisions still match the reconciliation snapshot.
    pub async fn delete_paths_if_revisions(
        database: &PgPool,
        repository: Uuid,
        rows: &[(String, i64)],
    ) -> Result<usize, sqlx::Error> {
        if rows.is_empty() {
            return Ok(0);
        }
        let paths: Vec<&str> = rows.iter().map(|(path, _)| path.as_str()).collect();
        let revisions: Vec<i64> = rows.iter().map(|(_, revision)| *revision).collect();
        let result = sqlx::query(
            "DELETE FROM docker_objects AS object
             USING unnest($2::text[], $3::bigint[]) AS candidate(path, revision)
             WHERE object.repository_id = $1
               AND object.path = candidate.path
               AND object.revision = candidate.revision",
        )
        .bind(repository)
        .bind(paths)
        .bind(revisions)
        .execute(database)
        .await?;
        Ok(result.rows_affected() as usize)
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
