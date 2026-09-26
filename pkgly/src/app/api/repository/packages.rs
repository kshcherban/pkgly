// ABOUTME: Serves admin package listing and deletion APIs for repository contents.
// ABOUTME: Maps catalog and storage metadata into package table rows for all repository types.
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, FixedOffset};
use http::header::HeaderValue;
use nr_storage::{DynStorage, FileType, Storage, StorageError, StorageFile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::io::AsyncReadExt;
use tracing::{debug, instrument, warn};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    app::{
        Pkgly,
        authentication::Authentication,
        responses::{MissingPermission, RepositoryNotFound},
        webhooks::{self, PackageWebhookActor, PackageWebhookSnapshot, WebhookEventType},
    },
    error::{InternalError, OtherInternalError},
    repository::{
        DynRepository, Repository,
        docker::{
            DockerRegistry,
            metadata::{backfill_manifest_objects, docker_package_key, split_manifest_cache_path},
            types::{Manifest as DockerManifest, MediaType},
        },
        go::GoRepository,
        helm::hosted::HelmHosted,
        helm::{DeletePackageEntry, HelmRepository, HelmRepositoryError},
        npm::NPMRegistry,
        proxy_indexing::{ProxyIndexing, ProxyIndexingError},
        python::PythonRepository,
        utils::can_read_repository_with_auth,
    },
    utils::ResponseBuilder,
};
use ahash::{HashSet, HashSetExt};
use nr_core::user::permissions::{HasPermissions, RepositoryActions};
use nr_core::{
    database::entities::package_file::{
        DBPackageFile, PackageFileListParams, PackageFileSortBy, SortDirection,
    },
    repository::project::ProxyArtifactKey,
    storage::StoragePath,
};

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PackageListQuery {
    #[serde(default = "default_page")]
    #[param(default = 1)]
    pub page: usize,
    #[serde(default = "default_per_page")]
    #[param(default = 50)]
    pub per_page: usize,
    /// Optional search term applied server-side across all repository packages.
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub sort_by: PackageSortBy,
    #[serde(default)]
    pub sort_dir: PackageSortDirection,
}

const fn default_page() -> usize {
    1
}
const fn default_per_page() -> usize {
    50
}

const MAX_PER_PAGE: usize = 1000;

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageSortBy {
    Modified,
    Package,
    Name,
    Size,
    Path,
    Digest,
}

impl Default for PackageSortBy {
    fn default() -> Self {
        Self::Modified
    }
}

