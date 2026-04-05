use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use nr_core::{
    database::entities::storage::{DBStorage, DBStorageNoConfig, NewDBStorage, StorageDBType},
    storage::StorageName,
    user::permissions::HasPermissions,
};
use nr_storage::{StorageConfig, StorageConfigInner, StorageTypeConfig, local::LocalConfig};
use serde::{Deserialize, Serialize};
use tracing::{error, instrument};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;
mod local;
mod s3;
use crate::{
    app::{
        Pkgly,
        authentication::Authentication,
        responses::{InvalidStorageConfig, InvalidStorageType, MissingPermission},
    },
    error::InternalError,
    utils::{ResponseBuilder, conflict::ConflictResponse},
};
#[derive(OpenApi)]
#[openapi(
    paths(list_storages, new_storage, get_storage, update_storage),
    components(schemas(DBStorage, NewStorageRequest, StorageTypeConfig, LocalConfig)),
    nest(
        (path = "/local", api = local::LocalStorageAPI, tags=["local", "storage"]),
        (path = "/s3", api = s3::S3StorageAPI, tags=["s3", "storage"])
    ),
    tags(
        (name= "local", description = "Local Storage"),
        (name= "s3", description = "S3 Storage"),
    )
)]
pub struct StorageAPI;
pub fn storage_routes() -> axum::Router<crate::app::api::storage::Pkgly> {
    axum::Router::new()
        .route("/list", get(list_storages))
        .route("/new/{storage_type}", post(new_storage))
        .route("/{id}", get(get_storage))
        .route("/{id}", put(update_storage))
        .nest("/local", local::local_storage_routes())
        .nest("/s3", s3::s3_storage_api())
}
#[derive(Debug, Default, Serialize, Deserialize, ToSchema, IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
pub struct StorageListRequest {
    /// Include the storage configuration in the response (default: false)
    pub include_config: bool,
    /// Only include active storages (default: false)
    pub active_only: bool,
}
#[utoipa::path(
    get,
    path = "/list",
    params(
        StorageListRequest,
    ),
    responses(
        (status = 200, description = "All Storages registered to the system.", body = [DBStorage]),
        (status = 200, description = "All the storages without the configs", body = [DBStorageNoConfig]),
        (status = 403, description = "Does not have permission to view storages")
    )
)]
#[instrument(skip(site, auth, request), fields(user = %auth.id))]
pub async fn list_storages(
    State(site): State<Pkgly>,
    auth: Authentication,
    Query(request): Query<StorageListRequest>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(MissingPermission::StorageManager.into_response());
    }
    if request.include_config {
        let storages = DBStorage::get_all(&site.database).await?;
        Ok(ResponseBuilder::ok().json(&storages))
    } else {
        let storages = DBStorageNoConfig::get_all(&site.database).await?;
        Ok(ResponseBuilder::ok().json(&storages))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct NewStorageRequest {
    pub name: StorageName,
    pub config: StorageTypeConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UpdateStorageRequest {
    pub config: StorageTypeConfig,
}

#[utoipa::path(
    post,
    path = "/new/{storage_type}",
    request_body = NewStorageRequest,
    responses(
        (status = 201, description = "Storage Successfully Created", body = DBStorage),
        ConflictResponse,
        (status = 400, description = "Invalid Storage Config"),
    ),
    params(
        ("storage_type" = String, Path, description = "Storage Type"),
    )
)]
#[instrument(
    skip(auth, site, request),
    fields(user = %auth.id, storage_type = %storage_type)
)]
pub async fn new_storage(
    auth: Authentication,
    State(site): State<Pkgly>,
    Path(storage_type): Path<String>,
    Json(request): Json<NewStorageRequest>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(MissingPermission::StorageManager.into_response());
    }
    if !DBStorage::is_name_available(&request.name, site.as_ref()).await? {
        return Ok(ConflictResponse::from("name").into_response());
    }

    let Some(storage_factory) = site.get_storage_factory(&storage_type) else {
        return Ok(InvalidStorageType(storage_type).into_response());
    };
    if let Err(error) = storage_factory
        .test_storage_config(request.config.clone())
        .await
    {
        error!("Failed to test storage config: {}", error);
        return Ok(InvalidStorageConfig(error).into_response());
    }

    let config = serde_json::to_value(request.config)?;
    let storage = NewDBStorage::new(storage_type, request.name, config)
        .insert(&site.database)
        .await?;
    let Some(storage) = storage else {
        return Ok(ConflictResponse::from("name").into_response());
    };
    let id = storage.id;
    let storage_config = match StorageConfig::try_from(storage.clone()) {
        Ok(ok) => ok,
        Err(err) => {
            DBStorage::delete(id, site.as_ref()).await?;
            error!("Failed to create storage config: {}", err);
            return Err(InternalError::from(err));
        }
    };
    match storage_factory.create_storage(storage_config).await {
        Ok(ok) => site.add_storage(id, ok),
        Err(err) => {
            DBStorage::delete(id, site.as_ref()).await?;
            error!("Failed to create storage: {}", err);
            return Err(InternalError::from(err));
        }
    }
    Ok(ResponseBuilder::created().json(&storage))
}

