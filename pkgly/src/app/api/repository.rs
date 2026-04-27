use std::{collections::VecDeque, convert::TryFrom};

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use management::NewRepositoryRequest;
use nr_core::{
    database::entities::repository::{
        DBRepository, DBRepositoryConfig, DBRepositoryNames, DBRepositoryNamesWithVisibility,
        DBRepositoryWithStorageName,
    },
    repository::{
        RepositoryName, Visibility,
        browse::{BrowseFile, BrowseResponse},
        config::RepositoryConfigType,
        project::ProjectResolution,
    },
    storage::{StorageName, StoragePath},
    user::permissions::{HasPermissions, RepositoryActions},
};
use nr_storage::{FileType, Storage, StorageFile};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{instrument, warn};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

use crate::{
    app::{
        Pkgly, RepositoryStorageName,
        authentication::Authentication,
        responses::{MissingPermission, RepositoryNotFound},
    },
    error::InternalError,
    repository::{DynRepository, Repository, RepositoryTypeDescription},
    utils::ResponseBuilder,
};
mod browse;
mod config;
mod management;
pub mod packages;
mod r#virtual;
use self::r#virtual as virtual_api;
mod types;
#[derive(OpenApi)]
#[openapi(
    paths(
        list_repositories,
        get_repository,
        get_repository_names,
        find_repository_id,
        types::repository_types,
        config::config_schema,
        config::config_validate,
        config::config_default,
        config::config_description,
        management::new_repository,
        management::get_config,
        management::update_config,
        management::get_configs_for_repository,
        management::deb_refresh,
        management::deb_refresh_status,
        management::delete_repository,
        browse::browse,
        packages::list_cached_packages,
        packages::delete_cached_packages,
        virtual_api::list_members,
        virtual_api::update_members,
        virtual_api::update_resolution_order,
    ),
    components(schemas(
        DBRepository,
        DBRepositoryWithStorageName,
        RepositoryTypeDescription,
        NewRepositoryRequest,
        BrowseFile,
        BrowseResponse,
        ProjectResolution,
        DBRepositoryNames,
        DBRepositoryNamesWithVisibility,
        RepositoryListEntry,
        packages::PackageDeleteRequest,
        packages::PackageDeleteResponse,
        crate::repository::deb::proxy_refresh::DebProxyRefreshSummary,
        crate::repository::retention::config::PackageRetentionConfig,
        virtual_api::VirtualConfigView,
        virtual_api::VirtualMemberView,
        virtual_api::UpdateMembersRequest,
        virtual_api::UpdateResolutionOrderRequest
    ))
)]
pub struct RepositoryAPI;
pub fn repository_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/list", get(list_repositories))
        .route(
            "/find-id/{storage_name}/{repository_name}",
            get(find_repository_id),
        )
        .route("/{repository_id}", get(get_repository))
        .route("/{repository_id}/names", get(get_repository_names))
        .route("/types", get(types::repository_types))
        .merge(browse::browse_routes())
        .merge(packages::package_routes())
        .merge(management::management_routes())
        .merge(config::config_routes())
        .merge(virtual_api::virtual_routes())
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryListEntry {
    pub id: Uuid,
    pub storage_id: Uuid,
    pub storage_name: StorageName,
    pub name: RepositoryName,
    pub repository_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_kind: Option<String>,
    pub visibility: Visibility,
    pub active: bool,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub auth_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_usage_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_usage_updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryIdResponse {
    pub repository_id: Uuid,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RepositoryUsageQuery {
    /// Include cached storage usage metadata in the response
    #[serde(default)]
    #[param(default = false)]
    pub include_usage: bool,
    /// Force recalculation of repository storage usage (admin only)
    #[serde(default)]
    #[param(default = false)]
    pub refresh_usage: bool,
}

#[utoipa::path(
    get,
    summary = "Find the Repository Id by the storage and repository name",
    path = "/find-id/{storage_name}/{repository_name}",
    params(
        RepositoryStorageName
    ),
    responses(
        (status = 200, description = "Repository Id", body = RepositoryIdResponse),
        (status = 403, description = "Missing permission"),
        (status = 404, description = "Repository not found"),
    )
)]
pub async fn find_repository_id(
    State(site): State<Pkgly>,
    auth: Option<Authentication>,

    Path(names): Path<RepositoryStorageName>,
) -> Result<Response, InternalError> {
    let Some(repository) = site.get_repository_from_names(&names).await? else {
        return Ok(RepositoryNotFound::RepositoryAndNameLookup(names).into_response());
    };
    if repository.visibility().is_private()
        && !auth
            .has_action(RepositoryActions::Read, repository.id(), site.as_ref())
            .await?
    {
        return Ok(MissingPermission::ReadRepository(repository.id()).into_response());
    }
    let auth_config = site.get_repository_auth_config(repository.id()).await?;
    if auth_config.enabled
        && !auth
            .has_action(RepositoryActions::Read, repository.id(), site.as_ref())
            .await?
    {
        return Ok(MissingPermission::ReadRepository(repository.id()).into_response());
    }

    Ok(ResponseBuilder::ok().json(&RepositoryIdResponse {
        repository_id: repository.id(),
    }))
}
#[utoipa::path(
    get,
    path = "/{repository_id}",
    params(
        ("repository_id" = Uuid,Path, description = "The Repository ID"),
        RepositoryUsageQuery
    ),
    responses(
        (status = 200, description = "Repository Types", body = DBRepositoryWithStorageName),
    )
)]
#[instrument(skip(site, auth, query), fields(repository_id = %repository))]
pub async fn get_repository(
    State(site): State<Pkgly>,
    auth: Option<Authentication>,
    Path(repository): Path<Uuid>,
    Query(query): Query<RepositoryUsageQuery>,
) -> Result<Response, InternalError> {
    let Some(config) = DBRepositoryWithStorageName::get_by_id(repository, site.as_ref()).await?
    else {
        return Ok(RepositoryNotFound::Uuid(repository).into_response());
    };
    if config.visibility.is_private()
        && !auth
            .has_action(RepositoryActions::Read, repository, site.as_ref())
            .await?
    {
        return Ok(MissingPermission::ReadRepository(repository).into_response());
    }
    let auth_config = site.get_repository_auth_config(config.id).await?;
    if auth_config.enabled
        && !auth
            .has_action(RepositoryActions::Read, config.id, site.as_ref())
            .await?
    {
        return Ok(MissingPermission::ReadRepository(config.id).into_response());
    }
    let include_usage = query.refresh_usage || query.include_usage;
    let mut storage_usage = normalize_cached_usage(config.storage_usage_bytes);
    let mut storage_usage_updated_at = config.storage_usage_updated_at;
    if include_usage && (query.refresh_usage || storage_usage.is_none()) {
        match refresh_repository_storage_usage(&site, config.id).await {
            Ok(Some((usage, updated_at))) => {
                storage_usage = Some(usage);
                storage_usage_updated_at = Some(updated_at);
            }
            Ok(None) => {}
            Err(err) => {
                warn!(repository = %config.id, %err, "Failed to refresh repository storage usage");
            }
        }
    }
    let repository_kind = resolve_repository_kind(&config, &site).await?;

    let response = RepositoryListEntry {
        id: config.id,
        storage_id: config.storage_id,
        storage_name: config.storage_name,
        name: config.name,
        repository_type: config.repository_type.clone(),
        visibility: config.visibility,
        active: config.active,
        updated_at: config.updated_at,
        created_at: config.created_at,
        auth_enabled: auth_config.enabled,
        repository_kind,
        storage_usage_bytes: storage_usage,
        storage_usage_updated_at,
    };
    Ok(ResponseBuilder::ok().json(&response))
}