impl From<PackageSortBy> for PackageFileSortBy {
    fn from(value: PackageSortBy) -> Self {
        match value {
            PackageSortBy::Modified => PackageFileSortBy::Modified,
            PackageSortBy::Package => PackageFileSortBy::Package,
            PackageSortBy::Name => PackageFileSortBy::Name,
            PackageSortBy::Size => PackageFileSortBy::Size,
            PackageSortBy::Path => PackageFileSortBy::Path,
            PackageSortBy::Digest => PackageFileSortBy::Digest,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageSortDirection {
    Asc,
    Desc,
}

impl Default for PackageSortDirection {
    fn default() -> Self {
        Self::Desc
    }
}

impl From<PackageSortDirection> for SortDirection {
    fn from(value: PackageSortDirection) -> Self {
        match value {
            PackageSortDirection::Asc => SortDirection::Asc,
            PackageSortDirection::Desc => SortDirection::Desc,
        }
    }
}

fn normalize_search_term(term: &Option<String>) -> Option<String> {
    term.as_ref()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
}

const GO_FILE_SUFFIXES: [&str; 3] = [".zip", ".mod", ".info"];

#[derive(Debug, Serialize, ToSchema, Clone)]
pub struct PackageFileEntry {
    pub package: String,
    pub name: String,
    pub cache_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_digest: Option<String>,
    pub size: u64,
    pub modified: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PackageListResponse {
    pub page: usize,
    pub per_page: usize,
    pub total_packages: usize,
    pub items: Vec<PackageFileEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageStrategy {
    PackagesDirectory { base: Option<&'static str> },
    MavenHosted,
    MavenProxy,
    PythonHosted,
    PythonProxy,
    PhpHosted,
    PhpProxy,
    DockerHosted,
    DockerProxy,
    Helm,
    GoHosted,
    GoProxy,
    Cargo,
    DebHosted,
    NpmProxy,
    NpmHosted,
    NpmVirtual,
    NugetHosted,
    NugetProxy,
    NugetVirtual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogDeletionMode {
    None,
    ExactPath,
    StripLastSegment,
}

fn package_strategy(repository: &DynRepository) -> PackageStrategy {
    match repository {
        DynRepository::Maven(maven_repo) => match maven_repo {
            crate::repository::maven::MavenRepository::Hosted(_) => PackageStrategy::MavenHosted,
            crate::repository::maven::MavenRepository::Proxy(_) => PackageStrategy::MavenProxy,
        },
        DynRepository::Python(python_repo) => match python_repo {
            crate::repository::python::PythonRepository::Hosted(_) => PackageStrategy::PythonHosted,
            crate::repository::python::PythonRepository::Proxy(_) => PackageStrategy::PythonProxy,
            crate::repository::python::PythonRepository::Virtual(_) => {
                PackageStrategy::PythonHosted
            }
        },
        DynRepository::Php(php_repo) => match php_repo {
            crate::repository::php::PhpRepository::Hosted(_) => PackageStrategy::PhpHosted,
            crate::repository::php::PhpRepository::Proxy(_) => PackageStrategy::PhpProxy,
        },
        DynRepository::Helm(_) => PackageStrategy::Helm,
        DynRepository::NPM(npm_repo) => match npm_repo {
            crate::repository::npm::NPMRegistry::Hosted(_) => PackageStrategy::NpmHosted,
            crate::repository::npm::NPMRegistry::Proxy(_) => PackageStrategy::NpmProxy,
            crate::repository::npm::NPMRegistry::Virtual(_) => PackageStrategy::NpmVirtual,
        },
        DynRepository::Docker(docker_repo) => match docker_repo {
            crate::repository::docker::DockerRegistry::Hosted(_) => PackageStrategy::DockerHosted,
            crate::repository::docker::DockerRegistry::Proxy(_) => PackageStrategy::DockerProxy,
        },
        DynRepository::Cargo(_) => PackageStrategy::Cargo,
        DynRepository::Deb(_) => PackageStrategy::DebHosted,
        DynRepository::Go(go_repo) => match go_repo {
            crate::repository::go::GoRepository::Hosted(_) => PackageStrategy::GoHosted,
            crate::repository::go::GoRepository::Proxy(_) => PackageStrategy::GoProxy,
        },
        DynRepository::Ruby(_) => PackageStrategy::PackagesDirectory { base: Some("gems") },
        DynRepository::Nuget(nuget_repo) => match nuget_repo {
            crate::repository::nuget::NugetRepository::Hosted(_) => PackageStrategy::NugetHosted,
            crate::repository::nuget::NugetRepository::Proxy(_) => PackageStrategy::NugetProxy,
            crate::repository::nuget::NugetRepository::Virtual(_) => PackageStrategy::NugetVirtual,
        },
    }
}

fn catalog_deletion_mode(repository: &DynRepository) -> CatalogDeletionMode {
    match repository {
        DynRepository::Cargo(_) => CatalogDeletionMode::StripLastSegment,
        DynRepository::Python(python_repo) => match python_repo {
            crate::repository::python::PythonRepository::Hosted(_) => {
                CatalogDeletionMode::StripLastSegment
            }
            _ => CatalogDeletionMode::None,
        },
        DynRepository::NPM(npm_repo) => match npm_repo {
            crate::repository::npm::NPMRegistry::Hosted(_) => CatalogDeletionMode::StripLastSegment,
            crate::repository::npm::NPMRegistry::Virtual(_) => CatalogDeletionMode::None,
            _ => CatalogDeletionMode::None,
        },
        DynRepository::Php(_) => CatalogDeletionMode::ExactPath,
        DynRepository::Deb(_) => CatalogDeletionMode::ExactPath,
        DynRepository::Ruby(_) => CatalogDeletionMode::ExactPath,
        DynRepository::Maven(_) => CatalogDeletionMode::StripLastSegment,
        // Helm uses repository-specific delete handlers that already update the catalog.
        DynRepository::Helm(_) => CatalogDeletionMode::None,
        _ => CatalogDeletionMode::None,
    }
}

fn derive_version_path(cache_path: &str, mode: CatalogDeletionMode) -> Option<String> {
    match mode {
        CatalogDeletionMode::None => None,
        CatalogDeletionMode::ExactPath => {
            let trimmed = cache_path.trim().trim_end_matches('/');
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        CatalogDeletionMode::StripLastSegment => {
            let components: Vec<String> = StoragePath::from(cache_path)
                .into_iter()
                .map(String::from)
                .collect();
            if components.len() <= 1 {
                return None;
            }
            let stripped = components[..components.len() - 1].join("/");
            if stripped.is_empty() {
                None
            } else {
                Some(stripped)
            }
        }
    }
}

fn normalize_catalog_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.trim_end_matches('/').to_lowercase())
}

fn normalize_catalog_paths(version_paths: &HashSet<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(version_paths.len());
    for path in version_paths {
        if let Some(value) = normalize_catalog_path(path) {
            normalized.push(value);
        }
    }
    normalized.sort();
    normalized.dedup();
    normalized
}

async fn delete_version_records_by_path(
    database: &PgPool,
    repository_id: Uuid,
    version_paths: &HashSet<String>,
) -> Result<u64, sqlx::Error> {
    let normalized = normalize_catalog_paths(version_paths);
    if normalized.is_empty() {
        return Ok(0);
    }

    sql_delete_project_versions(database, repository_id, normalized).await
}

async fn sql_delete_project_versions(
    database: &PgPool,
    repository_id: Uuid,
    normalized_paths: Vec<String>,
) -> Result<u64, sqlx::Error> {
    let mut total_deleted = 0u64;
    for path in normalized_paths {
        let rows = sqlx::query(
            r#"
            DELETE FROM project_versions
            WHERE repository_id = $1
              AND LOWER(path) = $2
            RETURNING id
            "#,
        )
        .bind(repository_id)
        .bind(&path)
        .fetch_all(database)
        .await?;
        total_deleted += rows.len() as u64;
    }

    Ok(total_deleted)
}

pub fn package_routes() -> axum::Router<Pkgly> {
    axum::Router::new().route(
        "/{repository_id}/packages",
        get(list_cached_packages).delete(delete_cached_packages),
    )
}

#[utoipa::path(
    get,
    path = "/{repository_id}/packages",
    params(
        PackageListQuery,
        ("repository_id" = Uuid, Path, description = "The Repository ID"),
    ),
    responses(
        (status = 200, description = "Cached package listing", body = PackageListResponse),
        (status = 404, description = "Repository or packages not found"),
        (status = 403, description = "Missing permission")
    )
)]
#[instrument(skip(site, auth, query), fields(repository_id = %repository_id))]
pub async fn list_cached_packages(
    State(site): State<Pkgly>,
    auth: Option<Authentication>,
    Path(repository_id): Path<Uuid>,
    Query(query): Query<PackageListQuery>,
) -> Result<Response, InternalError> {
    let Some(repository) = site.get_repository(repository_id) else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };
    let search_term = normalize_search_term(&query.q);
    let auth_config = site.get_repository_auth_config(repository.id()).await?;
    if !can_read_repository_with_auth(
        &auth,
        repository.visibility(),
        repository.id(),
        site.as_ref(),
        &auth_config,
    )
    .await?
    {
        return Ok(MissingPermission::ReadRepository(repository.id()).into_response());
    }
    let current_page = query.page.max(1);
    let per_page = query.per_page.clamp(1, MAX_PER_PAGE);
    let params = PackageFileListParams {
        repository_id: repository.id(),
        page: current_page,
        per_page,
        search: search_term.clone(),
        sort_by: query.sort_by.into(),
        sort_dir: query.sort_dir.into(),
    };
    let (total_packages, rows) =
        DBPackageFile::list_repository_page(&site.database, &params).await?;
    let is_docker_repository = matches!(repository, DynRepository::Docker(_));
    let is_maven_repository = matches!(repository, DynRepository::Maven(_));
    let storage = repository.get_storage();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let mut entry = PackageFileEntry {
            package: row.package,
            name: row.name,
            cache_path: row.path.clone(),
            blob_digest: row.content_digest.or(row.upstream_digest),
            size: row.size_bytes.max(0) as u64,
            modified: row.modified_at,
        };

        if is_docker_repository {
            if let Some(size) = row.referenced_size_bytes {
                entry.size = size.max(0) as u64;
            } else {
                match backfill_manifest_objects(
                    &site.database,
                    &storage,
                    repository.id(),
                    &entry.cache_path,
                )
                .await
                {
                    Ok(Some(size)) => entry.size = size,
                    Ok(None) => {}
                    Err(err) => {
                        warn!(?err, cache_path = %entry.cache_path, "Failed to backfill Docker object accounting")
                    }
                }
            }
        } else if is_maven_repository {
            match calculate_stored_path_size(&storage, repository.id(), &entry.cache_path).await {
                Ok(Some(size)) => {
                    entry.size = size;
                }
                Ok(None) => {}
                Err(err) => {
                    warn!(
                        ?err,
                        cache_path = %entry.cache_path,
                        "Failed to calculate Maven stored path size"
                    );
                }
            }
        }

        items.push(entry);
    }
    let response_body = PackageListResponse {
        page: current_page,
        per_page,
        total_packages,
        items,
    };
    let mut response = ResponseBuilder::ok().json(&response_body);
    let has_index_rows =
        DBPackageFile::repository_has_rows(&site.database, repository.id()).await?;
    let repository_name = repository.name();

    if !has_index_rows {
        if let Ok(value) = HeaderValue::from_str(&format!(
            "Repository awaiting indexing: {}",
            repository_name
        )) {
            response.headers_mut().insert("X-Pkgly-Warning", value);
        }
    }

    Ok(response)
}

fn should_ignore(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".nr-meta")
}

async fn calculate_stored_path_size(
    storage: &DynStorage,
    repository_id: Uuid,
    cache_path: &str,
) -> Result<Option<u64>, StorageError> {
    let storage_path = StoragePath::from(cache_path);
    let Some(file) = storage.open_file(repository_id, &storage_path).await? else {
        return Ok(None);
    };

    match file {
        StorageFile::File { meta, .. } => Ok(Some(meta.file_type.file_size)),
        StorageFile::Directory { files, .. } => {
            let total = files
                .iter()
                .filter(|entry| !should_ignore(entry.name()))
                .filter_map(|entry| match entry.file_type() {
                    FileType::File(file_meta) => Some(file_meta.file_size),
                    FileType::Directory(_) => None,
                })
                .sum();
            Ok(Some(total))
        }
    }
}

struct GoDeletionResult {
    removed: usize,
    missing: Vec<String>,
}

fn go_related_paths(path: &str) -> Option<Vec<String>> {
    for suffix in GO_FILE_SUFFIXES.iter() {
        if let Some(base) = path.strip_suffix(suffix) {
            let mut paths = Vec::with_capacity(GO_FILE_SUFFIXES.len());
            for candidate in GO_FILE_SUFFIXES.iter() {
                paths.push(format!("{}{}", base, candidate));
            }
            return Some(paths);
        }
    }
    None
}

async fn delete_go_package(
    storage: &nr_storage::DynStorage,
    repository_id: Uuid,
    path: &str,
) -> Result<Option<GoDeletionResult>, nr_storage::StorageError> {
    let Some(paths) = go_related_paths(path) else {
        return Ok(None);
    };

    let mut removed = 0usize;
    let mut missing = Vec::new();

    for related_path in paths.iter() {
        let storage_path = nr_core::storage::StoragePath::from(related_path.as_str());
        match storage.delete_file(repository_id, &storage_path).await {
            Ok(true) => removed += 1,
            Ok(false) => missing.push(related_path.clone()),
            Err(err) => {
                missing.push(related_path.clone());
                return Err(err);
            }
        }
    }

    Ok(Some(GoDeletionResult { removed, missing }))
}

fn is_valid_cache_path(path: &str, strategy: PackageStrategy) -> bool {
    match strategy {
        PackageStrategy::PackagesDirectory { base } => {
            if let Some(prefix) = base {
                path.starts_with(prefix) && is_valid_repository_path(path)
            } else {
                is_valid_repository_path(path)
            }
        }
        PackageStrategy::NpmProxy => {
            path.starts_with("packages/") && is_valid_repository_path(path)
        }
        PackageStrategy::NpmHosted | PackageStrategy::NpmVirtual => {
            path.starts_with("packages/") && is_valid_repository_path(path)
        }
        PackageStrategy::MavenHosted
        | PackageStrategy::PhpHosted
        | PackageStrategy::PhpProxy
        | PackageStrategy::MavenProxy
        | PackageStrategy::PythonHosted
        | PackageStrategy::PythonProxy
        | PackageStrategy::Cargo
        | PackageStrategy::NugetHosted
        | PackageStrategy::NugetProxy
        | PackageStrategy::NugetVirtual => is_valid_repository_path(path),
        PackageStrategy::DockerHosted | PackageStrategy::DockerProxy => {
            is_valid_docker_manifest_path(path)
        }
        PackageStrategy::Helm => {
            if !(path.starts_with("charts/") || path.starts_with("v2/")) {
                return false;
            }
            is_valid_repository_path(path)
        }
        PackageStrategy::GoHosted | PackageStrategy::GoProxy | PackageStrategy::DebHosted => {
            is_valid_repository_path(path)
        }
    }
}

fn is_valid_repository_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.starts_with('/') || path.contains("..") {
        return false;
    }
    true
}

