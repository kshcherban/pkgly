use axum::{
    Json, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, put},
};
use nr_core::{
    database::entities::repository::{
        DBRepository, DBRepositoryConfig, DBVirtualRepositoryMember, GenericDBRepositoryConfig,
        NewVirtualRepositoryMember,
    },
    database::entities::storage::{DBStorage, StorageDBType},
    repository::config::RepositoryConfigType,
    user::permissions::{HasPermissions, RepositoryActions},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::{
        Pkgly,
        authentication::Authentication,
        responses::{MissingPermission, RepositoryNotFound},
    },
    error::{InternalError, OtherInternalError},
    repository::{
        DynRepository, Repository,
        npm::{
            NPMRegistry, NPMRegistryConfig, NPMRegistryConfigType,
            npm_virtual::{VirtualRepositoryMemberConfig, VirtualResolutionOrder},
        },
        nuget::{NugetRepository, NugetRepositoryConfig, NugetRepositoryConfigType},
        python::{PythonRepository, PythonRepositoryConfig, PythonRepositoryConfigType},
        r#virtual::config::VirtualRepositoryConfig,
    },
    utils::ResponseBuilder,
};

pub fn virtual_routes() -> Router<Pkgly> {
    Router::new()
        .route(
            "/{repository_id}/virtual/members",
            get(list_members).post(update_members),
        )
        .route(
            "/{repository_id}/virtual/resolution-order",
            put(update_resolution_order),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VirtualMemberView {
    #[schema(value_type = String, format = "uuid")]
    pub repository_id: Uuid,
    pub repository_name: String,
    pub priority: u32,
    pub enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VirtualConfigView {
    pub resolution_order: VirtualResolutionOrder,
    pub cache_ttl_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub publish_to: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publish_to_name: Option<String>,
    pub members: Vec<VirtualMemberView>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum RepositoryReference {
    #[schema(value_type = String, format = "uuid")]
    Id(Uuid),
    Name(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
pub struct VirtualRepositoryMemberInput {
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub repository_id: Option<Uuid>,
    pub repository_name: String,
    #[serde(default)]
    pub priority: u32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
pub struct VirtualRepositoryConfigInput {
    #[serde(default)]
    pub member_repositories: Vec<VirtualRepositoryMemberInput>,
    #[serde(default)]
    pub resolution_order: VirtualResolutionOrder,
    #[serde(default = "default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub publish_to: Option<RepositoryReference>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMembersRequest {
    pub members: Vec<VirtualRepositoryMemberInput>,
    #[serde(default)]
    pub resolution_order: Option<VirtualResolutionOrder>,
    #[serde(default)]
    pub cache_ttl_seconds: Option<u64>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub publish_to: Option<RepositoryReference>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateResolutionOrderRequest {
    pub resolution_order: VirtualResolutionOrder,
    #[serde(default)]
    pub cache_ttl_seconds: Option<u64>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub publish_to: Option<RepositoryReference>,
}

#[utoipa::path(
    get,
    path = "/api/repository/{repository_id}/virtual/members",
    params(("repository_id" = Uuid, Path, description = "Virtual repository id")),
    responses((status = 200, description = "Virtual members", body = VirtualConfigView))
)]
async fn list_members(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository_id): Path<Uuid>,
) -> Result<axum::response::Response, InternalError> {
    if !auth
        .has_action(RepositoryActions::Edit, repository_id, site.as_ref())
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository_id).into_response());
    }
    let Some(repository) = DBRepository::get_by_id(repository_id, site.as_ref()).await? else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };
    let virtual_config: VirtualRepositoryConfig = match repository
        .repository_type
        .to_ascii_lowercase()
        .as_str()
    {
        "npm" => {
            let config = DBRepositoryConfig::<NPMRegistryConfig>::get_config(
                repository_id,
                NPMRegistryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                NPMRegistryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        "python" => {
            let config = DBRepositoryConfig::<PythonRepositoryConfig>::get_config(
                repository_id,
                PythonRepositoryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                PythonRepositoryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        "nuget" => {
            let config = DBRepositoryConfig::<NugetRepositoryConfig>::get_config(
                repository_id,
                NugetRepositoryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                NugetRepositoryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        _ => {
            return Ok(
                ResponseBuilder::bad_request().body("Repository is not NPM, Python, or NuGet")
            );
        }
    };

    let members = DBVirtualRepositoryMember::list_for_virtual(repository_id, site.as_ref()).await?;
    let views = hydrate_members(&members, &site).await?;
    let publish_to_name = resolve_publish_to_name(virtual_config.publish_to, site.as_ref()).await?;

    let view = VirtualConfigView {
        resolution_order: virtual_config.resolution_order,
        cache_ttl_seconds: virtual_config.cache_ttl_seconds,
        publish_to: virtual_config.publish_to,
        publish_to_name,
        members: views,
    };
    Ok(ResponseBuilder::ok().json(&view))
}

#[utoipa::path(
    post,
    path = "/api/repository/{repository_id}/virtual/members",
    request_body = UpdateMembersRequest,
    params(("repository_id" = Uuid, Path, description = "Virtual repository id")),
    responses((status = 200, description = "Updated virtual members", body = VirtualConfigView))
)]
async fn update_members(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository_id): Path<Uuid>,
    Json(payload): Json<UpdateMembersRequest>,
) -> Result<axum::response::Response, InternalError> {
    if !auth
        .has_action(RepositoryActions::Edit, repository_id, site.as_ref())
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository_id).into_response());
    }

    let Some(repository) = DBRepository::get_by_id(repository_id, site.as_ref()).await? else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };
    let repo_type = repository.repository_type.to_ascii_lowercase();
    let mut config: VirtualRepositoryConfig = match repo_type.as_str() {
        "npm" => {
            let config = DBRepositoryConfig::<NPMRegistryConfig>::get_config(
                repository_id,
                NPMRegistryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                NPMRegistryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        "python" => {
            let config = DBRepositoryConfig::<PythonRepositoryConfig>::get_config(
                repository_id,
                PythonRepositoryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                PythonRepositoryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        "nuget" => {
            let config = DBRepositoryConfig::<NugetRepositoryConfig>::get_config(
                repository_id,
                NugetRepositoryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                NugetRepositoryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        _ => {
            return Ok(
                ResponseBuilder::bad_request().body("Repository is not NPM, Python, or NuGet")
            );
        }
    };

    let members =
        match resolve_member_inputs(repository.storage_id, &payload.members, site.as_ref()).await {
            Ok(members) => members,
            Err(message) => return Ok(ResponseBuilder::bad_request().body(message)),
        };
    if members.is_empty() {
        return Ok(ResponseBuilder::bad_request().body("Member list cannot be empty"));
    }
    if has_duplicate_members(&members) {
        return Ok(ResponseBuilder::bad_request().body("Duplicate member repositories provided"));
    }

    if let Some(order) = payload.resolution_order {
        config.resolution_order = order;
    }
    if let Some(ttl) = payload.cache_ttl_seconds {
        if ttl == 0 {
            return Ok(ResponseBuilder::bad_request().body("cache_ttl_seconds must be > 0"));
        }
        config.cache_ttl_seconds = ttl;
    }
    if let Some(publish_to) = payload.publish_to {
        let resolved_publish_to =
            match resolve_publish_target(repository.storage_id, Some(&publish_to), site.as_ref())
                .await
            {
                Ok(resolved_publish_to) => resolved_publish_to,
                Err(message) => return Ok(ResponseBuilder::bad_request().body(message)),
            };
        let Some(resolved_publish_to) = resolved_publish_to else {
            return Ok(ResponseBuilder::bad_request().body("publish_to must not be empty"));
        };
        config.publish_to = Some(resolved_publish_to);
    }

    config.member_repositories = members;

    if let Err(message) = validate_publish_target(repo_type.as_str(), &config, site.as_ref()).await
    {
        return Ok(ResponseBuilder::bad_request().body(message));
    }

    persist_virtual_config(repository_id, repo_type.as_str(), &config, site.as_ref()).await?;
    replace_members(repository_id, &config, site.as_ref()).await?;
    reload_runtime_virtual(&site, repository_id).await;

    list_members(State(site), auth, Path(repository_id)).await
}

#[utoipa::path(
    put,
    path = "/api/repository/{repository_id}/virtual/resolution-order",
    request_body = UpdateResolutionOrderRequest,
    params(("repository_id" = Uuid, Path, description = "Virtual repository id")),
    responses((status = 200, description = "Updated resolution order", body = VirtualConfigView))
)]
async fn update_resolution_order(
    State(site): State<Pkgly>,
    auth: Authentication,
    Path(repository_id): Path<Uuid>,
    Json(payload): Json<UpdateResolutionOrderRequest>,
) -> Result<axum::response::Response, InternalError> {
    if !auth
        .has_action(RepositoryActions::Edit, repository_id, site.as_ref())
        .await?
    {
        return Ok(MissingPermission::EditRepository(repository_id).into_response());
    }

    let Some(repository) = DBRepository::get_by_id(repository_id, site.as_ref()).await? else {
        return Ok(RepositoryNotFound::Uuid(repository_id).into_response());
    };
    let repo_type = repository.repository_type.to_ascii_lowercase();
    let mut config: VirtualRepositoryConfig = match repo_type.as_str() {
        "npm" => {
            let config = DBRepositoryConfig::<NPMRegistryConfig>::get_config(
                repository_id,
                NPMRegistryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                NPMRegistryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        "python" => {
            let config = DBRepositoryConfig::<PythonRepositoryConfig>::get_config(
                repository_id,
                PythonRepositoryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                PythonRepositoryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        "nuget" => {
            let config = DBRepositoryConfig::<NugetRepositoryConfig>::get_config(
                repository_id,
                NugetRepositoryConfigType::get_type_static(),
                site.as_ref(),
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                NugetRepositoryConfig::Virtual(cfg) => cfg,
                _ => return Ok(ResponseBuilder::bad_request().body("Repository is not virtual")),
            }
        }
        _ => {
            return Ok(
                ResponseBuilder::bad_request().body("Repository is not NPM, Python, or NuGet")
            );
        }
    };

    config.resolution_order = payload.resolution_order;
    if let Some(ttl) = payload.cache_ttl_seconds {
        config.cache_ttl_seconds = ttl.max(1);
    }
    if let Some(publish_to) = payload.publish_to {
        let resolved_publish_to =
            match resolve_publish_target(repository.storage_id, Some(&publish_to), site.as_ref())
                .await
            {
                Ok(resolved_publish_to) => resolved_publish_to,
                Err(message) => return Ok(ResponseBuilder::bad_request().body(message)),
            };
        config.publish_to = resolved_publish_to;
    }

    if let Err(message) = validate_publish_target(repo_type.as_str(), &config, site.as_ref()).await
    {
        return Ok(ResponseBuilder::bad_request().body(message));
    }

    persist_virtual_config(repository_id, repo_type.as_str(), &config, site.as_ref()).await?;
    reload_runtime_virtual(&site, repository_id).await;

    list_members(State(site), auth, Path(repository_id)).await
}

async fn hydrate_members(
    members: &[DBVirtualRepositoryMember],
    site: &Pkgly,
) -> Result<Vec<VirtualMemberView>, InternalError> {
    let mut views = Vec::with_capacity(members.len());
    for member in members {
        let Some(repo) =
            DBRepository::get_by_id(member.member_repository_id, site.as_ref()).await?
        else {
            continue;
        };
        views.push(VirtualMemberView {
            repository_id: repo.id,
            repository_name: repo.name.to_string(),
            priority: member.priority.max(0) as u32,
            enabled: member.enabled,
        });
    }
    Ok(views)
}

async fn persist_virtual_config(
    repository_id: Uuid,
    repository_type: &str,
    config: &VirtualRepositoryConfig,
    database: &sqlx::PgPool,
) -> Result<(), InternalError> {
    match repository_type {
        "npm" => {
            let value = serde_json::to_value(NPMRegistryConfig::Virtual(config.clone()))
                .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
            GenericDBRepositoryConfig::add_or_update(
                repository_id,
                NPMRegistryConfigType::get_type_static().to_string(),
                value,
                database,
            )
            .await?;
        }
        "python" => {
            let value = serde_json::to_value(PythonRepositoryConfig::Virtual(config.clone()))
                .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
            GenericDBRepositoryConfig::add_or_update(
                repository_id,
                PythonRepositoryConfigType::get_type_static().to_string(),
                value,
                database,
            )
            .await?;
        }
        "nuget" => {
            let value = serde_json::to_value(NugetRepositoryConfig::Virtual(config.clone()))
                .map_err(|err| InternalError::from(OtherInternalError::new(err)))?;
            GenericDBRepositoryConfig::add_or_update(
                repository_id,
                NugetRepositoryConfigType::get_type_static().to_string(),
                value,
                database,
            )
            .await?;
        }
        _ => {
            let err = std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Repository type {repository_type} does not support virtual config"),
            );
            return Err(InternalError::from(OtherInternalError::new(err)));
        }
    }
    Ok(())
}

async fn replace_members(
    repository_id: Uuid,
    config: &VirtualRepositoryConfig,
    database: &sqlx::PgPool,
) -> Result<(), InternalError> {
    let members: Vec<_> = config
        .member_repositories
        .iter()
        .map(|member| NewVirtualRepositoryMember {
            member_repository_id: member.repository_id,
            priority: member.priority as i32,
            enabled: member.enabled,
        })
        .collect();
    DBVirtualRepositoryMember::replace_all(repository_id, &members, database).await?;
    Ok(())
}

fn has_duplicate_members(members: &[VirtualRepositoryMemberConfig]) -> bool {
    let mut seen = HashSet::new();
    members
        .iter()
        .any(|member| !seen.insert(member.repository_id))
}

pub(crate) async fn normalize_virtual_repository_request_value(
    storage_id: Uuid,
    repository_config: &mut Value,
    database: &sqlx::PgPool,
) -> Result<(), String> {
    let Some(config_type) = repository_config.get("type").and_then(Value::as_str) else {
        return Ok(());
    };
    if !config_type.eq_ignore_ascii_case("virtual") {
        return Ok(());
    }

    let Some(config_value) = repository_config.get_mut("config") else {
        return Err("Virtual repository config is missing the config section".to_string());
    };
    let input: VirtualRepositoryConfigInput =
        serde_json::from_value(config_value.clone()).map_err(|err| err.to_string())?;
    let normalized = resolve_virtual_config_input(storage_id, input, database).await?;
    *config_value = serde_json::to_value(normalized).map_err(|err| err.to_string())?;
    Ok(())
}

pub(crate) async fn resolve_virtual_config_input(
    storage_id: Uuid,
    input: VirtualRepositoryConfigInput,
    database: &sqlx::PgPool,
) -> Result<VirtualRepositoryConfig, String> {
    let member_repositories =
        resolve_member_inputs(storage_id, &input.member_repositories, database).await?;
    let publish_to =
        resolve_publish_target(storage_id, input.publish_to.as_ref(), database).await?;
    Ok(VirtualRepositoryConfig {
        member_repositories,
        resolution_order: input.resolution_order,
        cache_ttl_seconds: input.cache_ttl_seconds,
        publish_to,
    })
}

async fn resolve_member_inputs(
    storage_id: Uuid,
    members: &[VirtualRepositoryMemberInput],
    database: &sqlx::PgPool,
) -> Result<Vec<VirtualRepositoryMemberConfig>, String> {
    let mut resolved = Vec::with_capacity(members.len());
    for member in members {
        let repository_id = resolve_member_repository_id(storage_id, member, database).await?;
        resolved.push(VirtualRepositoryMemberConfig {
            repository_id,
            repository_name: member.repository_name.clone(),
            priority: member.priority,
            enabled: member.enabled,
        });
    }
    Ok(resolved)
}

async fn resolve_member_repository_id(
    storage_id: Uuid,
    member: &VirtualRepositoryMemberInput,
    database: &sqlx::PgPool,
) -> Result<Uuid, String> {
    if member.repository_name.trim().is_empty() {
        return Err("repository_name must not be empty".to_string());
    }

    let Some(storage) = DBStorage::get_by_id(storage_id, database)
        .await
        .map_err(|err| err.to_string())?
    else {
        return Err("Invalid Storage".to_string());
    };

    if let Some(repository_id) = member.repository_id {
        let Some(repository) = DBRepository::get_by_id(repository_id, database)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Err(format!("Repository {} not found", repository_id));
        };
        if repository.storage_id != storage_id {
            return Err(format!(
                "Repository {} is not in storage {}",
                repository_id, storage.name
            ));
        }
        if repository.name.as_ref() != member.repository_name {
            return Err(format!(
                "Repository {} name mismatch: expected {}, got {}",
                repository_id, member.repository_name, repository.name
            ));
        }
        return Ok(repository_id);
    }

    let Some(repository_lookup) = DBRepository::get_id_from_storage_and_name(
        storage.name.as_ref(),
        &member.repository_name,
        database,
    )
    .await
    .map_err(|err| err.to_string())?
    else {
        return Err(format!(
            "Repository {} not found in storage {}",
            member.repository_name, storage.name
        ));
    };
    Ok(repository_lookup.repository_id)
}

async fn resolve_publish_target(
    storage_id: Uuid,
    publish_to: Option<&RepositoryReference>,
    database: &sqlx::PgPool,
) -> Result<Option<Uuid>, String> {
    let Some(publish_to) = publish_to else {
        return Ok(None);
    };

    let Some(storage) = DBStorage::get_by_id(storage_id, database)
        .await
        .map_err(|err| err.to_string())?
    else {
        return Err("Invalid Storage".to_string());
    };

    match publish_to {
        RepositoryReference::Id(repository_id) => {
            let Some(repository) = DBRepository::get_by_id(*repository_id, database)
                .await
                .map_err(|err| err.to_string())?
            else {
                return Err(format!("Repository {} not found", repository_id));
            };
            if repository.storage_id != storage_id {
                return Err(format!(
                    "Repository {} is not in storage {}",
                    repository_id, storage.name
                ));
            }
            Ok(Some(*repository_id))
        }
        RepositoryReference::Name(repository_name) => {
            let Some(repository_lookup) = DBRepository::get_id_from_storage_and_name(
                storage.name.as_ref(),
                repository_name,
                database,
            )
            .await
            .map_err(|err| err.to_string())?
            else {
                return Err(format!(
                    "Repository {} not found in storage {}",
                    repository_name, storage.name
                ));
            };
            Ok(Some(repository_lookup.repository_id))
        }
    }
}

async fn resolve_publish_to_name(
    publish_to: Option<Uuid>,
    database: &sqlx::PgPool,
) -> Result<Option<String>, InternalError> {
    let Some(publish_to) = publish_to else {
        return Ok(None);
    };
    let Some(repository) = DBRepository::get_by_id(publish_to, database).await? else {
        return Ok(None);
    };
    Ok(Some(repository.name.to_string()))
}

const fn default_enabled() -> bool {
    true
}

const fn default_cache_ttl_seconds() -> u64 {
    60
}

async fn validate_publish_target(
    repository_type: &str,
    config: &VirtualRepositoryConfig,
    database: &sqlx::PgPool,
) -> Result<(), String> {
    let Some(target) = config.publish_to else {
        return Ok(());
    };

    let Some(member) = config
        .member_repositories
        .iter()
        .find(|member| member.repository_id == target)
    else {
        return Err("publish_to must reference a member repository".to_string());
    };

    if !member.enabled {
        return Err("publish_to must reference an enabled member repository".to_string());
    }

    match repository_type {
        "npm" => {
            let target_config = DBRepositoryConfig::<NPMRegistryConfig>::get_config(
                target,
                NPMRegistryConfigType::get_type_static(),
                database,
            )
            .await
            .map_err(|err| err.to_string())?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            if !matches!(target_config, NPMRegistryConfig::Hosted) {
                return Err(
                    "publish_to must reference an enabled hosted member repository".to_string(),
                );
            }
        }
        "python" => {
            let target_config = DBRepositoryConfig::<PythonRepositoryConfig>::get_config(
                target,
                PythonRepositoryConfigType::get_type_static(),
                database,
            )
            .await
            .map_err(|err| err.to_string())?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            if !matches!(target_config, PythonRepositoryConfig::Hosted) {
                return Err(
                    "publish_to must reference an enabled hosted member repository".to_string(),
                );
            }
        }
        "nuget" => {
            let target_config = DBRepositoryConfig::<NugetRepositoryConfig>::get_config(
                target,
                NugetRepositoryConfigType::get_type_static(),
                database,
            )
            .await
            .map_err(|err| err.to_string())?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            if !matches!(target_config, NugetRepositoryConfig::Hosted) {
                return Err(
                    "publish_to must reference an enabled hosted member repository".to_string(),
                );
            }
        }
        _ => return Err("Repository type does not support virtual publishing".to_string()),
    }

    Ok(())
}

async fn reload_runtime_virtual(site: &Pkgly, repository_id: Uuid) {
    if let Some(DynRepository::NPM(NPMRegistry::Virtual(virtual_repo))) =
        site.get_repository(repository_id)
    {
        if let Err(err) = virtual_repo.reload().await {
            tracing::warn!(repository = %repository_id, error = %err, "Failed to reload virtual repository after config update");
        }
    }

    if let Some(DynRepository::Python(PythonRepository::Virtual(virtual_repo))) =
        site.get_repository(repository_id)
    {
        if let Err(err) = virtual_repo.reload().await {
            tracing::warn!(repository = %repository_id, error = %err, "Failed to reload virtual repository after config update");
        }
    }

    if let Some(DynRepository::Nuget(NugetRepository::Virtual(virtual_repo))) =
        site.get_repository(repository_id)
    {
        if let Err(err) = virtual_repo.reload().await {
            tracing::warn!(repository = %repository_id, error = %err, "Failed to reload virtual repository after config update");
        }
    }
}

#[cfg(test)]
mod tests;