#[utoipa::path(
    get,
    path = "/list",
    params(RepositoryUsageQuery),
    responses(
        (status = 200, description = "List Repositories", body = [RepositoryListEntry]),
    )
)]
#[instrument(skip(auth, site, query))]
pub async fn list_repositories(
    auth: Option<Authentication>,
    State(site): State<Pkgly>,
    Query(query): Query<RepositoryUsageQuery>,
) -> Result<Response, InternalError> {
    let repositories = DBRepositoryWithStorageName::get_all(site.as_ref()).await?;
    let mut entries = Vec::with_capacity(repositories.len());
    for repository in repositories {
        if matches!(
            repository.visibility,
            Visibility::Private | Visibility::Hidden
        ) && !auth
            .has_action(RepositoryActions::Read, repository.id, site.as_ref())
            .await?
        {
            continue;
        }
        let auth_config = site.get_repository_auth_config(repository.id).await?;
        if auth_config.enabled
            && !auth
                .has_action(RepositoryActions::Read, repository.id, site.as_ref())
                .await?
        {
            continue;
        }
        let include_usage = query.refresh_usage || query.include_usage;
        let mut storage_usage_bytes = normalize_cached_usage(repository.storage_usage_bytes);
        let mut storage_usage_updated_at = repository.storage_usage_updated_at;
        if include_usage && (query.refresh_usage || storage_usage_bytes.is_none()) {
            match refresh_repository_storage_usage(&site, repository.id).await {
                Ok(Some((usage, updated_at))) => {
                    storage_usage_bytes = Some(usage);
                    storage_usage_updated_at = Some(updated_at);
                }
                Ok(None) => {}
                Err(err) => {
                    warn!(repository = %repository.id, %err, "Failed to refresh repository storage usage");
                }
            }
        }
        entries.push(RepositoryListEntry {
            id: repository.id,
            storage_id: repository.storage_id,
            storage_name: repository.storage_name.clone(),
            name: repository.name.clone(),
            repository_type: repository.repository_type.clone(),
            repository_kind: resolve_repository_kind(&repository, &site).await?,
            visibility: repository.visibility,
            active: repository.active,
            updated_at: repository.updated_at,
            created_at: repository.created_at,
            auth_enabled: auth_config.enabled,
            storage_usage_bytes,
            storage_usage_updated_at,
        });
    }
    Ok(ResponseBuilder::ok().json(&entries))
}

