use ahash::HashMap;
use axum::{
    Json, Router, debug_handler,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use chrono::{DateTime, FixedOffset, Utc};
use nr_core::{
    database::entities::{
        repository::{DBRepository, GenericDBRepositoryConfig},
        storage::{DBStorage, StorageDBType},
    },
    repository::{Visibility, config::RepositoryConfigType},
    storage::StorageName,
    user::permissions::{HasPermissions, RepositoryActions},
};
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use tracing::{debug, error, info, instrument};
use utoipa::ToSchema;
use uuid::Uuid;

use super::r#virtual::normalize_virtual_repository_request_value;
use crate::{
    app::{
        Pkgly,
        authentication::{Authentication, AuthenticationError},
        responses::{InvalidRepositoryConfig, MissingPermission, RepositoryNotFound},
    },
    error::InternalError,
    repository::{
        DynRepository, Repository,
        retention::{config::PackageRetentionConfigType, is_virtual_repository},
    },
    utils::{ResponseBuilder, conflict::ConflictResponse},
};
use nr_storage::Storage;

fn repository_supports_config(repository: &DynRepository, config_key: &str) -> bool {
    repository_supported_config_types(repository).contains(&config_key)
}

fn repository_supported_config_types(repository: &DynRepository) -> Vec<&str> {
    let mut config_types = repository.config_types();
    let retention_key = PackageRetentionConfigType::get_type_static();
    if !is_virtual_repository(repository) && !config_types.contains(&retention_key) {
        config_types.push(retention_key);
    }
    config_types
}

fn unsupported_config_response(repository: &DynRepository, config_key: String) -> Response {
    InvalidRepositoryConfig::RepositoryTypeDoesntSupportConfig {
        repository_type: repository.get_type().to_owned(),
        config_key,
    }
    .into_response()
}

fn creation_request_is_virtual(repository_type: &str, configs: &HashMap<String, Value>) -> bool {
    let repository_type = repository_type.to_lowercase();
    if !matches!(repository_type.as_str(), "npm" | "python" | "nuget") {
        return false;
    }
    configs
        .get(repository_type.as_str())
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .map(|value| value.eq_ignore_ascii_case("virtual"))
        .unwrap_or(false)
}

fn validate_create_time_package_retention_config(
    site: &Pkgly,
    repository_type: &str,
    configs: &HashMap<String, Value>,
) -> Option<Response> {
    let retention_key = PackageRetentionConfigType::get_type_static();
    let config = configs.get(retention_key)?;
    if creation_request_is_virtual(repository_type, configs) {
        return Some(
            InvalidRepositoryConfig::RepositoryTypeDoesntSupportConfig {
                repository_type: repository_type.to_string(),
                config_key: retention_key.to_string(),
            }
            .into_response(),
        );
    }
    let Some(config_type) = site.get_repository_config_type(retention_key) else {
        return Some(ResponseBuilder::internal_server_error().body(format!(
            "Missing repository config type registration for key {}",
            retention_key
        )));
    };
    if let Err(error) = config_type.validate_config(config.clone()) {
        return Some(
            InvalidRepositoryConfig::InvalidConfig {
                config_key: retention_key.to_string(),
                error,
            }
            .into_response(),
        );
    }
    None
}
pub fn management_routes() -> Router<Pkgly> {
    Router::new()
        .route("/{repository_id}/configs", get(get_configs_for_repository))
        .route("/new/{repository_type}", post(new_repository))
        .route("/{repository_id}/config/{key}", put(update_config))
        .route("/{repository_id}/config/{key}", get(get_config))
        .route("/{repository_id}/deb/refresh", post(deb_refresh))
        .route(
            "/{repository_id}/deb/refresh/status",
            get(deb_refresh_status),
        )
        .route("/{repository_id}", delete(delete_repository))
}

fn format_missing_storage_error(storage_id: Uuid, repository: Uuid) -> String {
    format!(
        "Storage backend {} not available for repository {}",
        storage_id, repository
    )
}

async fn delete_repository_sequence<DB, DBFut, DBE, ST, STFut, STE, R>(
    delete_from_db: DB,
    delete_from_storage: ST,
    remove_from_memory: R,
) -> Result<(), InternalError>
where
    DB: FnOnce() -> DBFut,
    DBFut: Future<Output = Result<(), DBE>>,
    DBE: Into<InternalError>,
    ST: FnOnce() -> STFut,
    STFut: Future<Output = Result<(), STE>>,
    STE: Into<InternalError>,
    R: FnOnce(),
{
    delete_from_db().await.map_err(Into::into)?;
    delete_from_storage().await.map_err(Into::into)?;
    remove_from_memory();
    Ok(())
}
#[derive(Deserialize, ToSchema, Debug)]
pub struct NewRepositoryRequest {
    /// The Name of the Repository
    pub name: String,
    /// The Storage ID
    #[serde(default)]
    pub storage: Option<Uuid>,
    /// The Storage Name
    #[serde(default)]
    pub storage_name: Option<StorageName>,
    /// Optional Sub Type of the Repository
    /// A Map of Config Key to Config Value
    pub configs: HashMap<String, Value>,
}
#[utoipa::path(
    post,
    request_body = NewRepositoryRequest,
    path = "/new/{repository_type}",
    params(
        ("repository_type" = String, Path, description = "The Repository Type"),
    ),
    responses(
        (status = 200, description = "Create new Repository", body = DBRepository),
    )
)]
#[instrument(
    skip(site, auth, request),
    fields(user = %auth.id, repository_type = %repository_type)
)]
pub async fn new_repository(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository_type): Path<String>,
    Json(request): Json<NewRepositoryRequest>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(MissingPermission::RepositoryManager.into_response());
    }
    let NewRepositoryRequest {
        name,
        mut configs,
        storage,
        storage_name,
    } = request;
    let Some(repository_factory) = site.get_repository_type(&repository_type) else {
        return Ok(InvalidRepositoryConfig::InvalidConfigType(repository_type).into_response());
    };

    let storage = match (storage, storage_name) {
        (Some(storage), None) => DBStorage::get_by_id(storage, &site.database).await?,
        (None, Some(storage_name)) => {
            DBStorage::get_by_name(storage_name.as_ref(), &site.database).await?
        }
        (Some(_), Some(_)) => {
            return Ok(ResponseBuilder::bad_request()
                .body("Provide either storage or storage_name, not both"));
        }
        (None, None) => {
            return Ok(ResponseBuilder::bad_request().body("Missing storage or storage_name"));
        }
    };
    let Some(storage) = storage else {
        return Ok(ResponseBuilder::bad_request().body("Invalid Storage"));
    };
    let storage_id = storage.id;

    let Some(loaded_storage) = site.get_storage(storage_id) else {
        return Ok(ResponseBuilder::bad_request().body("Invalid Storage"));
    };
    if DBRepository::does_name_exist_for_storage(storage_id, &name, &site.database).await? {
        return Ok(ConflictResponse::from("name").into_response());
    }

    let uuid = DBRepository::generate_uuid(&site.database).await?;
    for config_key in repository_factory.config_types() {
        if configs.contains_key(config_key) {
            continue;
        }
        let Some(config_type) = site.get_repository_config_type(config_key) else {
            error!(
                "Repository {} requires config {} but type was not registered",
                repository_factory.get_type(),
                config_key
            );
            return Ok(ResponseBuilder::internal_server_error().body(format!(
                "Missing repository config type registration for key {}",
                config_key
            )));
        };
        match config_type.default() {
            Ok(default) => {
                configs.insert((*config_key).to_string(), default);
            }
            Err(err) => {
                error!(
                    "Failed to load default config {} for repository {}: {}",
                    config_key,
                    repository_factory.get_type(),
                    err
                );
                return Ok(InvalidRepositoryConfig::InvalidConfig {
                    config_key: config_key.to_string(),
                    error: err,
                }
                .into_response());
            }
        }
    }

    if matches!(repository_type.as_str(), "npm" | "python" | "nuget") {
        if let Some(repository_config) = configs.get_mut(repository_type.as_str()) {
            if let Err(message) = normalize_virtual_repository_request_value(
                storage_id,
                repository_config,
                &site.database,
            )
            .await
            {
                return Ok(ResponseBuilder::bad_request().body(message));
            }
        }
    }

    if let Some(response) =
        validate_create_time_package_retention_config(&site, &repository_type, &configs)
    {
        return Ok(response);
    }

    let repository = repository_factory
        .create_new(name, uuid, configs, loaded_storage.clone())
        .await;
    let repository = match repository {
        Ok(repository) => repository,
        Err(err) => {
            error!("Failed to create repository: {}", err);
            return Ok(ResponseBuilder::internal_server_error().body("Failed to create repository"));
        }
    };
    let db_repository = repository.insert(storage_id, site.as_ref()).await?;
    match repository_factory
        .load_repo(db_repository.clone(), loaded_storage, site.clone())
        .await
    {
        Ok(loaded) => {
            site.add_repository(db_repository.id, loaded);
        }
        Err(err) => {
            error!("Failed to load repository: {}", err);
            return Ok(ResponseBuilder::internal_server_error().body("Failed to load repository"));
        }
    }
    Ok(ResponseBuilder::created().json(&db_repository))
}