#[utoipa::path(
    put,
    path = "/{id}",
    request_body = UpdateStorageRequest,
    responses(
        (status = 200, description = "Storage Successfully Updated", body = DBStorage),
        (status = 400, description = "Invalid Storage Config"),
        (status = 404, description = "Storage not found"),
    ),
    params(
        ("id" = Uuid, Path, description = "Storage ID"),
    )
)]
#[instrument(skip(auth, site, request), fields(user = %auth.id, storage_id = %id))]
pub async fn update_storage(
    auth: Authentication,
    State(site): State<Pkgly>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateStorageRequest>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(MissingPermission::StorageManager.into_response());
    }

    let Some(existing) = DBStorage::get_by_id(id, &site.database).await? else {
        return Ok(ResponseBuilder::not_found().body("Storage not found"));
    };

    let Some(factory) = site.get_storage_factory(&existing.storage_type) else {
        return Ok(InvalidStorageType(existing.storage_type).into_response());
    };

    // Validate provided config matches storage type
    if !existing
        .storage_type
        .eq_ignore_ascii_case(request.config.type_name())
    {
        return Ok(InvalidStorageType(existing.storage_type).into_response());
    }

    if let Err(error) = factory.test_storage_config(request.config.clone()).await {
        error!(%error, storage = %id, "Failed to test updated storage config");
        return Ok(InvalidStorageConfig(error).into_response());
    }

    // Persist new config
    let config_json = serde_json::to_value(request.config.clone())?;
    let updated =
        DBStorage::update_config(id, serde_json::json!(config_json).into(), &site.database).await?;
    let Some(updated) = updated else {
        return Ok(ResponseBuilder::not_found().body("Storage not found"));
    };

    // Rebuild runtime storage and replace
    let storage_config = StorageConfig {
        storage_config: StorageConfigInner {
            storage_name: updated.name.clone().into(),
            storage_id: updated.id,
            storage_type: updated.storage_type.clone(),
            created_at: updated.created_at,
        },
        type_config: request.config.clone(),
    };
    let new_storage = factory
        .create_storage(storage_config)
        .await
        .map_err(InternalError::from)?;
    site.replace_storage(updated.id, new_storage);

    Ok(ResponseBuilder::ok().json(&updated))
}

#[utoipa::path(
    post,
    path = "/{id}",
    responses(
        (status = 200, description = "Storage Configuration", body = DBStorage),
        (status = 404, description = "Storage not found")
    )
)]
#[instrument(skip(auth, site), fields(user = %auth.id, storage_id = %id))]
pub async fn get_storage(
    auth: Authentication,
    Path(id): Path<Uuid>,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(MissingPermission::StorageManager.into_response());
    }
    let storage = DBStorage::get_by_id(id, &site.database).await?;
    match storage {
        Some(storage) => Ok(ResponseBuilder::ok().json(&storage)),
        None => Ok(ResponseBuilder::not_found().body("Storage not found")),
    }
}
