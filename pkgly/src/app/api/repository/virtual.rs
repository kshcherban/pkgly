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
    repository::config::RepositoryConfigType,
    user::permissions::{HasPermissions, RepositoryActions},
};
use serde::{Deserialize, Serialize};
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
    pub members: Vec<VirtualMemberView>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMembersRequest {
    pub members: Vec<VirtualRepositoryMemberConfig>,
    #[serde(default)]
    pub resolution_order: Option<VirtualResolutionOrder>,
    #[serde(default)]
    pub cache_ttl_seconds: Option<u64>,
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub publish_to: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateResolutionOrderRequest {
    pub resolution_order: VirtualResolutionOrder,
    #[serde(default)]
    pub cache_ttl_seconds: Option<u64>,
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub publish_to: Option<Uuid>,
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
        _ => return Ok(ResponseBuilder::bad_request().body("Repository is not NPM, Python, or NuGet")),
    };

    let members = DBVirtualRepositoryMember::list_for_virtual(repository_id, site.as_ref()).await?;
    let views = hydrate_members(&members, &site).await?;

    let view = VirtualConfigView {
        resolution_order: virtual_config.resolution_order,
        cache_ttl_seconds: virtual_config.cache_ttl_seconds,
        publish_to: virtual_config.publish_to,
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
        _ => return Ok(ResponseBuilder::bad_request().body("Repository is not NPM, Python, or NuGet")),
    };

    if payload.members.is_empty() {
        return Ok(ResponseBuilder::bad_request().body("Member list cannot be empty"));
    }
    if has_duplicate_members(&payload.members) {
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
        config.publish_to = Some(publish_to);
    }

    config.member_repositories = payload.members;

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
        _ => return Ok(ResponseBuilder::bad_request().body("Repository is not NPM, Python, or NuGet")),
    };

    config.resolution_order = payload.resolution_order;
    if let Some(ttl) = payload.cache_ttl_seconds {
        config.cache_ttl_seconds = ttl.max(1);
    }
    config.publish_to = payload.publish_to.or(config.publish_to);

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