async fn resolve_repository_kind(
    repository: &DBRepositoryWithStorageName,
    site: &Pkgly,
) -> Result<Option<String>, InternalError> {
    let repo_type = repository.repository_type.as_str();
    if repo_type.eq_ignore_ascii_case(crate::repository::docker::REPOSITORY_TYPE_ID) {
        load_proxy_kind::<crate::repository::docker::DockerRegistryConfig>(
            repository,
            site,
            crate::repository::docker::DockerRegistryConfigType::get_type_static(),
        )
        .await
    } else if repo_type.eq_ignore_ascii_case(crate::repository::maven::REPOSITORY_TYPE_ID) {
        load_proxy_kind::<crate::repository::maven::MavenRepositoryConfig>(
            repository,
            site,
            crate::repository::maven::MavenRepositoryConfigType::get_type_static(),
        )
        .await
    } else if repo_type.eq_ignore_ascii_case("python") {
        load_proxy_kind::<crate::repository::python::PythonRepositoryConfig>(
            repository,
            site,
            crate::repository::python::PythonRepositoryConfigType::get_type_static(),
        )
        .await
    } else if repo_type.eq_ignore_ascii_case("npm") {
        load_proxy_kind::<crate::repository::npm::NPMRegistryConfig>(
            repository,
            site,
            crate::repository::npm::NPMRegistryConfigType::get_type_static(),
        )
        .await
    } else if repo_type.eq_ignore_ascii_case("go") {
        load_proxy_kind::<crate::repository::go::GoRepositoryConfig>(
            repository,
            site,
            crate::repository::go::GoRepositoryConfigType::get_type_static(),
        )
        .await
    } else if repo_type.eq_ignore_ascii_case("php") {
        load_proxy_kind::<crate::repository::php::PhpRepositoryConfig>(
            repository,
            site,
            crate::repository::php::PhpRepositoryConfigType::get_type_static(),
        )
        .await
    } else if repo_type.eq_ignore_ascii_case("ruby") {
        load_proxy_kind::<crate::repository::ruby::RubyRepositoryConfig>(
            repository,
            site,
            crate::repository::ruby::RubyRepositoryConfigType::get_type_static(),
        )
        .await
    } else if repo_type.eq_ignore_ascii_case("deb") {
        load_proxy_kind::<crate::repository::deb::DebRepositoryConfig>(
            repository,
            site,
            crate::repository::deb::DebRepositoryConfigType::get_type_static(),
        )
        .await
    } else {
        Ok(None)
    }
}

