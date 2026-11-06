use ahash::HashMap;
use sqlx::{PgConnection, PgExecutor, PgPool, types::Json};

use uuid::Uuid;

/// Persists npm tags and deprecation metadata stored in the shared project tables.
/// Request parsing and npm semantic validation belong to the registry layer.
pub struct DBNpmProjectMetadata;

impl DBNpmProjectMetadata {
    /// Locks the project's tag state before a caller reads or modifies it.
    pub async fn lock_project(
        database: &mut PgConnection,
        project: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT id FROM projects WHERE id = $1 FOR UPDATE")
            .bind(project)
            .execute(database)
            .await?;
        Ok(())
    }

    /// Sets a tag after checking its target while the caller holds the project lock.
    pub async fn set_tag(
        project: Uuid,
        tag: String,
        version: String,
        fallback: HashMap<String, String>,
        database: &mut PgConnection,
    ) -> Result<bool, sqlx::Error> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM project_versions WHERE project_id = $1 AND version = $2)",
        )
        .bind(project)
        .bind(&version)
        .fetch_one(&mut *database)
        .await?;
        if !exists {
            return Ok(false);
        }
        Self::merge_tags(
            project,
            HashMap::from_iter([(tag, version)]),
            fallback,
            database,
        )
        .await?;
        Ok(true)
    }

    /// Clears only tags that refer to versions removed in the caller's transaction.
    /// If the deleted version was the inferred latest, stores an explicit empty map.
    pub async fn clear_deleted_version_tags(
        database: &mut PgConnection,
        project: Uuid,
        version: &str,
        clear_inferred_latest: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE projects SET npm_dist_tags = COALESCE(
                (SELECT jsonb_object_agg(key, value) FROM jsonb_each_text(npm_dist_tags)
                 WHERE value <> $2), '{}'::jsonb)
             WHERE id = $1 AND (npm_dist_tags IS NOT NULL OR $3)",
        )
        .bind(project)
        .bind(version)
        .bind(clear_inferred_latest)
        .execute(database)
        .await?;
        Ok(())
    }

    /// Loads npm distribution tags. NULL identifies packages published before tag storage.
    pub async fn load_tags<'e>(
        project: Uuid,
        database: impl PgExecutor<'e>,
    ) -> Result<Option<HashMap<String, String>>, sqlx::Error> {
        let tags: Option<Json<HashMap<String, String>>> =
            sqlx::query_scalar("SELECT npm_dist_tags FROM projects WHERE id = $1")
                .bind(project)
                .fetch_one(database)
                .await?;
        Ok(tags.map(|Json(tags)| tags))
    }

    /// Atomically merges npm tags, initializing legacy storage from the supplied fallback.
    pub async fn merge_tags<'e>(
        project: Uuid,
        tags: HashMap<String, String>,
        fallback: HashMap<String, String>,
        database: impl PgExecutor<'e>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE projects SET npm_dist_tags = COALESCE(npm_dist_tags, $2) || $3 WHERE id = $1",
        )
        .bind(project)
        .bind(Json(fallback))
        .bind(Json(tags))
        .execute(database)
        .await?;
        Ok(())
    }

    /// Atomically removes one npm tag without restoring concurrently removed tags.
    pub async fn remove_tag<'e>(
        project: Uuid,
        tag: &str,
        fallback: HashMap<String, String>,
        database: impl PgExecutor<'e>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE projects SET npm_dist_tags = COALESCE(npm_dist_tags, $2) - $3 WHERE id = $1",
        )
        .bind(project)
        .bind(Json(fallback))
        .bind(tag)
        .execute(database)
        .await?;
        Ok(())
    }

    /// Reads legacy npm release candidates in upload order without loading manifests.
    /// The caller validates semantic versions before choosing a stable release.
    pub async fn stable_version_candidates<'e>(
        project: Uuid,
        database: impl PgExecutor<'e>,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT version FROM project_versions
             WHERE project_id = $1 AND split_part(version, '+', 1) NOT LIKE '%-%'
             ORDER BY created_at DESC, id DESC",
        )
        .bind(project)
        .fetch_all(database)
        .await
    }

    /// Updates only npm deprecation messages in one transaction.
    /// Returns false without changes if any requested version has no npm metadata.
    pub async fn update_deprecations(
        project: Uuid,
        updates: HashMap<String, String>,
        database: &PgPool,
    ) -> Result<bool, sqlx::Error> {
        let mut transaction = database.begin().await?;
        Self::lock_project(&mut transaction, project).await?;
        let versions: Vec<_> = updates.keys().cloned().collect();
        let existing: Vec<String> = sqlx::query_scalar(
            "SELECT version FROM project_versions WHERE project_id = $1 AND version = ANY($2)
             AND jsonb_typeof(extra->'extra') = 'object' ORDER BY version FOR UPDATE",
        )
        .bind(project)
        .bind(&versions)
        .fetch_all(&mut *transaction)
        .await?;
        if existing.len() != updates.len() {
            return Ok(false);
        }
        for (version, message) in updates {
            sqlx::query(
                "UPDATE project_versions
                 SET extra = jsonb_set(extra, '{extra,deprecated}', to_jsonb($3::text)), updated_at = NOW()
                 WHERE project_id = $1 AND version = $2",
            )
            .bind(project)
            .bind(version)
            .bind(message)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query("UPDATE projects SET updated_at = NOW() WHERE id = $1")
            .bind(project)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(true)
    }
}
