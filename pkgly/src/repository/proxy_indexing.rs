use async_trait::async_trait;
use nr_core::{
    database::entities::project::{
        DBProject, NewProject, ProjectDBType,
        versions::{DBProjectVersion, NewVersion},
    },
    repository::project::{ProxyArtifactKey, ProxyArtifactMeta, ReleaseType, VersionData},
};
use thiserror::Error;
use tracing::{instrument, warn};
use uuid::Uuid;

use crate::{app::Pkgly, utils::ResponseBuilder};

#[derive(Debug, Error)]
pub enum ProxyIndexingError {
    #[error("proxy metadata missing version for package {package}")]
    MissingVersion { package: String },
    #[error("proxy metadata missing package key")]
    MissingPackageKey,
    #[error("proxy metadata missing cache path")]
    MissingCachePath,
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ProxyIndexing: Send + Sync {
    async fn record_cached_artifact(
        &self,
        meta: ProxyArtifactMeta,
    ) -> Result<(), ProxyIndexingError>;

    async fn evict_cached_artifact(&self, key: ProxyArtifactKey) -> Result<(), ProxyIndexingError>;
}

#[derive(Clone)]
pub struct DatabaseProxyIndexer {
    site: Pkgly,
    repository_id: Uuid,
}

impl DatabaseProxyIndexer {
    pub fn new(site: Pkgly, repository_id: Uuid) -> Self {
        Self {
            site,
            repository_id,
        }
    }

    async fn ensure_project(
        &self,
        meta: &ProxyArtifactMeta,
    ) -> Result<DBProject, ProxyIndexingError> {
        let db = &self.site.database;
        if let Some(project) =
            DBProject::find_by_project_key(&meta.package_key, self.repository_id, db).await?
        {
            return Ok(project);
        }

        let new_project = NewProject {
            scope: None,
            project_key: meta.package_key.clone(),
            name: meta.package_name.clone(),
            description: None,
            repository: self.repository_id,
            storage_path: format!("{}/", meta.package_key),
        };
        let project = new_project.insert(db).await?;
        Ok(project)
    }

    fn normalize_version_path(path: &str, package_key: &str, version: &str) -> String {
        let trimmed = path.trim().trim_matches('/');
        if trimmed.is_empty() {
            return format!("{package_key}/{version}");
        }
        trimmed.to_lowercase()
    }

    async fn delete_version_by_id(&self, version_id: Uuid) -> Result<(), ProxyIndexingError> {
        sqlx::query("DELETE FROM project_versions WHERE id = $1")
            .bind(version_id)
            .execute(&self.site.database)
            .await?;
        Ok(())
    }

    async fn delete_version_by_path(&self, path: &str) -> Result<u64, ProxyIndexingError> {
        let normalized = path.trim().trim_matches('/').to_lowercase();
        if normalized.is_empty() {
            return Ok(0);
        }
        let outcome = sqlx::query(
            r#"
            DELETE FROM project_versions
            WHERE repository_id = $1
              AND LOWER(path) = $2
            "#,
        )
        .bind(self.repository_id)
        .bind(normalized)
        .execute(&self.site.database)
        .await?;
        Ok(outcome.rows_affected())
    }
}

#[async_trait]
impl ProxyIndexing for DatabaseProxyIndexer {
    #[instrument(skip(self), fields(repository_id = %self.repository_id))]
    async fn record_cached_artifact(
        &self,
        meta: ProxyArtifactMeta,
    ) -> Result<(), ProxyIndexingError> {
        let version = meta
            .version
            .clone()
            .ok_or_else(|| ProxyIndexingError::MissingVersion {
                package: meta.package_name.clone(),
            })?;
        if meta.package_key.trim().is_empty() {
            return Err(ProxyIndexingError::MissingPackageKey);
        }
        if meta.cache_path.trim().is_empty() {
            return Err(ProxyIndexingError::MissingCachePath);
        }

        let project = self.ensure_project(&meta).await?;
        let db = &self.site.database;

        if let Some(existing) =
            DBProjectVersion::find_by_version_and_project(&version, project.id, db).await?
        {
            self.delete_version_by_id(existing.id).await?;
        }

        let mut version_data = VersionData::default();
        version_data.set_proxy_artifact(&meta)?;

        let version_path =
            Self::normalize_version_path(&meta.cache_path, &meta.package_key, &version);

        let new_version = NewVersion {
            project_id: project.id,
            repository_id: self.repository_id,
            version: version.clone(),
            release_type: ReleaseType::release_type_from_version(&version),
            version_path,
            publisher: None,
            version_page: None,
            extra: version_data,
        };
        new_version.insert(db).await?;
        Ok(())
    }

    #[instrument(skip(self), fields(repository_id = %self.repository_id))]
    async fn evict_cached_artifact(&self, key: ProxyArtifactKey) -> Result<(), ProxyIndexingError> {
        if key.package_key.trim().is_empty() {
            return Err(ProxyIndexingError::MissingPackageKey);
        }
        let project = match DBProject::find_by_project_key(
            &key.package_key,
            self.repository_id,
            &self.site.database,
        )
        .await?
        {
            Some(project) => project,
            None => return Ok(()),
        };

        if let Some(version) = key.version.as_ref() {
            sqlx::query(
                r#"
                DELETE FROM project_versions
                WHERE repository_id = $1
                  AND project_id = $2
                  AND version = $3
                "#,
            )
            .bind(self.repository_id)
            .bind(project.id)
            .bind(version)
            .execute(&self.site.database)
            .await?;
        } else if let Some(path) = key.cache_path.as_ref() {
            let removed = self.delete_version_by_path(path).await?;
            if removed == 0 {
                warn!(
                    repository_id = %self.repository_id,
                    package = %key.package_key,
                    path,
                    "Proxy eviction had no matching catalog rows"
                );
            }
        }
        Ok(())
    }
}

impl crate::utils::IntoErrorResponse for ProxyIndexingError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        ResponseBuilder::internal_server_error().body(format!("Proxy indexing error: {}", self))
    }
}