#[utoipa::path(
    get,
    path = "/{repository_id}/configs",
    params(
        ("repository_id" = Uuid, Path,description = "The Repository ID"),
    ),
    responses(
        (status = 200, description = "List Configs for Repository", body = [String]),
    )
)]
#[instrument(skip(site, auth), fields(user = %auth.id, repository_id = %repository))]
pub async fn get_configs_for_repository(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository): Path<Uuid>,
) -> Result<Response, InternalError> {
    if !auth
        .has_action(RepositoryActions::Edit, repository, &site.database)
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository).into_response());
    }
    let Some(repository) = site.get_repository(repository) else {
        return Ok(RepositoryNotFound::Uuid(repository).into_response());
    };

    let config_types = repository_supported_config_types(&repository);
    debug!(configs = ?config_types, "Repository config types");
    Ok(ResponseBuilder::ok().json(&config_types))
}
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct GetConfigParams {
    default: bool,
}
#[utoipa::path(
    get,
    path = "/{repository_id}/config/{config_key}",
    params(
        ("repository_id" = Uuid, Path, description = "The Repository ID"),
        ("config_key" = String, Path, description = "The Config Key"),
    ),
    responses(
        (status = 200, description = "Config for the repository"),
    )
)]
#[debug_handler]
#[instrument(
    skip(site, auth, params),
    fields(repository_id = %repository, config_key = %config)
)]
pub async fn get_config(
    State(site): State<Pkgly>,
    auth: Option<Authentication>,
    Query(params): Query<GetConfigParams>,
    Path((repository, config)): Path<(Uuid, String)>,
) -> Result<Response, InternalError> {
    let Some(config_type) = site.get_repository_config_type(&config) else {
        return Ok(InvalidRepositoryConfig::InvalidConfigType(config).into_response());
    };
    let Some(repository_value) = site.get_repository(repository) else {
        return Ok(RepositoryNotFound::Uuid(repository).into_response());
    };
    if !repository_supports_config(&repository_value, &config) {
        return Ok(unsupported_config_response(&repository_value, config));
    }
    let repository_visibility = repository_value.visibility();
    let config =
        match GenericDBRepositoryConfig::get_config(repository, &config, site.as_ref()).await? {
            Some(config) => config.value.0,
            None => {
                if params.default {
                    debug!("Getting default config for config type: {}", config);
                    config_type.default()?
                } else {
                    return Ok(RepositoryNotFound::Uuid(repository).into_response());
                }
            }
        };
    let config = if auth
        .has_action(RepositoryActions::Edit, repository, &site.database)
        .await?
    {
        Some(config)
    } else {
        // User does not have permission to view the config. Sanitize it
        // If None is returned, the user does not have permission to view the config
        debug!("Sanitizing config for public view");
        match repository_visibility {
            Visibility::Hidden | Visibility::Public => {
                config_type.sanitize_for_public_view(config)?
            }
            _ => None,
        }
    };
    if let Some(config) = config {
        Ok(ResponseBuilder::ok().json(&config))
    } else {
        Ok(AuthenticationError::Forbidden.into_response())
    }
}