fn is_valid_docker_manifest_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains("..") {
        return false;
    }
    path.starts_with("v2/") && path.contains("/manifests/")
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DockerDeletionResult {
    pub removed_manifests: usize,
    pub removed_blobs: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum DockerDeletionError {
    #[error("manifest not found")]
    ManifestMissing,
    #[error("invalid manifest path")]
    InvalidManifestPath,
    #[error("storage error: {0}")]
    Storage(#[from] nr_storage::StorageError),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("indexing error: {0}")]
    Indexing(#[from] ProxyIndexingError),
    #[error("accounting error: {0}")]
    Accounting(#[from] sqlx::Error),
}

fn docker_proxy_key_from_path(path: &str) -> Option<ProxyArtifactKey> {
    let (repository, reference) = split_manifest_cache_path(path)?;
    Some(ProxyArtifactKey {
        package_key: docker_package_key(&repository),
        version: Some(reference),
        cache_path: Some(path.to_string()),
    })
}

/// Deletes a Docker manifest graph and removes successfully deleted inventory entries.
/// Returns storage, catalog, or accounting errors without suppressing failures.
pub async fn delete_docker_package(
    storage: &nr_storage::DynStorage,
    repository_id: Uuid,
    cache_path: &str,
    indexer: Option<&dyn ProxyIndexing>,
    database: Option<&PgPool>,
) -> Result<DockerDeletionResult, DockerDeletionError> {
    let (repository_name, _) =
        split_manifest_cache_path(cache_path).ok_or(DockerDeletionError::InvalidManifestPath)?;

    let mut visited_manifests = HashSet::new();
    let mut paths_to_delete = HashSet::new();
    let mut stack = Vec::new();
    stack.push(cache_path.to_string());

    // First pass: collect all paths to delete
    while let Some(current_path) = stack.pop() {
        match collect_manifest_paths(
            storage,
            repository_id,
            &repository_name,
            &current_path,
            &mut visited_manifests,
            &mut paths_to_delete,
        )
        .await
        {
            Ok(nested) => {
                stack.extend(nested);
            }
            Err(DockerDeletionError::ManifestMissing) if current_path != cache_path => {
                // Nested manifest already removed; skip silently.
            }
            Err(err) => return Err(err),
        }
    }

    if let Some(indexer) = indexer {
        for path in paths_to_delete.iter() {
            if let Some(key) = docker_proxy_key_from_path(path) {
                indexer.evict_cached_artifact(key).await?;
            }
        }
    }

    // Second pass: batch delete all collected paths
    let paths_vec: Vec<nr_core::storage::StoragePath> = paths_to_delete
        .iter()
        .map(|p| nr_core::storage::StoragePath::from(p.as_str()))
        .collect();

    let _deleted = storage
        .delete_files_batch(repository_id, &paths_vec)
        .await?;

    if let Some(database) = database {
        nr_core::database::entities::docker_object::DBDockerObject::delete_paths(
            database,
            repository_id,
            &paths_to_delete.iter().cloned().collect::<Vec<_>>(),
        )
        .await?;
    }

    // Count manifests vs blobs for the result
    let manifest_count = paths_to_delete
        .iter()
        .filter(|p| p.contains("/manifests/") && !p.ends_with(".nr-docker-tagmeta"))
        .count();
    let blob_count = paths_to_delete
        .iter()
        .filter(|p| p.contains("/blobs/"))
        .count();

    Ok(DockerDeletionResult {
        removed_manifests: manifest_count,
        removed_blobs: blob_count,
    })
}

const DOCKER_BATCH_DELETE_FLUSH_THRESHOLD: usize = 1_000;

#[derive(Debug, Default)]
struct DockerBatchDeletion {
    missing: Vec<String>,
    rejected: Vec<String>,
    deleted_packages: usize,
    deleted_objects: usize,
}

struct StreamingDockerBatchDeletion {
    storage: nr_storage::DynStorage,
    repository_id: Uuid,
    paths_to_delete: HashSet<String>,
    visited_manifests: HashSet<String>,
    flush_threshold: usize,
    missing: Vec<String>,
    rejected: Vec<String>,
    deleted_packages: usize,
    deleted_objects: usize,
    indexer: Option<Arc<dyn ProxyIndexing>>,
    database: Option<PgPool>,
}

impl StreamingDockerBatchDeletion {
    fn new(
        storage: &nr_storage::DynStorage,
        repository_id: Uuid,
        indexer: Option<Arc<dyn ProxyIndexing>>,
    ) -> Self {
        Self {
            storage: storage.clone(),
            repository_id,
            paths_to_delete: HashSet::new(),
            visited_manifests: HashSet::new(),
            flush_threshold: DOCKER_BATCH_DELETE_FLUSH_THRESHOLD,
            missing: Vec::new(),
            rejected: Vec::new(),
            deleted_packages: 0,
            deleted_objects: 0,
            indexer,
            database: None,
        }
    }

    async fn flush(&mut self) -> Result<(), DockerDeletionError> {
        if self.paths_to_delete.is_empty() {
            return Ok(());
        }

        let drained: Vec<String> = self.paths_to_delete.drain().collect();

        if let Some(indexer) = self.indexer.as_ref() {
            for path in drained.iter() {
                if let Some(key) = docker_proxy_key_from_path(path) {
                    indexer.evict_cached_artifact(key).await?;
                }
            }
        }

        let paths: Vec<_> = drained
            .iter()
            .map(|p| nr_core::storage::StoragePath::from(p.as_str()))
            .collect();

        let deleted = self
            .storage
            .delete_files_batch(self.repository_id, &paths)
            .await?;

        if let Some(database) = &self.database {
            nr_core::database::entities::docker_object::DBDockerObject::delete_paths(
                database,
                self.repository_id,
                &drained,
            )
            .await?;
        }
        self.deleted_objects += deleted;
        Ok(())
    }

    async fn flush_if_needed(&mut self) -> Result<(), DockerDeletionError> {
        if self.paths_to_delete.len() >= self.flush_threshold {
            self.flush().await?;
        }
        Ok(())
    }
}

impl From<StreamingDockerBatchDeletion> for DockerBatchDeletion {
    fn from(streaming: StreamingDockerBatchDeletion) -> Self {
        Self {
            missing: streaming.missing,
            rejected: streaming.rejected,
            deleted_packages: streaming.deleted_packages,
            deleted_objects: streaming.deleted_objects,
        }
    }
}

/// Collect deletion targets for multiple Docker manifests at once, deduplicating shared layers
/// and manifest digests to minimize downstream S3 delete calls.
#[instrument(
    name = "collect_docker_deletions_batch",
    skip(storage, paths, indexer),
    fields(repository_id = %repository_id, path_count = paths.len())
)]
async fn collect_docker_deletions_batch(
    storage: &nr_storage::DynStorage,
    repository_id: Uuid,
    paths: &[String],
    indexer: Option<Arc<dyn ProxyIndexing>>,
    database: Option<&PgPool>,
) -> Result<DockerBatchDeletion, DockerDeletionError> {
    let mut batch = StreamingDockerBatchDeletion::new(storage, repository_id, indexer);
    batch.database = database.cloned();

    for path in paths {
        if !is_valid_docker_manifest_path(path) {
            batch.rejected.push(path.clone());
            continue;
        }

        let (repository_name, _) = match split_manifest_cache_path(path) {
            Some(parts) => parts,
            None => {
                batch.rejected.push(path.clone());
                continue;
            }
        };

        let mut stack = Vec::new();
        stack.push(path.clone());
        let mut found_manifest = false;

        while let Some(current_path) = stack.pop() {
            match collect_manifest_paths(
                storage,
                repository_id,
                &repository_name,
                &current_path,
                &mut batch.visited_manifests,
                &mut batch.paths_to_delete,
            )
            .await
            {
                Ok(nested) => {
                    found_manifest = true;
                    stack.extend(nested);
                    batch.flush_if_needed().await?;
                }
                Err(DockerDeletionError::ManifestMissing) if current_path != *path => {
                    // Nested manifest already removed; ignore.
                }
                Err(DockerDeletionError::ManifestMissing) => {
                    batch.missing.push(path.clone());
                    found_manifest = false;
                    break;
                }
                Err(DockerDeletionError::InvalidManifestPath) => {
                    batch.rejected.push(path.clone());
                    found_manifest = false;
                    break;
                }
                Err(err) => {
                    // Treat parse/storage errors as missing for the user but stop processing this path.
                    warn!(?err, path, "Failed to collect docker manifest for deletion");
                    batch.missing.push(path.clone());
                    found_manifest = false;
                    break;
                }
            }
        }

        if found_manifest {
            batch.deleted_packages += 1;
        }
    }

    // Always attempt to delete tag metadata sidecars for the requested paths
    for path in paths {
        batch
            .paths_to_delete
            .insert(format!("{path}.nr-docker-tagmeta"));
        batch.flush_if_needed().await?;
    }

    batch.flush().await?;

    Ok(batch.into())
}

async fn delete_helm_package(
    site: &Pkgly,
    hosted: &HelmHosted,
    cache_path: &str,
) -> Result<bool, HelmRepositoryError> {
    let row = sqlx::query(
        r#"
        SELECT
            p.name AS chart_name,
            pv.version
        FROM project_versions pv
        INNER JOIN projects p ON pv.project_id = p.id
        WHERE p.repository_id = $1 AND LOWER(pv.path) = LOWER($2)
        "#,
    )
    .bind(hosted.id())
    .bind(cache_path)
    .fetch_optional(&site.database)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    let chart_name: String = row.try_get("chart_name")?;
    let version: String = row.try_get("version")?;
    let entry = DeletePackageEntry {
        name: chart_name,
        version,
    };
    let removed = hosted
        .delete_chart_versions(std::slice::from_ref(&entry), None)
        .await?;
    Ok(!removed.is_empty())
}

fn collect_blob_path(
    repository_name: &str,
    digest: &str,
    collected_blobs: &mut HashSet<String>,
    paths_to_delete: &mut HashSet<String>,
) {
    if collected_blobs.insert(digest.to_string()) {
        let blob_path = format!("v2/{}/blobs/{}", repository_name, digest);
        paths_to_delete.insert(blob_path);
    }
}

async fn collect_manifest_paths(
    storage: &nr_storage::DynStorage,
    repository_id: Uuid,
    repository_name: &str,
    cache_path: &str,
    visited_manifests: &mut HashSet<String>,
    paths_to_delete: &mut HashSet<String>,
) -> Result<Vec<String>, DockerDeletionError> {
    let storage_path = nr_core::storage::StoragePath::from(cache_path);
    let Some(file) = storage.open_file(repository_id, &storage_path).await? else {
        return Err(DockerDeletionError::ManifestMissing);
    };
    let nr_storage::StorageFile::File { meta, mut content } = file else {
        return Err(DockerDeletionError::InvalidManifest(
            "expected manifest file".to_string(),
        ));
    };

    // Read manifest content (manifests are typically small); if unexpectedly large, we still proceed
    let mut bytes = Vec::with_capacity(usize::try_from(meta.file_type.file_size).unwrap_or(0));
    content
        .read_to_end(&mut bytes)
        .await
        .map_err(|err| DockerDeletionError::InvalidManifest(err.to_string()))?;

    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    let manifest = DockerManifest::from_bytes(&bytes, MediaType::OCI_IMAGE_MANIFEST)
        .map_err(|err| DockerDeletionError::InvalidManifest(err.to_string()))?;

    let mut collected_blobs = HashSet::new();
    debug!(cache_path, digest = %manifest_digest, "Parsed manifest for deletion");

    // Add manifest paths to delete (and related tag metadata if present)
    paths_to_delete.insert(cache_path.to_string());
    // Tag meta sidecar created by docker proxy
    paths_to_delete.insert(format!("{cache_path}.nr-docker-tagmeta"));

    let digest_path = format!("v2/{}/manifests/{}", repository_name, manifest_digest);
    if digest_path != cache_path {
        paths_to_delete.insert(digest_path.clone());
        paths_to_delete.insert(format!("{digest_path}.nr-docker-tagmeta"));
    }

    let first_visit = visited_manifests.insert(manifest_digest.clone());
    if !first_visit {
        return Ok(Vec::new());
    }

    let mut nested = Vec::new();

    match manifest {
        DockerManifest::DockerV2(manifest) => {
            collect_blob_path(
                repository_name,
                &manifest.config.digest,
                &mut collected_blobs,
                paths_to_delete,
            );
            for layer in manifest.layers {
                collect_blob_path(
                    repository_name,
                    &layer.digest,
                    &mut collected_blobs,
                    paths_to_delete,
                );
            }
        }
        DockerManifest::OciImage(manifest) => {
            if let Some(config) = manifest.config {
                collect_blob_path(
                    repository_name,
                    &config.digest,
                    &mut collected_blobs,
                    paths_to_delete,
                );
            }
            for layer in manifest.layers {
                collect_blob_path(
                    repository_name,
                    &layer.digest,
                    &mut collected_blobs,
                    paths_to_delete,
                );
            }
        }
        DockerManifest::OciIndex(index) => {
            for descriptor in index.manifests {
                if !visited_manifests.contains(&descriptor.digest) {
                    nested.push(format!(
                        "v2/{}/manifests/{}",
                        repository_name, descriptor.digest
                    ));
                }
            }
        }
    }

    Ok(nested)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PackageDeleteRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PackageDeleteResponse {
    pub deleted: usize,
    pub missing: Vec<String>,
    pub rejected: Vec<String>,
}

pub async fn delete_cached_package_paths(
    site: &Pkgly,
    repository: DynRepository,
    paths: &[String],
    actor: PackageWebhookActor,
) -> Result<PackageDeleteResponse, InternalError> {
    let strategy = package_strategy(&repository);
    let helm_repository = if let PackageStrategy::Helm = strategy {
        match repository.clone() {
            DynRepository::Helm(HelmRepository::Hosted(hosted)) => Some(hosted),
            _ => None,
        }
    } else {
        None
    };
    let python_proxy = match repository.clone() {
        DynRepository::Python(PythonRepository::Proxy(proxy)) => Some(proxy),
        _ => None,
    };
    let php_proxy = match repository.clone() {
        DynRepository::Php(crate::repository::php::PhpRepository::Proxy(proxy)) => Some(proxy),
        _ => None,
    };
    let npm_proxy = match repository.clone() {
        DynRepository::NPM(NPMRegistry::Proxy(proxy)) => Some(proxy),
        _ => None,
    };
    let go_proxy = match repository.clone() {
        DynRepository::Go(GoRepository::Proxy(proxy)) => Some(proxy),
        _ => None,
    };
    let maven_proxy = match repository.clone() {
        DynRepository::Maven(crate::repository::maven::MavenRepository::Proxy(proxy)) => {
            Some(proxy)
        }
        _ => None,
    };
    let docker_proxy = match repository.clone() {
        DynRepository::Docker(DockerRegistry::Proxy(proxy)) => Some(proxy),
        _ => None,
    };
    let storage = repository.get_storage();
    let mut deleted = 0usize;
    let mut missing = Vec::new();
    let mut rejected = Vec::new();
    let mut deleted_paths: HashSet<String> = HashSet::new();
    let catalog_mode = catalog_deletion_mode(&repository);
    let mut catalog_targets: HashSet<String> = HashSet::new();
    let mut delete_webhook_snapshots: std::collections::HashMap<String, PackageWebhookSnapshot> =
        std::collections::HashMap::new();
    for path in paths {
        if let Some(snapshot) = webhooks::build_package_event_snapshot(
            site,
            repository.id(),
            WebhookEventType::PackageDeleted,
            path.clone(),
            actor.clone(),
            true,
        )
        .await
        .map_err(|err| {
            InternalError::from(OtherInternalError::new(std::io::Error::other(
                err.to_string(),
            )))
        })? {
            delete_webhook_snapshots.insert(path.clone(), snapshot);
        }
    }

    if matches!(
        strategy,
        PackageStrategy::DockerHosted | PackageStrategy::DockerProxy
    ) {
        let docker_indexer = docker_proxy.as_ref().map(|proxy| proxy.indexer().clone());
        let batch = collect_docker_deletions_batch(
            &storage,
            repository.id(),
            paths,
            docker_indexer,
            Some(&site.database),
        )
        .await
        .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;

        debug!(
            paths = paths.len(),
            deleted_packages = batch.deleted_packages,
            deleted_objects = batch.deleted_objects,
            missing = batch.missing.len(),
            rejected = batch.rejected.len(),
            "Docker deletion batch streamed"
        );

        let DockerBatchDeletion {
            deleted_packages: batch_deleted_packages,
            deleted_objects,
            missing: batch_missing,
            rejected: batch_rejected,
            ..
        } = batch;

        if deleted_objects > 0 {
            deleted += batch_deleted_packages;
            for path in paths.iter() {
                deleted_paths.insert(path.clone());
            }
        } else {
            missing.extend(paths.iter().cloned());
        }

        missing.extend(batch_missing);
        rejected.extend(batch_rejected);
    } else {
        for path in paths.iter() {
            if !is_valid_cache_path(path, strategy) {
                rejected.push(path.clone());
                continue;
            }
            if let PackageStrategy::Helm = strategy {
                if let Some(hosted) = helm_repository.as_ref() {
                    match delete_helm_package(site, hosted, path).await {
                        Ok(true) => {
                            deleted += 1;
                            deleted_paths.insert(path.clone());
                        }
                        Ok(false) => missing.push(path.clone()),
                        Err(err) => {
                            warn!(?err, path, "Failed to delete Helm chart package");
                            missing.push(path.clone());
                        }
                    }
                } else {
                    warn!(
                        path,
                        "Helm repository missing hosted instance during deletion"
                    );
                    missing.push(path.clone());
                }
                continue;
            }
            if matches!(
                strategy,
                PackageStrategy::GoHosted | PackageStrategy::GoProxy
            ) {
                match delete_go_package(&storage, repository.id(), path).await {
                    Ok(Some(result)) => {
                        deleted += result.removed;
                        if result.removed > 0 {
                            deleted_paths.insert(path.clone());
                        }
                        missing.extend(result.missing);
                        continue;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        warn!(?err, path, "Failed to delete Go package files");
                        missing.push(path.clone());
                        continue;
                    }
                }
            }

            let storage_path = nr_core::storage::StoragePath::from(path.as_str());
            match storage.delete_file(repository.id(), &storage_path).await {
                Ok(true) => {
                    deleted += 1;
                    deleted_paths.insert(path.clone());
                    if let Some(version_path) = derive_version_path(path, catalog_mode) {
                        catalog_targets.insert(version_path);
                    }
                    if let Some(proxy) = python_proxy.as_ref() {
                        proxy
                            .handle_external_eviction(&storage_path)
                            .await
                            .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
                    }
                    if let Some(proxy) = npm_proxy.as_ref() {
                        proxy
                            .handle_external_eviction(&storage_path)
                            .await
                            .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
                    }
                    if let Some(proxy) = php_proxy.as_ref() {
                        proxy
                            .handle_external_eviction(&storage_path)
                            .await
                            .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
                    }
                    if let Some(proxy) = go_proxy.as_ref() {
                        proxy
                            .handle_external_eviction(&storage_path)
                            .await
                            .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
                    }
                    if let Some(proxy) = maven_proxy.as_ref() {
                        proxy
                            .handle_external_eviction(&storage_path)
                            .await
                            .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
                    }
                }
                Ok(false) => missing.push(path.clone()),
                Err(err) => {
                    warn!(?err, path, "Failed to delete cached package");
                    missing.push(path.clone());
                }
            }
        }
    }

    if catalog_mode != CatalogDeletionMode::None && !catalog_targets.is_empty() {
        delete_version_records_by_path(&site.database, repository.id(), &catalog_targets)
            .await
            .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
    }
    if !deleted_paths.is_empty() {
        let mut paths: Vec<String> = deleted_paths.into_iter().collect();
        paths.sort();
        let _ = DBPackageFile::soft_delete_by_paths(&site.database, repository.id(), &paths).await;
        for path in paths {
            if let Some(snapshot) = delete_webhook_snapshots.remove(&path) {
                if let Err(err) = webhooks::enqueue_snapshot(site, snapshot).await {
                    warn!(error = %err, path, "Failed to enqueue package delete webhook");
                }
            }
        }
    }

    Ok(PackageDeleteResponse {
        deleted,
        missing,
        rejected,
    })
}

#[utoipa::path(
    delete,
    path = "/{repository_id}/packages",
    request_body = PackageDeleteRequest,
    params(
        ("repository_id" = Uuid, Path, description = "The Repository ID"),
    ),
    responses(
        (status = 200, description = "Deleted cached packages", body = PackageDeleteResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Missing permission"),
        (status = 404, description = "Repository not found"),
    )
)]
#[instrument(
    skip(site, auth, request),
    fields(repository_id = %repository_id, user = %auth.id, path_count = request.paths.len())
)]
pub async fn delete_cached_packages(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository_id): Path<Uuid>,
    Json(request): Json<PackageDeleteRequest>,
) -> Result<Response, InternalError> {
    if request.paths.is_empty() {
        return Ok(ResponseBuilder::bad_request().body("paths cannot be empty".to_string()));
    }

    let Some(repository) = site.get_repository(repository_id) else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };

    if !auth
        .has_action(RepositoryActions::Edit, repository.id(), site.as_ref())
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository.id()).into_response());
    }

    let response = delete_cached_package_paths(
        &site,
        repository,
        &request.paths,
        PackageWebhookActor::from_user(&auth),
    )
    .await?;
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
mod tests;
