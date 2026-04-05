use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use tracing::instrument;

use crate::{
    app::{Pkgly, authentication::Authentication},
    error::InternalError,
    utils::ResponseBuilder,
};
pub fn config_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/config/{key}/schema", axum::routing::get(config_schema))
        .route(
            "/config/{key}/validate",
            axum::routing::post(config_validate),
        )
        .route("/config/{key}/default", axum::routing::get(config_default))
        .route(
            "/config/{key}/description",
            axum::routing::get(config_description),
        )
}
pub struct InvalidConfigType(String);
impl IntoResponse for InvalidConfigType {
    fn into_response(self) -> Response {
        ResponseBuilder::bad_request().body(format!("Invalid Config Type: {}", self.0))
    }
}

#[utoipa::path(
    get,
    summary = "Get Config Schema",
    path = "/config/{key}/schema",
    responses(
        (status = 200, description = "Returns a JSON Schema for the config type")
    ),
    params(
        ("key" = String, Path, description = "Config Key"),
    ),
)]
#[instrument(skip(site), fields(config_key = %key))]
pub async fn config_schema(
    State(site): State<Pkgly>,
    Path(key): Path<String>,
) -> Result<Response, InternalError> {
    // TODO: Add Client side caching

    let Some(config_type) = site.get_repository_config_type(&key) else {
        return Ok(InvalidConfigType(key).into_response());
    };

    let response = match config_type.schema() {
        Some(schema) => Json(schema).into_response(),
        None => ResponseBuilder::not_found().body("No schema found"),
    };
    Ok(response)
}
/// Requires Authentication to prevent abuse
#[utoipa::path(
    post,
    request_body = Value,
    summary = "Validate a Config",
    path = "/config/{key}/validate",
    responses(
        (status = 200, description = "Returns a JSON Schema for the config type")
    ),
    params(
        ("key" = String, Path, description = "Config Key"),
    ),
)]
#[instrument(skip(site, auth, config), fields(user = %auth.id, config_key = %key))]
pub async fn config_validate(
    State(site): State<Pkgly>,
    Path(key): Path<String>,
    auth: Authentication,
    Json(config): Json<serde_json::Value>,
) -> Result<Response, InternalError> {
    //TODO: Check permissions
    let Some(config_type) = site.get_repository_config_type(&key) else {
        return Ok(InvalidConfigType(key).into_response());
    };

    let response = match config_type.validate_config(config) {
        Ok(_) => ResponseBuilder::no_content().empty(),
        Err(err) => ResponseBuilder::bad_request().body(err.to_string()),
    };
    Ok(response)
}
#[utoipa::path(
    get,
    summary = "Get Default Config",
    path = "/config/{key}/default",
    responses(
        (status = 200, description = "Returns the default config for the config type"),
    ),
    params(
        ("key" = String, Path, description = "Config Key"),
    ),
)]
#[instrument(skip(site), fields(config_key = %key))]
pub async fn config_default(
    State(site): State<Pkgly>,
    Path(key): Path<String>,
) -> Result<Response, InternalError> {
    // TODO: Add Client side caching
    let Some(config_type) = site.get_repository_config_type(&key) else {
        return Ok(InvalidConfigType(key).into_response());
    };

    match config_type.default() {
        Ok(ok) => Ok(ResponseBuilder::ok().json(&ok)),
        Err(err) => Ok(ResponseBuilder::internal_server_error().body(err.to_string())),
    }
}
#[utoipa::path(
    get,
    path = "/config/{key}/description",
    summary = "Get Config Description",
    responses(
        (status = 200, description = "Returns the description for the config type"),
    ),
    params(
        ("key" = String, Path, description = "Config Key"),
    ),
)]
pub async fn config_description(
    State(site): State<Pkgly>,
    Path(key): Path<String>,
) -> Result<Response, InternalError> {
    let Some(config_type) = site.get_repository_config_type(&key) else {
        return Ok(InvalidConfigType(key).into_response());
    };

    let description = config_type.get_description();
    Ok(ResponseBuilder::ok().json(&description))
}