#[utoipa::path(
    post,
    summary = "Refresh Debian proxy mirror",
    path = "/{repository_id}/deb/refresh",
    params(
        ("repository_id" = Uuid, Path, description = "The Repository ID"),
    ),
    responses(
        (status = 200, description = "Mirror refresh completed", body = crate::repository::deb::proxy_refresh::DebProxyRefreshSummary),
        (status = 400, description = "Repository is not a Debian proxy"),
        (status = 403, description = "Missing permissions"),
        (status = 409, description = "Refresh already running"),
    )
)]
#[instrument(skip(site, auth), fields(user = %auth.id, repository_id = %repository_id))]
pub async fn deb_refresh(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository_id): Path<Uuid>,
) -> Result<Response, InternalError> {
    if !auth
        .has_action(RepositoryActions::Edit, repository_id, &site.database)
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository_id).into_response());
    }

    let Some(repository) = site.get_repository(repository_id) else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };

    let DynRepository::Deb(crate::repository::deb::DebRepository::Proxy(proxy)) = repository else {
        return Ok(ResponseBuilder::bad_request().body("Repository is not a Debian proxy"));
    };

    use crate::repository::deb::refresh_status::{
        DebProxyRefreshLockOutcome, mark_deb_proxy_refresh_failed,
        mark_deb_proxy_refresh_succeeded, try_mark_deb_proxy_refresh_started,
    };

    let lock = match try_mark_deb_proxy_refresh_started(&site.database, repository_id).await? {
        DebProxyRefreshLockOutcome::Acquired(lock) => lock,
        DebProxyRefreshLockOutcome::AlreadyRunning => {
            return Ok(ResponseBuilder::conflict().body("Debian proxy refresh already running"));
        }
    };

    let refresh_result = proxy.refresh_offline_mirror().await;
    let status_update: Result<(), InternalError> = match &refresh_result {
        Ok(summary) => mark_deb_proxy_refresh_succeeded(&site.database, repository_id, *summary)
            .await
            .map_err(|err| err.into()),
        Err(err) => mark_deb_proxy_refresh_failed(&site.database, repository_id, &err.to_string())
            .await
            .map_err(|err| err.into()),
    };

    let response = match refresh_result {
        Ok(summary) => ResponseBuilder::ok().json(&summary),
        Err(err) => ResponseBuilder::internal_server_error().body(err.to_string()),
    };

    let release_result: Result<(), InternalError> = lock.release().await.map_err(|err| err.into());
    status_update?;
    release_result?;
    Ok(response)
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct DebProxyRefreshStatusResponse {
    pub in_progress: bool,
    pub last_started_at: Option<DateTime<FixedOffset>>,
    pub last_finished_at: Option<DateTime<FixedOffset>>,
    pub last_success_at: Option<DateTime<FixedOffset>>,
    pub last_error: Option<String>,
    pub last_downloaded_packages: Option<i32>,
    pub last_downloaded_files: Option<i32>,
    pub due: bool,
    pub next_run_at: Option<DateTime<Utc>>,
}

