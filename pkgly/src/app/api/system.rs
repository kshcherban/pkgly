use axum::{
    Json,
    extract::{Path, State},
    response::Response,
    routing::get,
};
use nr_core::user::permissions::HasPermissions;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

use crate::{
    app::{
        Pkgly,
        authentication::Authentication,
        webhooks::{
            self, UpsertWebhookInput, WebhookDeliveryStatus, WebhookEventType, WebhookHeaderInput,
            WebhookSummary,
        },
    },
    error::{InternalError, OtherInternalError},
    utils::ResponseBuilder,
};
use tracing::{error, instrument};

#[derive(OpenApi)]
#[openapi(
    paths(
        list_webhooks,
        create_webhook,
        get_webhook,
        update_webhook,
        delete_webhook
    ),
    components(schemas(
        WebhookEventType,
        WebhookDeliveryStatus,
        WebhookHeaderResponse,
        WebhookHeaderRequest,
        WebhookResponse,
        WebhookUpsertRequest
    ))
)]
pub struct SystemAPI;

pub fn system_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/webhooks", get(list_webhooks).post(create_webhook))
        .route(
            "/webhooks/{id}",
            get(get_webhook).put(update_webhook).delete(delete_webhook),
        )
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebhookHeaderResponse {
    pub name: String,
    pub configured: bool,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct WebhookHeaderRequest {
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WebhookResponse {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub target_url: String,
    pub events: Vec<WebhookEventType>,
    pub headers: Vec<WebhookHeaderResponse>,
    pub last_delivery_status: Option<WebhookDeliveryStatus>,
    pub last_delivery_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_http_status: Option<i32>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct WebhookUpsertRequest {
    pub name: String,
    pub enabled: bool,
    pub target_url: String,
    pub events: Vec<WebhookEventType>,
    #[serde(default)]
    pub headers: Vec<WebhookHeaderRequest>,
}

impl From<WebhookSummary> for WebhookResponse {
    fn from(value: WebhookSummary) -> Self {
        Self {
            id: value.id,
            name: value.name,
            enabled: value.enabled,
            target_url: value.target_url,
            events: value.events,
            headers: value
                .headers
                .into_iter()
                .map(|header| WebhookHeaderResponse {
                    name: header.name,
                    configured: header.configured,
                })
                .collect(),
            last_delivery_status: value.last_delivery_status,
            last_delivery_at: value.last_delivery_at,
            last_http_status: value.last_http_status,
            last_error: value.last_error,
        }
    }
}

impl From<WebhookUpsertRequest> for UpsertWebhookInput {
    fn from(value: WebhookUpsertRequest) -> Self {
        Self {
            name: value.name,
            enabled: value.enabled,
            target_url: value.target_url,
            events: value.events,
            headers: value
                .headers
                .into_iter()
                .map(|header| WebhookHeaderInput {
                    name: header.name,
                    value: header.value,
                    configured: header.configured,
                })
                .collect(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/webhooks",
    tag = "system",
    responses((status = 200, description = "List configured webhooks", body = [WebhookResponse])),
    security(("session" = []))
)]
#[instrument(skip(auth, site), fields(project_module = "System"))]
pub async fn list_webhooks(
    auth: Authentication,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    let webhooks = webhooks::list_webhooks(&site.database)
        .await
        .map_err(|err| {
            InternalError::from(OtherInternalError::new(std::io::Error::other(
                err.to_string(),
            )))
        })?;
    let response = webhooks
        .into_iter()
        .map(WebhookResponse::from)
        .collect::<Vec<_>>();
    Ok(ResponseBuilder::ok().json(&response))
}

#[utoipa::path(
    post,
    path = "/webhooks",
    tag = "system",
    request_body = WebhookUpsertRequest,
    responses((status = 201, description = "Created webhook", body = WebhookResponse)),
    security(("session" = []))
)]
#[instrument(skip(auth, site, request), fields(project_module = "System"))]
pub async fn create_webhook(
    auth: Authentication,
    State(site): State<Pkgly>,
    Json(request): Json<WebhookUpsertRequest>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    match webhooks::create_webhook(&site.database, request.into()).await {
        Ok(webhook) => Ok(ResponseBuilder::created().json(&WebhookResponse::from(webhook))),
        Err(err) => {
            error!(%err, "Failed to create webhook");
            Ok(ResponseBuilder::bad_request().body(err.to_string()))
        }
    }
}

#[utoipa::path(
    get,
    path = "/webhooks/{id}",
    tag = "system",
    params(("id" = Uuid, Path, description = "Webhook identifier")),
    responses(
        (status = 200, description = "Webhook details", body = WebhookResponse),
        (status = 404, description = "Webhook not found")
    ),
    security(("session" = []))
)]
#[instrument(skip(auth, site), fields(project_module = "System", webhook_id = %id))]
pub async fn get_webhook(
    auth: Authentication,
    State(site): State<Pkgly>,
    Path(id): Path<Uuid>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    let webhook = webhooks::get_webhook(&site.database, id)
        .await
        .map_err(|err| {
            InternalError::from(OtherInternalError::new(std::io::Error::other(
                err.to_string(),
            )))
        })?;
    let Some(webhook) = webhook else {
        return Ok(ResponseBuilder::not_found().body("Webhook not found"));
    };
    Ok(ResponseBuilder::ok().json(&WebhookResponse::from(webhook)))
}

#[utoipa::path(
    put,
    path = "/webhooks/{id}",
    tag = "system",
    params(("id" = Uuid, Path, description = "Webhook identifier")),
    request_body = WebhookUpsertRequest,
    responses(
        (status = 200, description = "Updated webhook", body = WebhookResponse),
        (status = 404, description = "Webhook not found")
    ),
    security(("session" = []))
)]
#[instrument(skip(auth, site, request), fields(project_module = "System", webhook_id = %id))]
pub async fn update_webhook(
    auth: Authentication,
    State(site): State<Pkgly>,
    Path(id): Path<Uuid>,
    Json(request): Json<WebhookUpsertRequest>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    match webhooks::update_webhook(&site.database, id, request.into()).await {
        Ok(Some(webhook)) => Ok(ResponseBuilder::ok().json(&WebhookResponse::from(webhook))),
        Ok(None) => Ok(ResponseBuilder::not_found().body("Webhook not found")),
        Err(err) => {
            error!(%err, "Failed to update webhook");
            Ok(ResponseBuilder::bad_request().body(err.to_string()))
        }
    }
}

#[utoipa::path(
    delete,
    path = "/webhooks/{id}",
    tag = "system",
    params(("id" = Uuid, Path, description = "Webhook identifier")),
    responses(
        (status = 204, description = "Deleted webhook"),
        (status = 404, description = "Webhook not found")
    ),
    security(("session" = []))
)]
#[instrument(skip(auth, site), fields(project_module = "System", webhook_id = %id))]
pub async fn delete_webhook(
    auth: Authentication,
    State(site): State<Pkgly>,
    Path(id): Path<Uuid>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    let deleted = webhooks::delete_webhook(&site.database, id)
        .await
        .map_err(|err| {
            InternalError::from(OtherInternalError::new(std::io::Error::other(
                err.to_string(),
            )))
        })?;
    if !deleted {
        return Ok(ResponseBuilder::not_found().body("Webhook not found"));
    }

    Ok(ResponseBuilder::no_content().empty())
}

#[cfg(test)]
mod tests;