async fn load_proxy_kind<T>(
    repository: &DBRepositoryWithStorageName,
    site: &Pkgly,
    config_key: &'static str,
) -> Result<Option<String>, InternalError>
where
    T: ProxyKindClassifier + DeserializeOwned + Send + Sync + Unpin + 'static,
{
    let config =
        DBRepositoryConfig::<T>::get_config(repository.id, config_key, site.as_ref()).await?;
    let Some(config) = config else {
        return Ok(None);
    };
    Ok(config.value.0.proxy_kind_label().map(str::to_string))
}

trait ProxyKindClassifier {
    fn proxy_kind_label(&self) -> Option<&'static str>;
}

impl ProxyKindClassifier for crate::repository::docker::DockerRegistryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
        }
    }
}

impl ProxyKindClassifier for crate::repository::maven::MavenRepositoryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
        }
    }
}

impl ProxyKindClassifier for crate::repository::python::PythonRepositoryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
            Self::Virtual(_) => Some("virtual"),
        }
    }
}

impl ProxyKindClassifier for crate::repository::npm::NPMRegistryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
            Self::Virtual(_) => Some("virtual"),
        }
    }
}

impl ProxyKindClassifier for crate::repository::go::GoRepositoryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
        }
    }
}

impl ProxyKindClassifier for crate::repository::php::PhpRepositoryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
        }
    }
}

impl ProxyKindClassifier for crate::repository::deb::DebRepositoryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted(_) => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
        }
    }
}

impl ProxyKindClassifier for crate::repository::ruby::RubyRepositoryConfig {
    fn proxy_kind_label(&self) -> Option<&'static str> {
        match self {
            Self::Hosted => Some("hosted"),
            Self::Proxy(_) => Some("proxy"),
        }
    }
}

#[cfg(test)]
mod repository_kind_tests {
    use super::ProxyKindClassifier;
    use crate::repository::{
        docker::{DockerRegistryConfig, proxy::DockerProxyConfig},
        go::{GoProxyConfig, GoRepositoryConfig},
        maven::{MavenRepositoryConfig, proxy::MavenProxyConfig},
        npm::{
            NPMRegistryConfig, NpmProxyConfig,
            npm_virtual::{NpmVirtualConfig, VirtualResolutionOrder},
        },
        python::{PythonProxyConfig, PythonRepositoryConfig},
        ruby::RubyRepositoryConfig,
    };

    fn sample_docker_proxy() -> DockerProxyConfig {
        DockerProxyConfig {
            upstream_url: "https://registry-1.docker.io".into(),
            upstream_auth: None,
            revalidation_ttl_seconds: 300,
            skip_tag_revalidation: false,
        }
    }

    #[test]
    fn docker_proxy_reports_proxy_kind() {
        let config = DockerRegistryConfig::Proxy(sample_docker_proxy());
        assert_eq!(config.proxy_kind_label(), Some("proxy"));
    }

    #[test]
    fn docker_hosted_reports_hosted_kind() {
        let config = DockerRegistryConfig::Hosted;
        assert_eq!(config.proxy_kind_label(), Some("hosted"));
    }

    #[test]
    fn ruby_hosted_reports_hosted_kind() {
        let config = RubyRepositoryConfig::Hosted;
        assert_eq!(config.proxy_kind_label(), Some("hosted"));
    }

    #[test]
    fn maven_proxy_reports_proxy_kind() {
        let config = MavenRepositoryConfig::Proxy(MavenProxyConfig::default());
        assert_eq!(config.proxy_kind_label(), Some("proxy"));
    }