#[utoipa::path(
    get,
    summary = "Get Debian proxy mirror refresh status",
    path = "/{repository_id}/deb/refresh/status",
    params(
        ("repository_id" = Uuid, Path, description = "The Repository ID"),
    ),
    responses(
        (status = 200, description = "Mirror refresh status", body = DebProxyRefreshStatusResponse),
        (status = 400, description = "Repository is not a Debian proxy"),
        (status = 403, description = "Missing permissions"),
    )
)]
#[instrument(skip(site, auth), fields(user = %auth.id, repository_id = %repository_id))]
pub async fn deb_refresh_status(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository_id): Path<Uuid>,
) -> Result<Response, InternalError> {
    if !auth
        .has_action(RepositoryActions::Edit, repository_id, &site.database)
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository_id).into_response());
    }

    let Some(repository) = site.get_repository(repository_id) else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };

    let DynRepository::Deb(crate::repository::deb::DebRepository::Proxy(proxy)) = repository else {
        return Ok(ResponseBuilder::bad_request().body("Repository is not a Debian proxy"));
    };

    #[derive(sqlx::FromRow, Debug)]
    struct StatusRow {
        in_progress: bool,
        last_started_at: Option<DateTime<FixedOffset>>,
        last_finished_at: Option<DateTime<FixedOffset>>,
        last_success_at: Option<DateTime<FixedOffset>>,
        last_error: Option<String>,
        last_downloaded_packages: Option<i32>,
        last_downloaded_files: Option<i32>,
    }

    let status: Option<StatusRow> = sqlx::query_as(
        r#"
        SELECT in_progress,
               last_started_at,
               last_finished_at,
               last_success_at,
               last_error,
               last_downloaded_packages,
               last_downloaded_files
        FROM deb_proxy_refresh_status
        WHERE repository_id = $1
        "#,
    )
    .bind(repository_id)
    .fetch_optional(&site.database)
    .await?;

    let now = Utc::now();
    let schedule = proxy
        .0
        .config
        .refresh
        .as_ref()
        .filter(|refresh| refresh.enabled)
        .map(|refresh| &refresh.schedule);

    let (
        last_started_at,
        in_progress,
        last_finished_at,
        last_success_at,
        last_error,
        last_downloaded_packages,
        last_downloaded_files,
    ) = if let Some(status) = status {
        (
            status.last_started_at,
            status.in_progress,
            status.last_finished_at,
            status.last_success_at,
            status.last_error,
            status.last_downloaded_packages,
            status.last_downloaded_files,
        )
    } else {
        (None, false, None, None, None, None, None)
    };

    let due = schedule
        .map(|schedule| {
            crate::repository::deb::scheduler::is_due(now, schedule, last_started_at.clone())
        })
        .unwrap_or(false);
    let next_run_at = schedule.and_then(|schedule| {
        crate::repository::deb::scheduler::next_run_at(now, schedule, last_started_at)
    });

    Ok(ResponseBuilder::ok().json(&DebProxyRefreshStatusResponse {
        in_progress,
        last_started_at,
        last_finished_at,
        last_success_at,
        last_error,
        last_downloaded_packages,
        last_downloaded_files,
        due,
        next_run_at,
    }))
}
/// Updates a config for a repository
///
/// # Method Body
/// Should be a the message body for the type of config you are updating
#[utoipa::path(
    put,
    path = "/{repository_id}/config/{config_key}",
    params(
        ("repository_id" = Uuid,Path, description = "The Repository ID"),
        ("config_key" = String,Path, description = "The Config Key"),
    ),
    responses(
        (status = 204, description = "Updated a config for a repository"),
        (status = 404, description = "Repository not found"),
        (status = 400, description="Invalid Config value for the repository"),
    )
)]
#[instrument(
    skip(site, auth, config),
    fields(user = %auth.id, repository_id = %repository, config_key = %config_key)
)]
pub async fn update_config(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path((repository, config_key)): Path<(Uuid, String)>,
    Json(config): Json<serde_json::Value>,
) -> Result<Response, InternalError> {
    if !auth
        .has_action(RepositoryActions::Edit, repository, &site.database)
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository).into_response());
    }
    let Some(config_type) = site.get_repository_config_type(&config_key) else {
        return Ok(InvalidRepositoryConfig::InvalidConfigType(config_key).into_response());
    };
    let Some(db_repository) = DBRepository::get_by_id(repository, site.as_ref()).await? else {
        return Ok(RepositoryNotFound::Uuid(repository).into_response());
    };
    let Some(repository) = site.get_repository(db_repository.id) else {
        return Ok(ResponseBuilder::internal_server_error()
            .body("Repository Exists. But it is not loaded. Illegal State"));
    };
    if !repository_supports_config(&repository, &config_key) {
        return Ok(unsupported_config_response(&repository, config_key));
    }
    match GenericDBRepositoryConfig::get_config(repository.id(), &config_key, site.as_ref()).await?
    {
        Some(old) => {
            if let Err(error) = config_type.validate_change(old.value.0, config.clone()) {
                error!("Error validating config: {}", error);
                return Ok(
                    InvalidRepositoryConfig::InvalidConfig { config_key, error }.into_response()
                );
            }
        }
        None => {
            if let Err(error) = config_type.validate_config(config.clone()) {
                error!("Error validating config: {}", error);
                return Ok(
                    InvalidRepositoryConfig::InvalidConfig { config_key, error }.into_response()
                );
            }
        }
    };

    GenericDBRepositoryConfig::add_or_update(db_repository.id, config_key, config, site.as_ref())
        .await?;
    if let Err(err) = repository.reload().await {
        return Ok(ResponseBuilder::internal_server_error()
            .body(format!("Failed to reload repository: {}", err)));
    }
    Ok(ResponseBuilder::no_content().empty())
}

#[utoipa::path(
    delete,
    path = "/{repository}",
    params(
        ("repository_id" = Uuid, description = "The Repository ID"),
    ),
    responses(
        (status = 204, description = "Repository Deleted"),
    )
)]
pub async fn delete_repository(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository): Path<Uuid>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(MissingPermission::RepositoryManager.into_response());
    }
    let Some(db_repository) = DBRepository::get_by_id(repository, site.as_ref()).await? else {
        return Ok(RepositoryNotFound::Uuid(repository).into_response());
    };
    info!("Deleting Repository: {}", db_repository.name);

    let Some(storage) = site.get_storage(db_repository.storage_id) else {
        error!(
            repository = %repository,
            storage_id = %db_repository.storage_id,
            "Storage not loaded for repository deletion"
        );
        return Ok(
            ResponseBuilder::internal_server_error().body(format_missing_storage_error(
                db_repository.storage_id,
                repository,
            )),
        );
    };

    delete_repository_sequence(
        || DBRepository::delete_by_id(repository, site.as_ref()),
        || storage.delete_repository(repository),
        || site.remove_repository(repository),
    )
    .await?;

    Ok(ResponseBuilder::no_content().empty())
}

#[cfg(test)]
mod tests;
