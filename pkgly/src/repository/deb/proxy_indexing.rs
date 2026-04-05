use async_trait::async_trait;
use bytes::Bytes;
use nr_core::{
    database::entities::project::{
        DBProject, NewProject, ProjectDBType,
        versions::{DBProjectVersion, NewVersion},
    },
    repository::project::{DebPackageMetadata, ReleaseType, VersionData},
    storage::StoragePath,
};
use serde_json::to_value;
use thiserror::Error;
use tracing::instrument;
use uuid::Uuid;

use crate::{app::Pkgly, utils::ResponseBuilder};

#[derive(Debug, Error)]
pub enum DebProxyIndexingError {
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl crate::utils::IntoErrorResponse for DebProxyIndexingError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        ResponseBuilder::internal_server_error().body(format!("Deb proxy indexing error: {}", self))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebProxyPackageRecord {
    pub package_name: String,
    pub package_key: String,
    pub version: String,
    pub metadata: DebPackageMetadata,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DebProxyIndexing: Send + Sync {
    async fn record_cached_deb(
        &self,
        record: DebProxyPackageRecord,
    ) -> Result<(), DebProxyIndexingError>;
}

#[derive(Clone)]
pub struct DatabaseDebProxyIndexer {
    site: Pkgly,
    repository_id: Uuid,
}

impl DatabaseDebProxyIndexer {
    pub fn new(site: Pkgly, repository_id: Uuid) -> Self {
        Self {
            site,
            repository_id,
        }
    }

    async fn ensure_project(
        &self,
        record: &DebProxyPackageRecord,
    ) -> Result<DBProject, sqlx::Error> {
        let db = &self.site.database;
        if let Some(project) =
            DBProject::find_by_project_key(&record.package_key, self.repository_id, db).await?
        {
            return Ok(project);
        }

        let new_project = NewProject {
            scope: None,
            project_key: record.package_key.clone(),
            name: record.package_name.clone(),
            description: None,
            repository: self.repository_id,
            storage_path: format!("deb/{}/", record.package_key),
        };
        new_project.insert(db).await
    }

    async fn delete_version_by_id(&self, version_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM project_versions WHERE id = $1")
            .bind(version_id)
            .execute(&self.site.database)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl DebProxyIndexing for DatabaseDebProxyIndexer {
    #[instrument(skip(self, record), fields(repository_id = %self.repository_id, package = %record.package_key, version = %record.version))]
    async fn record_cached_deb(
        &self,
        record: DebProxyPackageRecord,
    ) -> Result<(), DebProxyIndexingError> {
        let db = &self.site.database;
        let project = self.ensure_project(&record).await?;

        if let Some(existing) =
            DBProjectVersion::find_by_version_and_project(&record.version, project.id, db).await?
        {
            self.delete_version_by_id(existing.id).await?;
        }

        let version_path = record
            .metadata
            .filename
            .trim()
            .trim_matches('/')
            .to_string();
        let description = record
            .metadata
            .description
            .clone()
            .and_then(|value| value.lines().next().map(str::to_owned));

        let version_data = VersionData {
            description,
            extra: Some(to_value(&record.metadata)?),
            ..Default::default()
        };

        let new_version = NewVersion {
            project_id: project.id,
            repository_id: self.repository_id,
            version: record.version.clone(),
            release_type: ReleaseType::release_type_from_version(&record.version),
            version_path,
            publisher: None,
            version_page: None,
            extra: version_data,
        };
        new_version.insert(db).await?;
        Ok(())
    }
}

fn parse_dependency_list(value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn deb_proxy_record_from_deb_bytes(
    path: &StoragePath,
    bytes: Bytes,
) -> Result<Option<DebProxyPackageRecord>, super::package::DebPackageError> {
    if path.is_directory() {
        return Ok(None);
    }
    let path_string = path.to_string();
    if !path_string.to_ascii_lowercase().ends_with(".deb") {
        return Ok(None);
    }

    let parsed = super::package::parse_deb_package(bytes)?;
    let Some(package) = parsed.control.get("Package") else {
        return Ok(None);
    };
    let Some(version) = parsed.control.get("Version") else {
        return Ok(None);
    };
    let Some(architecture) = parsed.control.get("Architecture") else {
        return Ok(None);
    };

    let package_name = package.to_string();
    let package_key = format!("{}:{}", package.to_lowercase(), architecture.to_lowercase());

    let metadata = DebPackageMetadata {
        distribution: "unknown".to_string(),
        component: "unknown".to_string(),
        architecture: architecture.to_string(),
        filename: path_string,
        size: parsed.file_size,
        md5: parsed.md5,
        sha1: parsed.sha1,
        sha256: parsed.sha256,
        section: parsed.control.get("Section").map(str::to_owned),
        priority: parsed.control.get("Priority").map(str::to_owned),
        maintainer: parsed.control.get("Maintainer").map(str::to_owned),
        installed_size: parsed
            .control
            .get("Installed-Size")
            .and_then(|value| value.parse::<u64>().ok()),
        depends: parse_dependency_list(parsed.control.get("Depends")),
        homepage: parsed.control.get("Homepage").map(str::to_owned),
        description: parsed.control.get("Description").map(str::to_owned),
    };

    Ok(Some(DebProxyPackageRecord {
        package_name,
        package_key,
        version: version.to_string(),
        metadata,
    }))
}

pub async fn record_deb_proxy_cache_hit(
    indexer: &dyn DebProxyIndexing,
    path: &StoragePath,
    bytes: Bytes,
    _upstream: Option<&url::Url>,
) -> Result<(), DebProxyIndexingError> {
    let Ok(Some(record)) = deb_proxy_record_from_deb_bytes(path, bytes) else {
        return Ok(());
    };
    indexer.record_cached_deb(record).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