    #[test]
    fn maven_hosted_reports_hosted_kind() {
        let config = MavenRepositoryConfig::Hosted;
        assert_eq!(config.proxy_kind_label(), Some("hosted"));
    }

    #[test]
    fn python_proxy_reports_proxy_kind() {
        let config = PythonRepositoryConfig::Proxy(PythonProxyConfig::default());
        assert_eq!(config.proxy_kind_label(), Some("proxy"));
    }

    #[test]
    fn python_hosted_reports_hosted_kind() {
        let config = PythonRepositoryConfig::Hosted;
        assert_eq!(config.proxy_kind_label(), Some("hosted"));
    }

    #[test]
    fn npm_proxy_reports_proxy_kind() {
        let config = NPMRegistryConfig::Proxy(NpmProxyConfig::default());
        assert_eq!(config.proxy_kind_label(), Some("proxy"));
    }

    #[test]
    fn npm_hosted_reports_hosted_kind() {
        let config = NPMRegistryConfig::Hosted;
        assert_eq!(config.proxy_kind_label(), Some("hosted"));
    }

    #[test]
    fn npm_virtual_reports_virtual_kind() {
        let config = NPMRegistryConfig::Virtual(NpmVirtualConfig {
            member_repositories: Vec::new(),
            resolution_order: VirtualResolutionOrder::Priority,
            cache_ttl_seconds: 60,
            publish_to: None,
        });
        assert_eq!(config.proxy_kind_label(), Some("virtual"));
    }

    #[test]
    fn go_proxy_reports_proxy_kind() {
        let config = GoRepositoryConfig::Proxy(GoProxyConfig::default());
        assert_eq!(config.proxy_kind_label(), Some("proxy"));
    }

    #[test]
    fn go_hosted_reports_hosted_kind() {
        let config = GoRepositoryConfig::Hosted;
        assert_eq!(config.proxy_kind_label(), Some("hosted"));
    }
}

async fn compute_repository_storage_usage(site: &Pkgly, repository_id: Uuid) -> Option<u64> {
    let repository = site.get_repository(repository_id)?;
    match calculate_repository_storage_usage(&repository).await {
        Ok(size) => Some(size),
        Err(err) => {
            warn!(%repository_id, ?err, "Failed to calculate repository storage usage");
            None
        }
    }
}

async fn calculate_repository_storage_usage(
    repository: &DynRepository,
) -> Result<u64, nr_storage::StorageError> {
    let storage = repository.get_storage();
    let repository_id = repository.id();

    if let nr_storage::DynStorage::Local(local) = storage.clone() {
        match local.repository_size_bytes(repository_id).await {
            Ok(size) => return Ok(size),
            Err(err) => {
                warn!(
                    %repository_id,
                    %err,
                    "Fast local storage usage refresh failed; falling back to metadata traversal"
                );
            }
        }
    }

    if let nr_storage::DynStorage::S3(s3) = storage.clone() {
        match s3.repository_size_bytes(repository_id).await {
            Ok(size) => return Ok(size),
            Err(err) => {
                warn!(
                    %repository_id,
                    %err,
                    "Fast S3 storage usage refresh failed; falling back to metadata traversal"
                );
            }
        }
    }

    calculate_repository_storage_usage_fallback(storage, repository_id).await
}

