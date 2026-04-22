use std::sync::Arc;
#[cfg(test)]
use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
};

use futures::future::BoxFuture;
#[cfg(test)]
use futures::{StreamExt, stream};

use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, FixedOffset};
use http::header::HeaderValue;
use nr_storage::Storage;
#[cfg(test)]
use nr_storage::{DynStorage, FileType, StorageFile, StorageFileMeta};
#[cfg(test)]
use nr_storage::{StorageError, s3::S3Storage};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json;
use sha2::{Digest, Sha256};
#[cfg(test)]
use sqlx::types::Json as SqlxJson;
use sqlx::{PgPool, Row};
use tokio::io::AsyncReadExt;
use tracing::{debug, instrument, warn};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[cfg(test)]
use crate::repository::docker::metadata::collect_manifest_entries;
#[cfg(test)]
use crate::repository::helm::HelmChartVersionExtra;
use crate::{
    app::{
        Pkgly,
        authentication::Authentication,
        responses::{MissingPermission, RepositoryNotFound},
    },
    error::{InternalError, OtherInternalError},
    repository::{
        DynRepository, Repository,
        docker::{
            DockerRegistry,
            metadata::{docker_package_key, split_manifest_cache_path},
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
#[cfg(test)]
use nr_core::repository::project::{CargoPackageMetadata, DebPackageMetadata, VersionData};
use nr_core::user::permissions::{HasPermissions, RepositoryActions};
#[cfg(test)]
use nr_core::utils::base64_utils;
use nr_core::{
    database::entities::package_file::{
        DBPackageFile, PackageFileListParams, PackageFileSortBy, SortDirection,
    },
    repository::project::ProxyArtifactKey,
    storage::StoragePath,
};
#[cfg(test)]
use std::cmp::min;
#[cfg(test)]
use std::collections::BTreeMap;

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

#[cfg(test)]
fn normalize_sha256_digest(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("sha256:") {
        return Some(trimmed.to_string());
    }
    Some(format!("sha256:{trimmed}"))
}

#[cfg(test)]
fn sha256_digest_from_base64(value: &str) -> Option<String> {
    let bytes = base64_utils::decode(value).ok()?;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    Some(format!("sha256:{hex}"))
}

#[cfg(test)]
fn blob_digest_from_file_type(file: &nr_storage::FileFileType) -> Option<String> {
    file.file_hash
        .sha2_256
        .as_deref()
        .and_then(sha256_digest_from_base64)
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
struct PackageObject {
    /// Repository-relative object key (no bucket or repository prefix)
    key: String,
    size: u64,
    modified: DateTime<FixedOffset>,
}

/// Collapse a flat list of S3 objects into the paginated package response used by the
/// admin packages table. Objects must already be scoped to the repository and returned in
/// lexicographic key order (S3 default). Hidden files (".nr-meta") should be filtered out
/// by callers before invoking this helper.
#[cfg(test)]
fn build_package_page_from_objects(
    objects: impl IntoIterator<Item = PackageObject>,
    base: Option<&str>,
    page: usize,
    per_page: usize,
    search: Option<&str>,
) -> PackageListResponse {
    let per_page = per_page.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let start = (current_page - 1) * per_page;
    let end = start + per_page;

    let base_prefix = base.unwrap_or("");
    let search_term = search.map(|term| term.to_lowercase());

    let mut total_packages = 0usize;
    let mut page_items: Vec<PackageFileEntry> = Vec::new();

    let mut current_package: Option<(String, Vec<PackageFileEntry>)> = None;

    let finalize_current = |pkg: &mut Option<(String, Vec<PackageFileEntry>)>,
                            total_packages: &mut usize,
                            page_items: &mut Vec<PackageFileEntry>| {
        if let Some((_, files)) = pkg.take() {
            let filtered: Vec<PackageFileEntry> = if let Some(term) = &search_term {
                files
                    .into_iter()
                    .filter(|entry| matches_search(entry, term))
                    .collect()
            } else {
                files
            };

            if filtered.is_empty() {
                return;
            }

            if *total_packages >= start && *total_packages < end {
                page_items.extend(filtered);
            }
            *total_packages += 1;
        }
    };

    for obj in objects.into_iter() {
        let key = obj.key.trim_start_matches('/').to_string();

        // Only consider objects within the requested base prefix (when provided).
        if !base_prefix.is_empty() && !key.starts_with(base_prefix) {
            continue;
        }

        let relative = if base_prefix.is_empty() {
            key.as_str()
        } else {
            key.strip_prefix(base_prefix).unwrap_or(key.as_str())
        };

        let (package_dir, file_name) = match relative.rsplit_once('/') {
            Some(split) => split,
            None => continue,
        };

        if package_dir.is_empty() || should_ignore(file_name) {
            continue;
        }

        let mut display_name = package_dir.trim_matches('/').to_string();

        if let Some(prefix) = base {
            if prefix == "go-proxy-cache/" {
                if let Some(stripped) = display_name.strip_suffix("/@v") {
                    display_name = stripped.to_string();
                }
            }
        }

        if let Some(stripped) = display_name.strip_suffix("/@v") {
            display_name = stripped.to_string();
        }

        if display_name.is_empty() {
            continue;
        }

        let is_new_package = match &current_package {
            Some((name, _)) => name != &display_name,
            None => true,
        };

        if is_new_package {
            finalize_current(&mut current_package, &mut total_packages, &mut page_items);
            current_package = Some((display_name.clone(), Vec::new()));
        }

        if let Some((_, files)) = current_package.as_mut() {
            files.push(PackageFileEntry {
                package: display_name,
                name: file_name.to_string(),
                cache_path: key,
                blob_digest: None,
                size: obj.size,
                modified: obj.modified,
            });
        }
    }

    finalize_current(&mut current_package, &mut total_packages, &mut page_items);

    PackageListResponse {
        page: current_page,
        per_page,
        total_packages,
        items: page_items,
    }
}

#[cfg(test)]
fn matches_search(entry: &PackageFileEntry, term: &str) -> bool {
    let needle = term.to_lowercase();
    let haystack = format!(
        "{} {} {} {}",
        entry.package.to_lowercase(),
        entry.name.to_lowercase(),
        entry.cache_path.to_lowercase(),
        entry
            .blob_digest
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
    );
    haystack.contains(&needle)
}

#[cfg(test)]
async fn collect_directory_package_page(
    storage: &DynStorage,
    repository_id: Uuid,
    base: Option<&str>,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<PackageListResponse, nr_storage::StorageError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let start = (current_page - 1) * per_page;
    let end = start + per_page;

    let mut walker = PackageDirectoryWalker::new(storage, repository_id, base);
    let mut total_packages = 0usize;
    let mut items = Vec::new();

    while let Some(visit) = walker.next().await? {
        let mut entries = build_package_entries_from_directory(
            &visit.entry.display_name,
            &visit.files,
            &visit.entry.directory_path,
        );

        if let Some(term) = search {
            entries.retain(|entry| matches_search(entry, term));
        }

        if entries.is_empty() {
            continue;
        }

        if total_packages >= start && total_packages < end {
            items.extend(entries);
        }
        total_packages += 1;
    }

    let per_page_response = if total_packages == 0 {
        per_page_raw
    } else {
        per_page
    };

    Ok(PackageListResponse {
        page: current_page,
        per_page: per_page_response,
        total_packages,
        items,
    })
}

#[cfg(test)]
async fn collect_go_package_page(
    storage: &DynStorage,
    repository_id: Uuid,
    base: &str,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<PackageListResponse, nr_storage::StorageError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let start = (current_page - 1) * per_page;
    let end = start + per_page;

    let mut walker = PackageDirectoryWalker::new(storage, repository_id, Some(base));
    let mut total_versions = 0usize;
    let mut items = Vec::new();
    let normalized_search = search.map(|term| term.to_lowercase());

    while let Some(visit) = walker.next().await? {
        let mut entries = build_go_entries_from_directory(
            &visit.entry.display_name,
            &visit.files,
            &visit.entry.directory_path,
        );

        if let Some(term) = &normalized_search {
            entries.retain(|entry| matches_search(entry, term));
        }

        for entry in entries.into_iter() {
            if total_versions >= start && total_versions < end {
                items.push(entry);
            }
            total_versions += 1;
        }
    }

    Ok(PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions,
        items,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageStrategy {
    #[allow(dead_code)]
    PackagesDirectory {
        base: Option<&'static str>,
    },
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

#[cfg_attr(test, mockall::automock)]
trait CatalogDeletionExecutor {
    fn delete_paths<'a>(
        &'a self,
        repository_id: Uuid,
        normalized_paths: Vec<String>,
    ) -> BoxFuture<'a, Result<u64, sqlx::Error>>;
}

struct SqlCatalogDeletionExecutor<'a> {
    database: &'a PgPool,
}

impl<'a> CatalogDeletionExecutor for SqlCatalogDeletionExecutor<'a> {
    fn delete_paths<'b>(
        &'b self,
        repository_id: Uuid,
        normalized_paths: Vec<String>,
    ) -> BoxFuture<'b, Result<u64, sqlx::Error>> {
        Box::pin(sql_delete_project_versions(
            self.database,
            repository_id,
            normalized_paths,
        ))
    }
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

async fn delete_version_records_by_path<E: CatalogDeletionExecutor + ?Sized>(
    executor: &E,
    repository_id: Uuid,
    version_paths: &HashSet<String>,
) -> Result<u64, sqlx::Error> {
    if version_paths.is_empty() {
        return Ok(0);
    }
    let mut normalized = Vec::with_capacity(version_paths.len());
    for path in version_paths {
        if let Some(value) = normalize_catalog_path(path) {
            normalized.push(value);
        }
    }
    if normalized.is_empty() {
        return Ok(0);
    }
    normalized.sort();
    normalized.dedup();

    executor.delete_paths(repository_id, normalized).await
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
    let items = rows
        .into_iter()
        .map(|row| PackageFileEntry {
            package: row.package,
            name: row.name,
            cache_path: row.path,
            blob_digest: row.content_digest.or(row.upstream_digest),
            size: row.size_bytes.max(0) as u64,
            modified: row.modified_at,
        })
        .collect();
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

#[cfg(test)]
fn should_ignore(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".nr-meta")
}

#[cfg(test)]
#[allow(dead_code)]
const MAX_STORAGE_CONCURRENCY: usize = 8;

#[cfg(test)]
async fn map_ordered_concurrent<T, R, E, Fut, F>(
    items: Vec<T>,
    concurrency: usize,
    f: F,
) -> Result<Vec<R>, E>
where
    T: Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Fut: std::future::Future<Output = Result<R, E>> + Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
{
    let max_in_flight = concurrency.max(1);
    let mut stream = stream::iter(items.into_iter().enumerate().map(|(idx, item)| {
        let fut = f(item);
        async move { (idx, fut.await) }
    }))
    .buffer_unordered(max_in_flight);

    let mut ordered = Vec::new();
    while let Some((idx, result)) = stream.next().await {
        ordered.push((idx, result?));
    }
    ordered.sort_by_key(|(idx, _)| *idx);
    Ok(ordered.into_iter().map(|(_, value)| value).collect())
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_directory_packages(
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    base: Option<&str>,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let storage = repository.get_storage();
    let response =
        collect_directory_package_page(&storage, repository.id(), base, page, per_page_raw, search)
            .await?;
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
fn file_name_from_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
#[allow(dead_code)]
async fn load_single_file_entry(
    storage: DynStorage,
    repository_id: Uuid,
    package: String,
    cache_path: String,
    updated_at: DateTime<FixedOffset>,
) -> Result<PackageFileEntry, InternalError> {
    let storage_path = nr_core::storage::StoragePath::from(cache_path.as_str());
    if let Some(StorageFile::File { meta, .. }) =
        storage.open_file(repository_id, &storage_path).await?
    {
        return Ok(PackageFileEntry {
            package,
            name: file_name_from_path(&cache_path),
            cache_path,
            blob_digest: blob_digest_from_file_type(&meta.file_type),
            size: meta.file_type.file_size,
            modified: meta.modified,
        });
    }

    Ok(PackageFileEntry {
        package,
        name: file_name_from_path(&cache_path),
        cache_path,
        blob_digest: None,
        size: 0,
        modified: updated_at,
    })
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_directory_packages_s3(
    repository_id: Uuid,
    storage: S3Storage,
    page: usize,
    per_page_raw: usize,
    base: Option<&str>,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let objects = storage
        .list_repository_objects(repository_id, base)
        .await
        .map_err(StorageError::from)?;

    let mut objects: Vec<PackageObject> = objects
        .into_iter()
        .map(|obj| PackageObject {
            key: obj.key,
            size: obj.size,
            modified: obj
                .last_modified
                .unwrap_or_else(|| chrono::Local::now().fixed_offset()),
        })
        .collect();

    // Ensure deterministic ordering independent of S3 pagination
    objects.sort_by(|a, b| a.key.cmp(&b.key));

    let response = build_package_page_from_objects(objects, base, page, per_page_raw, search);

    Ok(ResponseBuilder::ok().json(&response))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg(test)]
enum GoFileKind {
    Info,
    Mod,
    Zip,
}

#[cfg(test)]
impl GoFileKind {
    const fn priority(self) -> u8 {
        match self {
            GoFileKind::Info => 1,
            GoFileKind::Mod => 2,
            GoFileKind::Zip => 3,
        }
    }
}

#[cfg(test)]
fn parse_go_file_name(name: &str) -> Option<(String, GoFileKind)> {
    if name == "list" {
        return None;
    }
    if let Some(version) = name.strip_suffix(".info") {
        return Some((version.to_string(), GoFileKind::Info));
    }
    if let Some(version) = name.strip_suffix(".mod") {
        return Some((version.to_string(), GoFileKind::Mod));
    }
    if let Some(version) = name.strip_suffix(".zip") {
        return Some((version.to_string(), GoFileKind::Zip));
    }
    None
}

#[cfg(test)]
fn should_replace_go_file(current: GoFileKind, candidate: GoFileKind) -> bool {
    candidate.priority() > current.priority()
}

#[cfg(test)]
#[cfg(test)]
async fn collect_go_package_entries(
    storage: &nr_storage::DynStorage,
    repository_id: Uuid,
    base: &str,
) -> Result<Vec<PackageFileEntry>, nr_storage::StorageError> {
    let mut walker = PackageDirectoryWalker::new(storage, repository_id, Some(base));
    let mut entries = Vec::new();
    while let Some(visit) = walker.next().await? {
        entries.extend(build_go_entries_from_directory(
            &visit.entry.display_name,
            &visit.files,
            &visit.entry.directory_path,
        ));
    }
    entries.sort_by(|a, b| a.package.cmp(&b.package).then(a.name.cmp(&b.name)));
    Ok(entries)
}

struct GoDeletionResult {
    removed: usize,
    missing: Vec<String>,
}

#[derive(sqlx::FromRow)]
#[cfg(test)]
#[allow(dead_code)]
struct DebPackageRow {
    project_name: String,
    version: String,
    extra: SqlxJson<VersionData>,
    created_at: DateTime<FixedOffset>,
}

#[derive(sqlx::FromRow)]
#[cfg(test)]
struct HostedCatalogRow {
    project_key: String,
    version: String,
    version_path: String,
    version_data: SqlxJson<VersionData>,
    updated_at: DateTime<FixedOffset>,
}

#[derive(sqlx::FromRow)]
#[cfg(test)]
struct ProxyCatalogRow {
    project_key: String,
    version: String,
    cache_path: String,
    version_data: SqlxJson<VersionData>,
    updated_at: DateTime<FixedOffset>,
}

#[cfg(test)]
async fn fetch_maven_catalog_page(
    database: &PgPool,
    repository_id: Uuid,
    per_page: usize,
    offset: i64,
    search: Option<&str>,
) -> Result<Vec<HostedCatalogRow>, sqlx::Error> {
    if let Some(term) = search {
        let pattern = format!("%{}%", term.to_lowercase());
        return sqlx::query_as::<_, HostedCatalogRow>(
            r#"
            SELECT
                p.key AS project_key,
                pv.version AS version,
                pv.path AS version_path,
                pv.extra AS version_data,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(database)
        .await;
    }

    sqlx::query_as::<_, HostedCatalogRow>(
        r#"
        SELECT
            p.key AS project_key,
            pv.version AS version,
            pv.path AS version_path,
            pv.extra AS version_data,
            pv.updated_at
        FROM project_versions pv
        INNER JOIN projects p ON pv.project_id = p.id
        WHERE p.repository_id = $1
        ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(repository_id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(database)
    .await
}

#[cfg(test)]
#[allow(dead_code)]
async fn fetch_maven_proxy_catalog_page(
    database: &PgPool,
    repository_id: Uuid,
    per_page: usize,
    offset: i64,
    search: Option<&str>,
) -> Result<Vec<ProxyCatalogRow>, sqlx::Error> {
    if let Some(term) = search {
        let pattern = format!("%{}%", term.to_lowercase());
        return sqlx::query_as::<_, ProxyCatalogRow>(
            r#"
            SELECT
                p.key AS project_key,
                pv.version,
                pv.path AS cache_path,
                pv.extra AS version_data,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE pv.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(database)
        .await;
    }

    sqlx::query_as::<_, ProxyCatalogRow>(
        r#"
        SELECT
            p.key AS project_key,
            pv.version,
            pv.path AS cache_path,
            pv.extra AS version_data,
            pv.updated_at
        FROM project_versions pv
        INNER JOIN projects p ON pv.project_id = p.id
        WHERE pv.repository_id = $1
        ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(repository_id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(database)
    .await
}

#[cfg(test)]
async fn fetch_php_catalog_page(
    database: &PgPool,
    repository_id: Uuid,
    per_page: usize,
    offset: i64,
    search: Option<&str>,
) -> Result<Vec<HostedCatalogRow>, sqlx::Error> {
    // PHP uses the same project_versions schema as Maven; reuse the same shape.
    fetch_maven_catalog_page(database, repository_id, per_page, offset, search).await
}

#[cfg(test)]
#[allow(dead_code)]
async fn fetch_php_proxy_catalog_page(
    database: &PgPool,
    repository_id: Uuid,
    per_page: usize,
    offset: i64,
    search: Option<&str>,
) -> Result<Vec<HostedCatalogRow>, sqlx::Error> {
    if let Some(term) = search {
        let pattern = format!("%{}%", term.to_lowercase());
        return sqlx::query_as::<_, HostedCatalogRow>(
            r#"
            SELECT
                p.key AS project_key,
                pv.version AS version,
                pv.path AS version_path,
                pv.extra AS version_data,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (pv.extra->'extra'->>'size') IS NOT NULL
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(database)
        .await;
    }

    sqlx::query_as::<_, HostedCatalogRow>(
        r#"
        SELECT
            p.key AS project_key,
            pv.version AS version,
            pv.path AS version_path,
            pv.extra AS version_data,
            pv.updated_at
        FROM project_versions pv
        INNER JOIN projects p ON pv.project_id = p.id
        WHERE p.repository_id = $1
          AND (pv.extra->'extra'->>'size') IS NOT NULL
        ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(repository_id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(database)
    .await
}

#[cfg(test)]
async fn fetch_npm_proxy_catalog_page(
    database: &PgPool,
    repository_id: Uuid,
    per_page: usize,
    offset: i64,
    search: Option<&str>,
) -> Result<Vec<ProxyCatalogRow>, sqlx::Error> {
    fetch_proxy_catalog_page(database, repository_id, per_page, offset, search).await
}

#[cfg(test)]
async fn fetch_proxy_catalog_page(
    database: &PgPool,
    repository_id: Uuid,
    per_page: usize,
    offset: i64,
    search: Option<&str>,
) -> Result<Vec<ProxyCatalogRow>, sqlx::Error> {
    if let Some(term) = search {
        let pattern = format!("%{}%", term.to_lowercase());
        return sqlx::query_as::<_, ProxyCatalogRow>(
            r#"
            SELECT
                p.key AS project_key,
                pv.version,
                pv.path AS cache_path,
                pv.extra AS version_data,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2
              )
            ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(database)
        .await;
    }

    sqlx::query_as::<_, ProxyCatalogRow>(
        r#"
        SELECT
            p.key AS project_key,
            pv.version,
            pv.path AS cache_path,
            pv.extra AS version_data,
            pv.updated_at
        FROM project_versions pv
        INNER JOIN projects p ON pv.project_id = p.id
        WHERE p.repository_id = $1
        ORDER BY LOWER(p.key) COLLATE "C", LOWER(pv.version) COLLATE "C"
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(repository_id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(database)
    .await
}

#[cfg(test)]
#[allow(dead_code)]
fn deb_metadata(data: &VersionData) -> Option<DebPackageMetadata> {
    data.extra
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok())
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

#[derive(Debug, Clone)]
#[cfg(test)]
struct PackageDirEntry {
    display_name: String,
    storage_relative: String,
    directory_path: String,
}

#[cfg(test)]
struct PackageDirVisit {
    entry: PackageDirEntry,
    files: Vec<StorageFileMeta<FileType>>,
}

#[derive(Debug, Clone)]
#[cfg(test)]
struct DirNode {
    path: String,
    relative: String,
    sort_key: String,
}

#[cfg(test)]
impl DirNode {
    fn root(path: String, base: Option<&str>) -> Self {
        Self::new(path, String::new(), base)
    }

    fn new(path: String, relative: String, base: Option<&str>) -> Self {
        let sort_key = if !relative.is_empty() {
            relative.clone()
        } else if let Some(prefix) = base {
            path.trim_start_matches(prefix)
                .trim_matches('/')
                .to_string()
        } else {
            path.trim_matches('/').to_string()
        };
        Self {
            path,
            relative,
            sort_key,
        }
    }

    fn child(&self, name: &str, base: Option<&str>) -> Self {
        let mut path = if self.path.is_empty() {
            String::new()
        } else {
            self.path.clone()
        };
        path.push_str(name);
        if !path.ends_with('/') {
            path.push('/');
        }
        let relative = if self.relative.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", self.relative, name)
        };
        DirNode::new(path, relative, base)
    }
}

#[cfg(test)]
impl PartialEq for DirNode {
    fn eq(&self, other: &Self) -> bool {
        self.sort_key == other.sort_key && self.path == other.path
    }
}

#[cfg(test)]
impl Eq for DirNode {}

#[cfg(test)]
impl PartialOrd for DirNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
impl Ord for DirNode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key
            .cmp(&other.sort_key)
            .then(self.path.cmp(&other.path))
    }
}

#[cfg(test)]
struct PackageDirectoryWalker<'a> {
    storage: &'a DynStorage,
    repository_id: Uuid,
    base_prefix: Option<String>,
    pending: BinaryHeap<Reverse<DirNode>>,
}

#[cfg(test)]
impl<'a> PackageDirectoryWalker<'a> {
    fn new(storage: &'a DynStorage, repository_id: Uuid, base: Option<&str>) -> Self {
        let base_prefix = base.map(|value| value.to_string());
        let mut pending = BinaryHeap::new();
        pending.push(Reverse(DirNode::root(
            base_prefix.clone().unwrap_or_default(),
            base_prefix.as_deref(),
        )));
        Self {
            storage,
            repository_id,
            base_prefix,
            pending,
        }
    }

    async fn next(&mut self) -> Result<Option<PackageDirVisit>, nr_storage::StorageError> {
        while let Some(Reverse(node)) = self.pending.pop() {
            let storage_path = if node.path.is_empty() {
                nr_core::storage::StoragePath::default()
            } else {
                nr_core::storage::StoragePath::from(node.path.clone())
            };
            let Some(StorageFile::Directory { files, .. }) = self
                .storage
                .open_file(self.repository_id, &storage_path)
                .await?
            else {
                continue;
            };

            let mut has_files = false;
            let mut child_dirs = Vec::new();
            for entry in files.iter() {
                if should_ignore(entry.name()) {
                    continue;
                }
                match entry.file_type() {
                    FileType::File(_) => {
                        has_files = true;
                    }
                    FileType::Directory(_) => {
                        child_dirs.push(node.child(entry.name(), self.base_prefix.as_deref()));
                    }
                }
            }

            for child in child_dirs {
                self.pending.push(Reverse(child));
            }

            if has_files {
                if let Some((display_name, storage_relative)) =
                    compute_package_names(&node, self.base_prefix.as_deref())
                {
                    let entry = PackageDirEntry {
                        display_name,
                        storage_relative,
                        directory_path: node.path.clone(),
                    };
                    return Ok(Some(PackageDirVisit { entry, files }));
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
fn compute_package_names(node: &DirNode, base: Option<&str>) -> Option<(String, String)> {
    let storage_relative = if !node.relative.is_empty() {
        node.relative.trim_matches('/').to_string()
    } else if let Some(prefix) = base {
        node.path
            .trim_start_matches(prefix)
            .trim_matches('/')
            .to_string()
    } else {
        node.path.trim_matches('/').to_string()
    };

    if storage_relative.is_empty() {
        return None;
    }

    let mut display_name = storage_relative.clone();
    if matches!(base, Some(prefix) if prefix == "go-proxy-cache/") {
        if let Some(stripped) = display_name.strip_suffix("/@v") {
            display_name = stripped.to_string();
        }
    }
    if let Some(stripped) = display_name.strip_suffix("/@v") {
        display_name = stripped.to_string();
    }

    if display_name.is_empty() {
        return None;
    }

    Some((display_name, storage_relative))
}

#[cfg(test)]
fn build_package_entries_from_directory(
    display_name: &str,
    files: &[StorageFileMeta<FileType>],
    directory_path: &str,
) -> Vec<PackageFileEntry> {
    let directory_prefix = directory_path.trim_end_matches('/');
    let mut items = Vec::new();
    for meta in files.iter() {
        if should_ignore(meta.name()) {
            continue;
        }
        if let FileType::File(file_meta) = meta.file_type() {
            let cache_path = if directory_prefix.is_empty() {
                meta.name().to_string()
            } else {
                format!("{}/{}", directory_prefix, meta.name())
            };
            items.push(PackageFileEntry {
                package: display_name.to_string(),
                name: meta.name().to_string(),
                cache_path,
                blob_digest: blob_digest_from_file_type(file_meta),
                size: file_meta.file_size,
                modified: meta.modified().clone(),
            });
        }
    }
    items
}

#[cfg(test)]
fn build_go_entries_from_directory(
    display_name: &str,
    files: &[StorageFileMeta<FileType>],
    directory_path: &str,
) -> Vec<PackageFileEntry> {
    let mut versions: BTreeMap<String, (PackageFileEntry, GoFileKind)> = BTreeMap::new();

    for entry in files.iter() {
        if should_ignore(entry.name()) {
            continue;
        }
        if let FileType::File(file_meta) = entry.file_type() {
            if let Some((version, kind)) = parse_go_file_name(entry.name()) {
                let cache_path = format!("{}{}", directory_path, entry.name());
                let candidate = PackageFileEntry {
                    package: display_name.to_string(),
                    name: version.clone(),
                    cache_path,
                    blob_digest: blob_digest_from_file_type(file_meta),
                    size: file_meta.file_size,
                    modified: entry.modified().clone(),
                };
                match versions.get_mut(&version) {
                    Some((existing, existing_kind)) => {
                        if should_replace_go_file(*existing_kind, kind)
                            || (existing.cache_path.is_empty() && candidate.cache_path.is_empty())
                        {
                            *existing = candidate;
                            *existing_kind = kind;
                        } else if candidate.modified > existing.modified {
                            existing.modified = candidate.modified;
                            existing.size = candidate.size;
                        }
                    }
                    None => {
                        versions.insert(version, (candidate, kind));
                    }
                }
            }
        }
    }

    versions.into_values().map(|(entry, _)| entry).collect()
}

#[allow(dead_code)]
#[cfg(test)]
async fn list_go_packages(
    repository: DynRepository,
    base: &str,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let storage = repository.get_storage();
    let response =
        collect_go_package_page(&storage, repository.id(), base, page, per_page_raw, search)
            .await?;
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_helm_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository.id())
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository.id())
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    if offset >= total_versions {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    let rows = if let Some(pattern) = &search_pattern {
        sqlx::query(
            r#"
            SELECT
                p.name AS chart_name,
                pv.version,
                pv.path,
                pv.extra,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            ORDER BY p.name ASC, pv.version ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(repository.id())
        .bind(pattern)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&site.database)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT
                p.name AS chart_name,
                pv.version,
                pv.path,
                pv.extra,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            ORDER BY p.name ASC, pv.version ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(repository.id())
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&site.database)
        .await?
    };

    let mut items = Vec::with_capacity(rows.len());

    for row in rows {
        let chart_name: String = row.try_get("chart_name")?;
        let version: String = row.try_get("version")?;
        let path: String = row.try_get("path")?;
        let version_data: sqlx::types::Json<VersionData> = row.try_get("extra")?;
        let updated_at: DateTime<FixedOffset> = row.try_get("updated_at")?;

        let Some(extra_value) = version_data.0.extra else {
            debug!(
                chart = %chart_name,
                version = %version,
                "Skipping Helm version without extra metadata"
            );
            continue;
        };
        let chart_extra: HelmChartVersionExtra = serde_json::from_value(extra_value)?;
        let cache_path = if chart_extra.canonical_path.is_empty() {
            path.clone()
        } else {
            chart_extra.canonical_path.clone()
        };
        items.push(PackageFileEntry {
            package: chart_name,
            name: version,
            cache_path,
            blob_digest: Some(chart_extra.digest),
            size: chart_extra.size_bytes,
            modified: updated_at,
        });
    }

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_cargo_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository.id())
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository.id())
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    if offset >= total_versions {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    let rows = if let Some(pattern) = &search_pattern {
        sqlx::query(
            r#"
            SELECT
                p.name AS crate_name,
                p.key AS project_key,
                pv.version AS version,
                pv.extra AS extra,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            ORDER BY p.name ASC, pv.version ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(repository.id())
        .bind(pattern)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&site.database)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT
                p.name AS crate_name,
                p.key AS project_key,
                pv.version AS version,
                pv.extra AS extra,
                pv.updated_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            ORDER BY p.name ASC, pv.version ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(repository.id())
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&site.database)
        .await?
    };

    let mut items = Vec::with_capacity(rows.len());

    for row in rows {
        let crate_name: String = row.try_get("crate_name")?;
        let project_key: String = row.try_get("project_key")?;
        let version: String = row.try_get("version")?;
        let version_data: sqlx::types::Json<VersionData> = row.try_get("extra")?;
        let updated_at: DateTime<FixedOffset> = row.try_get("updated_at")?;

        let VersionData { extra, .. } = version_data.0;
        let Some(extra_value) = extra else {
            debug!(
                crate = %crate_name,
                version = %version,
                "Skipping Cargo version without metadata"
            );
            continue;
        };
        let metadata: CargoPackageMetadata = serde_json::from_value(extra_value)?;
        items.push(build_cargo_package_entry(
            &crate_name,
            &project_key,
            &version,
            updated_at,
            &metadata,
        ));
    }

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
fn build_cargo_package_entry(
    crate_name: &str,
    project_key: &str,
    version: &str,
    updated_at: DateTime<FixedOffset>,
    metadata: &CargoPackageMetadata,
) -> PackageFileEntry {
    PackageFileEntry {
        package: crate_name.to_string(),
        name: version.to_string(),
        cache_path: cargo_cache_path(project_key, version),
        blob_digest: normalize_sha256_digest(&metadata.checksum),
        size: metadata.crate_size,
        modified: updated_at,
    }
}

#[cfg(test)]
fn cargo_cache_path(project_key: &str, version: &str) -> String {
    format!(
        "crates/{key}/{ver}/{key}-{ver}.crate",
        key = project_key,
        ver = version
    )
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_npm_proxy_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE pv.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            WHERE pv.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows =
        fetch_npm_proxy_catalog_page(&site.database, repository_id, per_page, offset, search)
            .await?;

    let items: Vec<PackageFileEntry> = rows.iter().filter_map(proxy_entry_from_row).collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_npm_hosted_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows =
        fetch_maven_catalog_page(&site.database, repository_id, per_page, offset, search).await?;

    let storage = repository.get_storage();
    let items = map_ordered_concurrent(rows, MAX_STORAGE_CONCURRENCY, move |row| {
        let storage = storage.clone();
        async move {
            load_single_file_entry(
                storage,
                repository_id,
                row.project_key.clone(),
                row.version_path.clone(),
                row.updated_at,
            )
            .await
        }
    })
    .await?;

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_npm_virtual_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    list_npm_hosted_packages(site, repository, page, per_page_raw, search).await
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_python_hosted_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows =
        fetch_maven_catalog_page(&site.database, repository_id, per_page, offset, search).await?;

    let storage = repository.get_storage();
    let items = map_ordered_concurrent(rows, MAX_STORAGE_CONCURRENCY, move |row| {
        let storage = storage.clone();
        async move {
            load_single_file_entry(
                storage,
                repository_id,
                row.project_key.clone(),
                row.version_path.clone(),
                row.updated_at,
            )
            .await
        }
    })
    .await?;

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_python_proxy_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows =
        fetch_proxy_catalog_page(&site.database, repository_id, per_page, offset, search).await?;

    let items: Vec<PackageFileEntry> = rows.iter().filter_map(proxy_entry_from_row).collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_go_catalog_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows =
        fetch_maven_catalog_page(&site.database, repository_id, per_page, offset, search).await?;

    let storage = repository.get_storage();
    let items = map_ordered_concurrent(rows, MAX_STORAGE_CONCURRENCY, move |row| {
        let storage = storage.clone();
        async move {
            let mut entry = load_single_file_entry(
                storage,
                repository_id,
                row.project_key.clone(),
                row.version_path.clone(),
                row.updated_at,
            )
            .await?;
            entry.name = row.version.clone();
            Ok::<_, InternalError>(entry)
        }
    })
    .await?;

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
fn go_entry_from_proxy_row(row: &ProxyCatalogRow) -> Option<PackageFileEntry> {
    let (size, modified, blob_digest) = match row.version_data.0.proxy_artifact() {
        Some(proxy_meta) => (
            proxy_meta.size.unwrap_or_default(),
            DateTime::<FixedOffset>::from(proxy_meta.fetched_at),
            proxy_meta.upstream_digest.clone(),
        ),
        None => (0, row.updated_at, None),
    };
    Some(PackageFileEntry {
        package: row.project_key.clone(),
        name: row.version.clone(),
        cache_path: row.cache_path.clone(),
        blob_digest,
        size,
        modified,
    })
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_go_proxy_catalog_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows =
        fetch_proxy_catalog_page(&site.database, repository_id, per_page, offset, search).await?;
    let items = rows
        .iter()
        .filter_map(go_entry_from_proxy_row)
        .collect::<Vec<_>>();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
fn docker_entry_from_row(row: &ProxyCatalogRow) -> Option<PackageFileEntry> {
    let (package, size, modified, blob_digest) = match row.version_data.0.proxy_artifact() {
        Some(proxy_meta) => (
            proxy_meta.package_name,
            proxy_meta.size.unwrap_or_default(),
            DateTime::<FixedOffset>::from(proxy_meta.fetched_at),
            proxy_meta.upstream_digest.clone(),
        ),
        None => (
            row.project_key.clone(),
            0,
            row.updated_at,
            if row.version.starts_with("sha256:") {
                Some(row.version.clone())
            } else {
                None
            },
        ),
    };
    Some(PackageFileEntry {
        package,
        name: row.version.clone(),
        cache_path: row.cache_path.clone(),
        blob_digest,
        size,
        modified,
    })
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_docker_catalog_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows =
        fetch_proxy_catalog_page(&site.database, repository_id, per_page, offset, search).await?;

    let items: Vec<PackageFileEntry> = rows.iter().filter_map(docker_entry_from_row).collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_maven_hosted_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    if offset >= total_versions {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    let rows =
        fetch_maven_catalog_page(&site.database, repository_id, per_page, offset, search).await?;

    let storage = repository.get_storage();
    let version_chunks = map_ordered_concurrent(rows, MAX_STORAGE_CONCURRENCY, move |row| {
        let storage = storage.clone();
        async move { load_maven_version_entries(storage, repository_id, row).await }
    })
    .await?;

    let items: Vec<PackageFileEntry> = version_chunks.into_iter().flatten().collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_php_hosted_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    if offset >= total_versions {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    let rows =
        fetch_php_catalog_page(&site.database, repository_id, per_page, offset, search).await?;

    let storage = repository.get_storage();
    let version_chunks = map_ordered_concurrent(rows, MAX_STORAGE_CONCURRENCY, move |row| {
        let storage = storage.clone();
        async move { load_php_version_entries(storage, repository_id, row).await }
    })
    .await?;

    let items: Vec<PackageFileEntry> = version_chunks.into_iter().flatten().collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn load_maven_version_entries(
    storage: DynStorage,
    repository_id: Uuid,
    row: HostedCatalogRow,
) -> Result<Vec<PackageFileEntry>, InternalError> {
    let HostedCatalogRow {
        project_key,
        version,
        version_path,
        version_data,
        updated_at: _updated_at,
    } = row;
    let version_data = version_data.0;
    let package_label = format!("{}:{}", project_key, version);
    let cache_prefix = version_path.trim_end_matches('/');
    let normalized_path = ensure_trailing_slash(&version_path);
    let storage_path = nr_core::storage::StoragePath::from(normalized_path);

    if let Some(StorageFile::Directory { files, .. }) =
        storage.open_file(repository_id, &storage_path).await?
    {
        let mut file_entries: Vec<_> = files.iter().collect();
        file_entries.sort_by(|a, b| a.name().cmp(b.name()));

        let mut items = Vec::new();
        for meta in file_entries {
            if should_ignore(meta.name()) {
                continue;
            }
            if let FileType::File(file_meta) = meta.file_type() {
                let cache_path = if cache_prefix.is_empty() {
                    meta.name().to_string()
                } else {
                    format!("{cache_prefix}/{}", meta.name())
                };
                items.push(PackageFileEntry {
                    package: package_label.clone(),
                    name: meta.name().to_string(),
                    cache_path,
                    blob_digest: blob_digest_from_file_type(file_meta),
                    size: file_meta.file_size,
                    modified: meta.modified().clone(),
                });
            }
        }
        return Ok(items);
    }

    if let Some(proxy_meta) = version_data.proxy_artifact() {
        let modified: DateTime<FixedOffset> = proxy_meta.fetched_at.into();
        let file_name = proxy_meta
            .cache_path
            .rsplit('/')
            .next()
            .unwrap_or(&proxy_meta.cache_path)
            .to_string();
        return Ok(vec![PackageFileEntry {
            package: package_label,
            name: file_name,
            cache_path: proxy_meta.cache_path.clone(),
            blob_digest: proxy_meta.upstream_digest.clone(),
            size: proxy_meta.size.unwrap_or_default(),
            modified,
        }]);
    }

    let direct_path = nr_core::storage::StoragePath::from(version_path.as_str());
    if let Some(StorageFile::File { meta, .. }) =
        storage.open_file(repository_id, &direct_path).await?
    {
        let name = version_path
            .rsplit('/')
            .next()
            .unwrap_or(&version_path)
            .to_string();
        return Ok(vec![PackageFileEntry {
            package: package_label,
            name,
            cache_path: version_path,
            blob_digest: blob_digest_from_file_type(&meta.file_type),
            size: meta.file_type.file_size,
            modified: meta.modified,
        }]);
    }

    Ok(Vec::new())
}

#[cfg(test)]
#[allow(dead_code)]
async fn load_maven_proxy_version_entries(
    storage: DynStorage,
    repository_id: Uuid,
    row: ProxyCatalogRow,
) -> Result<Vec<PackageFileEntry>, InternalError> {
    let ProxyCatalogRow {
        project_key,
        version,
        cache_path,
        version_data,
        updated_at,
    } = row;
    let version_data = version_data.0;
    let package_label = format!("{}:{}", project_key, version);
    let cache_prefix = cache_path.trim_end_matches('/');
    let normalized_path = ensure_trailing_slash(&cache_path);
    let storage_path = nr_core::storage::StoragePath::from(normalized_path);

    if let Some(StorageFile::Directory { files, .. }) =
        storage.open_file(repository_id, &storage_path).await?
    {
        let mut file_entries: Vec<_> = files.iter().collect();
        file_entries.sort_by(|a, b| a.name().cmp(b.name()));

        let mut items = Vec::new();
        for meta in file_entries {
            if should_ignore(meta.name()) {
                continue;
            }
            if let FileType::File(file_meta) = meta.file_type() {
                let child_path = if cache_prefix.is_empty() {
                    meta.name().to_string()
                } else {
                    format!("{cache_prefix}/{}", meta.name())
                };
                items.push(PackageFileEntry {
                    package: package_label.clone(),
                    name: meta.name().to_string(),
                    cache_path: child_path,
                    blob_digest: blob_digest_from_file_type(file_meta),
                    size: file_meta.file_size,
                    modified: meta.modified().clone(),
                });
            }
        }
        return Ok(items);
    }

    if let Some(proxy_meta) = version_data.proxy_artifact() {
        let modified: DateTime<FixedOffset> = proxy_meta.fetched_at.into();
        return Ok(vec![PackageFileEntry {
            package: package_label,
            name: file_name_from_path(&proxy_meta.cache_path),
            cache_path: proxy_meta.cache_path,
            blob_digest: proxy_meta.upstream_digest,
            size: proxy_meta.size.unwrap_or_default(),
            modified,
        }]);
    }

    let direct_path = nr_core::storage::StoragePath::from(cache_path.as_str());
    if let Some(StorageFile::File { meta, .. }) =
        storage.open_file(repository_id, &direct_path).await?
    {
        return Ok(vec![PackageFileEntry {
            package: package_label,
            name: file_name_from_path(&cache_path),
            cache_path,
            blob_digest: blob_digest_from_file_type(&meta.file_type),
            size: meta.file_type.file_size,
            modified: meta.modified,
        }]);
    }

    let _ = updated_at;
    Ok(Vec::new())
}

#[cfg(test)]
async fn load_php_version_entries(
    storage: DynStorage,
    repository_id: Uuid,
    row: HostedCatalogRow,
) -> Result<Vec<PackageFileEntry>, InternalError> {
    let HostedCatalogRow {
        project_key,
        version,
        version_path,
        version_data,
        updated_at,
    } = row;

    if let Some(proxy_meta) = version_data.0.proxy_artifact() {
        // PHP proxy: rely on catalog metadata and only surface entries
        // for versions that have a recorded cache size, which is set
        // when the dist is actually cached.
        if let Some(size) = proxy_meta.size {
            let modified: DateTime<FixedOffset> = proxy_meta.fetched_at.into();
            let label = proxy_meta.version.clone().unwrap_or_else(|| {
                proxy_meta
                    .cache_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&proxy_meta.cache_path)
                    .to_string()
            });
            return Ok(vec![PackageFileEntry {
                package: proxy_meta.package_key.clone(),
                name: label,
                cache_path: proxy_meta.cache_path.clone(),
                blob_digest: proxy_meta.upstream_digest.clone(),
                size,
                modified,
            }]);
        }
        // Metadata-only proxy rows (no cached dist) are hidden from the list.
        return Ok(Vec::new());
    }

    // PHP hosted: list only versions that have a dist file in storage.
    let storage_path = nr_core::storage::StoragePath::from(version_path.as_str());
    if let Some(StorageFile::File { meta, .. }) =
        storage.open_file(repository_id, &storage_path).await?
    {
        return Ok(vec![PackageFileEntry {
            package: project_key,
            name: version,
            cache_path: version_path,
            blob_digest: blob_digest_from_file_type(&meta.file_type),
            size: meta.file_type.file_size,
            modified: meta.modified,
        }]);
    }

    let modified: DateTime<FixedOffset> = updated_at;
    Ok(vec![PackageFileEntry {
        package: project_key,
        name: version,
        cache_path: version_path,
        blob_digest: None,
        size: 0,
        modified,
    }])
}

#[cfg(test)]
fn proxy_entry_from_row(row: &ProxyCatalogRow) -> Option<PackageFileEntry> {
    let (cache_path, size, modified, blob_digest) = match row.version_data.0.proxy_artifact() {
        Some(proxy_meta) => (
            proxy_meta.cache_path,
            proxy_meta.size.unwrap_or_default(),
            DateTime::<FixedOffset>::from(proxy_meta.fetched_at),
            proxy_meta.upstream_digest,
        ),
        None => (row.cache_path.clone(), 0, row.updated_at, None),
    };

    Some(PackageFileEntry {
        package: row.project_key.clone(),
        name: file_name_from_path(&cache_path),
        cache_path,
        blob_digest,
        size,
        modified,
    })
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_deb_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2 OR
                LOWER(COALESCE(pv.extra::text, '')) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository.id())
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository.id())
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if offset >= total_versions {
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    let rows = if let Some(pattern) = &search_pattern {
        sqlx::query_as::<_, DebPackageRow>(
            r#"
            SELECT
                p.name AS project_name,
                pv.version,
                pv.extra,
                pv.created_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2 OR
                LOWER(COALESCE(pv.extra::text, '')) COLLATE "C" LIKE $2
              )
            ORDER BY pv.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(repository.id())
        .bind(pattern)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&site.database)
        .await?
    } else {
        sqlx::query_as::<_, DebPackageRow>(
            r#"
            SELECT
                p.name AS project_name,
                pv.version,
                pv.extra,
                pv.created_at
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
            ORDER BY pv.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(repository.id())
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&site.database)
        .await?
    };

    let mut items = Vec::new();
    for row in rows {
        if let Some(metadata) = deb_metadata(&row.extra.0) {
            items.push(PackageFileEntry {
                package: row.project_name.clone(),
                name: row.version.clone(),
                cache_path: metadata.filename.clone(),
                blob_digest: normalize_sha256_digest(&metadata.sha256),
                size: metadata.size,
                modified: row.created_at,
            });
        }
    }

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_php_proxy_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (pv.extra->'extra'->>'size') IS NOT NULL
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE p.repository_id = $1
              AND (pv.extra->'extra'->>'size') IS NOT NULL
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    if offset >= total_versions {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    let rows =
        fetch_php_proxy_catalog_page(&site.database, repository_id, per_page, offset, search)
            .await?;

    let storage = repository.get_storage();
    let version_chunks = map_ordered_concurrent(rows, MAX_STORAGE_CONCURRENCY, move |row| {
        let storage = storage.clone();
        async move { load_php_version_entries(storage, repository_id, row).await }
    })
    .await?;

    let items: Vec<PackageFileEntry> = version_chunks.into_iter().flatten().collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
#[allow(dead_code)]
async fn list_maven_proxy_packages(
    site: Pkgly,
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let offset = ((current_page - 1) * per_page) as i64;
    let repository_id = repository.id();
    let search_pattern = search.map(|term| format!("%{}%", term.to_lowercase()));

    let total_versions: i64 = if let Some(pattern) = &search_pattern {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            INNER JOIN projects p ON pv.project_id = p.id
            WHERE pv.repository_id = $1
              AND (
                LOWER(p.name) COLLATE "C" LIKE $2 OR
                LOWER(p.key) COLLATE "C" LIKE $2 OR
                LOWER(pv.version) COLLATE "C" LIKE $2 OR
                LOWER(pv.path) COLLATE "C" LIKE $2
              )
            "#,
        )
        .bind(repository_id)
        .bind(pattern)
        .fetch_one(&site.database)
        .await?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM project_versions pv
            WHERE pv.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_one(&site.database)
        .await?
    };

    if total_versions == 0 {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    if offset >= total_versions {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages: total_versions as usize,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    let rows =
        fetch_maven_proxy_catalog_page(&site.database, repository_id, per_page, offset, search)
            .await?;
    let storage = repository.get_storage();
    let version_chunks = map_ordered_concurrent(rows, MAX_STORAGE_CONCURRENCY, move |row| {
        let storage = storage.clone();
        async move { load_maven_proxy_version_entries(storage, repository_id, row).await }
    })
    .await?;

    let items: Vec<PackageFileEntry> = version_chunks.into_iter().flatten().collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages: total_versions as usize,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[allow(dead_code)]
#[cfg(test)]
async fn build_maven_proxy_package_list(
    storage: &DynStorage,
    repository_id: Uuid,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<PackageListResponse, InternalError> {
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let search_term = search.map(|value| value.to_lowercase());

    let mut directories = gather_package_dirs(storage, repository_id, None).await?;
    directories.sort_by(|a, b| a.0.cmp(&b.0));

    let mut package_entries: Vec<Vec<PackageFileEntry>> = Vec::new();

    for (_, relative) in directories.into_iter() {
        let Some(package_label) = derive_maven_package_label(&relative) else {
            continue;
        };
        let storage_path = nr_core::storage::StoragePath::from(ensure_trailing_slash(&relative));
        let Some(StorageFile::Directory { files, .. }) =
            storage.open_file(repository_id, &storage_path).await?
        else {
            continue;
        };
        let has_pom = files.iter().any(|entry| {
            matches!(entry.file_type(), FileType::File(_)) && entry.name().ends_with(".pom")
        });
        if !has_pom {
            continue;
        }

        let cache_prefix = relative.trim_matches('/');
        let mut file_entries: Vec<_> = files.iter().collect();
        file_entries.sort_by(|a, b| a.name().cmp(b.name()));

        let mut package_files = Vec::new();
        for entry in file_entries {
            if should_ignore(entry.name()) {
                continue;
            }
            if let FileType::File(file_meta) = entry.file_type() {
                let cache_path = if cache_prefix.is_empty() {
                    entry.name().to_string()
                } else {
                    format!("{cache_prefix}/{}", entry.name())
                };
                let file_entry = PackageFileEntry {
                    package: package_label.clone(),
                    name: entry.name().to_string(),
                    cache_path,
                    blob_digest: blob_digest_from_file_type(file_meta),
                    size: file_meta.file_size,
                    modified: entry.modified().clone(),
                };
                if let Some(term) = &search_term {
                    if !matches_search(&file_entry, term) {
                        continue;
                    }
                }
                package_files.push(file_entry);
            }
        }

        if package_files.is_empty() {
            continue;
        }
        package_entries.push(package_files);
    }

    let total_packages = package_entries.len();
    if total_packages == 0 {
        return Ok(PackageListResponse {
            page: current_page,
            per_page,
            total_packages: 0,
            items: Vec::new(),
        });
    }

    let start = (current_page - 1) * per_page;
    if start >= total_packages {
        return Ok(PackageListResponse {
            page: current_page,
            per_page,
            total_packages,
            items: Vec::new(),
        });
    }

    let end = min(start + per_page, total_packages);
    let mut items = Vec::new();
    for package in package_entries[start..end].iter() {
        items.extend(package.clone());
    }

    Ok(PackageListResponse {
        page: current_page,
        per_page,
        total_packages,
        items,
    })
}

#[cfg(test)]
fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

#[cfg(test)]
fn derive_maven_package_label(path: &str) -> Option<String> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let mut segments: Vec<&str> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() < 2 {
        return None;
    }
    let version = segments.pop()?.to_string();
    let artifact = segments.pop()?.to_string();
    let group = if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    };
    let project_key = if let Some(group) = group {
        format!("{group}:{artifact}")
    } else {
        artifact
    };
    Some(format!("{project_key}:{version}"))
}

#[allow(dead_code)]
#[cfg(test)]
async fn list_docker_packages(
    repository: DynRepository,
    page: usize,
    per_page_raw: usize,
    search: Option<&str>,
) -> Result<Response, InternalError> {
    let storage = repository.get_storage();
    let per_page = per_page_raw.clamp(1, MAX_PER_PAGE);
    let current_page = page.max(1);
    let start = (current_page - 1) * per_page;
    let search_term = search.map(|value| value.to_lowercase());

    if search_term.is_some() {
        let mut manifests = collect_manifest_entries(&storage, repository.id())
            .await
            .map_err(InternalError::from)?;

        manifests.sort_by(|a, b| {
            a.repository
                .cmp(&b.repository)
                .then(a.reference.cmp(&b.reference))
        });

        let mut entries: Vec<PackageFileEntry> = manifests
            .into_iter()
            .filter_map(|entry| {
                let pkg = PackageFileEntry {
                    package: entry.repository.clone(),
                    name: entry.reference.clone(),
                    cache_path: entry.cache_path.clone(),
                    blob_digest: if entry.reference.starts_with("sha256:") {
                        Some(entry.reference.clone())
                    } else {
                        None
                    },
                    size: entry.size,
                    modified: entry.modified,
                };
                if let Some(term) = &search_term {
                    if !matches_search(&pkg, term) {
                        return None;
                    }
                }
                Some(pkg)
            })
            .collect();

        let total_packages = entries.len();
        if total_packages == 0 || start >= total_packages {
            let empty = PackageListResponse {
                page: current_page,
                per_page,
                total_packages,
                items: Vec::new(),
            };
            return Ok(ResponseBuilder::ok().json(&empty));
        }

        let end = min(start + per_page, total_packages);
        let items = entries.drain(start..end).collect();
        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages,
            items,
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    if let DynStorage::S3(s3_storage) = storage.clone() {
        let (manifests, total_packages) = s3_storage
            .list_docker_manifests_paginated(repository.id(), start, per_page)
            .await
            .map_err(StorageError::from)?;

        if total_packages == 0 || start >= total_packages {
            let empty = PackageListResponse {
                page: current_page,
                per_page,
                total_packages,
                items: Vec::new(),
            };
            return Ok(ResponseBuilder::ok().json(&empty));
        }

        let now = chrono::Local::now().fixed_offset();
        let items = manifests
            .into_iter()
            .filter_map(|obj| {
                let repo_relative = obj.key.strip_prefix("v2/")?.to_string();
                let (repository, reference) = repo_relative.split_once("/manifests/")?;
                Some(PackageFileEntry {
                    package: repository.to_string(),
                    name: reference.to_string(),
                    cache_path: obj.key,
                    blob_digest: if reference.starts_with("sha256:") {
                        Some(reference.to_string())
                    } else {
                        None
                    },
                    size: obj.size,
                    modified: obj.last_modified.unwrap_or(now),
                })
            })
            .collect();

        let response = PackageListResponse {
            page: current_page,
            per_page,
            total_packages,
            items,
        };
        return Ok(ResponseBuilder::ok().json(&response));
    }

    // Fallback for non-S3 storage: load manifests into memory (local FS)
    let mut manifests = collect_manifest_entries(&storage, repository.id())
        .await
        .map_err(InternalError::from)?;

    manifests.sort_by(|a, b| {
        a.repository
            .cmp(&b.repository)
            .then(a.reference.cmp(&b.reference))
    });

    let total_packages = manifests.len();

    if total_packages == 0 || start >= total_packages {
        let empty = PackageListResponse {
            page: current_page,
            per_page,
            total_packages,
            items: Vec::new(),
        };
        return Ok(ResponseBuilder::ok().json(&empty));
    }

    let end = min(start + per_page, total_packages);
    let items = manifests[start..end]
        .iter()
        .map(|entry| PackageFileEntry {
            package: entry.repository.clone(),
            name: entry.reference.clone(),
            cache_path: entry.cache_path.clone(),
            blob_digest: if entry.reference.starts_with("sha256:") {
                Some(entry.reference.clone())
            } else {
                None
            },
            size: entry.size,
            modified: entry.modified,
        })
        .collect();

    let response = PackageListResponse {
        page: current_page,
        per_page,
        total_packages,
        items,
    };
    Ok(ResponseBuilder::ok().json(&response))
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
}

fn docker_proxy_key_from_path(path: &str) -> Option<ProxyArtifactKey> {
    let (repository, reference) = split_manifest_cache_path(path)?;
    Some(ProxyArtifactKey {
        package_key: docker_package_key(&repository),
        version: Some(reference),
        cache_path: Some(path.to_string()),
    })
}

pub async fn delete_docker_package(
    storage: &nr_storage::DynStorage,
    repository_id: Uuid,
    cache_path: &str,
    indexer: Option<&dyn ProxyIndexing>,
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
) -> Result<DockerBatchDeletion, DockerDeletionError> {
    let mut batch = StreamingDockerBatchDeletion::new(storage, repository_id, indexer);

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
        .delete_chart_versions(std::slice::from_ref(&entry))
        .await?;
    Ok(removed > 0)
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

    if matches!(
        strategy,
        PackageStrategy::DockerHosted | PackageStrategy::DockerProxy
    ) {
        let docker_indexer = docker_proxy.as_ref().map(|proxy| proxy.indexer().clone());
        let batch = collect_docker_deletions_batch(
            &storage,
            repository.id(),
            &request.paths,
            docker_indexer,
        )
        .await
        .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;

        debug!(
            paths = request.paths.len(),
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
            for path in request.paths.iter() {
                deleted_paths.insert(path.clone());
            }
        } else {
            missing.extend(request.paths.clone());
        }

        missing.extend(batch_missing);
        rejected.extend(batch_rejected);
    } else {
        for path in request.paths.iter() {
            if !is_valid_cache_path(path, strategy) {
                rejected.push(path.clone());
                continue;
            }
            if let PackageStrategy::Helm = strategy {
                if let Some(hosted) = helm_repository.as_ref() {
                    match delete_helm_package(&site, hosted, path).await {
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
        let executor = SqlCatalogDeletionExecutor {
            database: &site.database,
        };
        delete_version_records_by_path(&executor, repository.id(), &catalog_targets)
            .await
            .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
    }
    if !deleted_paths.is_empty() {
        let mut paths: Vec<String> = deleted_paths.into_iter().collect();
        paths.sort();
        let _ = DBPackageFile::soft_delete_by_paths(&site.database, repository.id(), &paths).await;
    }

    let response = PackageDeleteResponse {
        deleted,
        missing,
        rejected,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[cfg(test)]
async fn gather_package_dirs(
    storage: &nr_storage::DynStorage,
    repository_id: Uuid,
    base: Option<&str>,
) -> Result<Vec<(String, String)>, nr_storage::StorageError> {
    let mut walker = PackageDirectoryWalker::new(storage, repository_id, base);
    let mut packages = Vec::new();
    while let Some(visit) = walker.next().await? {
        packages.push((
            visit.entry.display_name.clone(),
            visit.entry.storage_relative.clone(),
        ));
    }
    Ok(packages)
}

#[cfg(test)]
mod tests;