async fn calculate_repository_storage_usage_fallback(
    storage: nr_storage::DynStorage,
    repository_id: Uuid,
) -> Result<u64, nr_storage::StorageError> {
    // Start with root directory
    let root_path = StoragePath::from("/");
    let Some(root_entry) = storage.open_file(repository_id, &root_path).await? else {
        return Ok(0);
    };

    match root_entry {
        StorageFile::File { meta, .. } => Ok(meta.file_type.file_size),
        StorageFile::Directory { files, .. } => {
            use tokio::task::JoinSet;
            const MAX_CONCURRENT_TASKS: usize = 20;

            let mut total = 0u64;
            let mut tasks = JoinSet::new();
            let mut queue: VecDeque<String> = VecDeque::new();

            for entry in &files {
                if let FileType::Directory(_) = entry.file_type() {
                    let mut path = String::from("/");
                    path.push_str(entry.name());
                    path.push('/');
                    queue.push_back(path);
                }
            }

            for entry in &files {
                if let FileType::File(file_meta) = entry.file_type() {
                    if tasks.len() < MAX_CONCURRENT_TASKS {
                        let file_size = file_meta.file_size;
                        tasks.spawn(async move { file_size });
                    } else {
                        while let Some(result) = tasks.join_next().await {
                            total += result.unwrap_or(0);
                        }
                        let file_size = file_meta.file_size;
                        tasks.spawn(async move { file_size });
                    }
                }
            }

            while let Some(path) = queue.pop_front() {
                let storage_path = StoragePath::from(path.as_str());
                if let Ok(Some(entry)) = storage.open_file(repository_id, &storage_path).await {
                    if let StorageFile::Directory { files, .. } = entry {
                        for file_entry in &files {
                            match file_entry.file_type() {
                                FileType::File(file_meta) => {
                                    if tasks.len() < MAX_CONCURRENT_TASKS {
                                        let file_size = file_meta.file_size;
                                        tasks.spawn(async move { file_size });
                                    } else {
                                        while let Some(result) = tasks.join_next().await {
                                            total += result.unwrap_or(0);
                                        }
                                        let file_size = file_meta.file_size;
                                        tasks.spawn(async move { file_size });
                                    }
                                }
                                FileType::Directory(_) => {
                                    let mut next_path = path.clone();
                                    next_path.push_str(file_entry.name());
                                    next_path.push('/');
                                    queue.push_back(next_path);
                                }
                            }
                        }
                    }
                }
            }

            while let Some(result) = tasks.join_next().await {
                total += result.unwrap_or(0);
            }

            Ok(total)
        }
    }
}

fn normalize_cached_usage(value: Option<i64>) -> Option<u64> {
    value.and_then(|raw| u64::try_from(raw).ok())
}

async fn refresh_repository_storage_usage(
    site: &Pkgly,
    repository_id: Uuid,
) -> Result<Option<(u64, chrono::DateTime<chrono::FixedOffset>)>, InternalError> {
    let Some(usage) = compute_repository_storage_usage(site, repository_id).await else {
        return Ok(None);
    };

    let updated_at = DBRepository::update_storage_usage(repository_id, Some(usage), &site.database)
        .await
        .map_err(InternalError::from)?;

    Ok(Some((usage, updated_at)))
}
#[derive(Debug, Clone, Copy, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueryRepositoryNames {
    /// Rather or not to include the visibility of the repository
    #[serde(default)]
    #[param(default = false)]
    pub include_visibility: bool,
}
#[utoipa::path(
    get,
    path = "/{repository_id}/names",
    params(
        QueryRepositoryNames,
        ("repository_id" = Uuid, Path, description = "The Repository ID"),
    ),
    responses(
        (status = 200, description = "The Storage Name/ID and the Repository Name/ID for the given Repository ID", body = DBRepositoryNames),
        (status = 200, description = "The Storage Name/ID and the Repository Name/ID for the given Repository ID", body = DBRepositoryNamesWithVisibility),
        (status = 404, description = "Repository not found"),
        (status = 403, description = "Missing permission"),
    )
)]
#[instrument(skip(site, auth, query), fields(repository_id = %repository_id))]
pub async fn get_repository_names(
    State(site): State<Pkgly>,
    auth: Option<Authentication>,
    Query(query): Query<QueryRepositoryNames>,
    Path(repository_id): Path<Uuid>,
) -> Result<Response, InternalError> {
    let Some(repository) =
        DBRepositoryNamesWithVisibility::get_by_id(repository_id, site.as_ref()).await?
    else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };
    if repository.visibility.is_private()
        && !auth
            .has_action(RepositoryActions::Read, repository_id, site.as_ref())
            .await?
    {
        return Ok(MissingPermission::ReadRepository(repository_id).into_response());
    }
    if query.include_visibility {
        Ok(ResponseBuilder::ok().json(&repository))
    } else {
        Ok(ResponseBuilder::ok().json(&DBRepositoryNames::from(repository)))
    }
}
