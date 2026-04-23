use axum::{
    Json,
    extract::{Extension, Request, State},
    response::Response,
};
use http::{StatusCode, Uri};
use nr_core::{
    database::entities::user::NewUserRequest,
    user::{
        Email, Username,
        scopes::{NRScope, ScopeDescription},
    },
};
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use strum::IntoEnumIterator;
use tower_http::cors::CorsLayer;
use tracing::{error, instrument};
use utoipa::ToSchema;
pub mod artipie;
pub mod project;
pub mod repository;
pub mod search;
pub mod security;
pub mod storage;
pub mod system;
pub mod user;
pub mod user_management;
use super::{Instance, Pkgly, PkglyState, authentication::password};
use crate::{
    error::InternalError,
    utils::{
        ResponseBuilder, api_error_response::APIErrorResponse,
        request_logging::access_log::AccessLogContext,
    },
};
pub fn api_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/info", axum::routing::get(info))
        .route("/info/scopes", axum::routing::get(scopes))
        .route("/install", axum::routing::post(install))
        .nest("/user", user::user_routes())
        .nest("/storage", storage::storage_routes())
        .nest(
            "/user-management",
            user_management::user_management_routes(),
        )
        .nest("/repository", repository::repository_routes())
        .nest("/search", search::search_routes())
        .nest("/security", security::security_routes())
        .nest("/system", system::system_routes())
        .nest("/project", project::project_routes())
        .merge(artipie::routes())
        .fallback(route_not_found)
        .layer(CorsLayer::very_permissive())
}
#[utoipa::path(
    get,
    path = "/api/info",
    responses(
        (status = 200, description = "information about the Site", body = Instance)
    )
)]
#[instrument(skip(site))]
pub async fn info(State(site): PkglyState) -> Json<Instance> {
    let site = site.instance.lock().clone();
    Json(site)
}
#[utoipa::path(
    get,
    path = "/api/info/scopes",
    responses(
        (status = 200, description = "List of all the scopes", body = [ScopeDescription])
    )
)]
pub async fn scopes() -> Response {
    let scopes = NRScope::iter()
        .map(|scope| scope.description())
        .collect::<Vec<_>>();
    ResponseBuilder::ok()
        .header("Content-Type", "application/json")
        .json(&scopes)
}

const DEFAULT_FIRST_ADMIN_EMAIL: &str = "admin@pkgly.local";

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct InstallRequest {
    pub user: InstallUserRequest,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct InstallUserRequest {
    pub username: Username,
    pub password: Option<String>,
}

impl InstallUserRequest {
    fn into_new_user_request(self) -> Result<NewUserRequest, nr_core::user::InvalidEmail> {
        Ok(NewUserRequest {
            name: self.username.as_ref().to_string(),
            username: self.username,
            email: Email::new(DEFAULT_FIRST_ADMIN_EMAIL.to_string())?,
            password: self.password,
        })
    }
}
/// Installs the site with the first user. If Site is already installed, it will return a 404.
#[utoipa::path(
    post,
    request_body = InstallRequest,
    path = "/api/install",
    responses(
        (status = 204, description = "Site is now installed"),
        (status = 404, description = "Site is already installed"),
    )
)]
#[instrument(skip(site, request))]
pub async fn install(
    State(site): PkglyState,
    Extension(access_log): Extension<AccessLogContext>,
    Json(request): Json<InstallRequest>,
) -> Result<StatusCode, InternalError> {
    {
        let instance = site.instance.lock();
        if instance.is_installed {
            return Ok(StatusCode::NOT_FOUND);
        }
    }
    let InstallRequest { user } = request;
    let mut user = match user.into_new_user_request() {
        Ok(user) => user,
        Err(err) => {
            error!(?err, "Failed to create default email for first admin.");
            return Ok(StatusCode::BAD_REQUEST);
        }
    };
    let password = user
        .password
        .as_ref()
        .and_then(|password| password::encrypt_password(password));
    if password.is_none() {
        error!("A Password must exist for the first user.");
        return Ok(StatusCode::BAD_REQUEST);
    }
    user.password = password;
    let created_user = user.insert_admin(&site.database).await?;
    access_log.set_user(created_user.username.as_ref().to_string());
    access_log.set_user_id(created_user.id);
    {
        let mut instance = site.instance.lock();
        instance.is_installed = true;
    }
    return Ok(StatusCode::NO_CONTENT);
}

#[derive(Debug)]
pub struct RouteNotFound {
    pub uri: Uri,
    pub method: http::Method,
}
impl Serialize for RouteNotFound {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut struct_ser = serializer.serialize_struct("RouteNotFound", 3)?;
        struct_ser.serialize_field("uri", &self.uri.to_string())?;
        struct_ser.serialize_field("path", &self.uri.path())?;
        struct_ser.serialize_field("method", &self.method.to_string())?;
        struct_ser.end()
    }
}
/// `/api/*` fall back is different than the rest of the site
async fn route_not_found(request: Request) -> Response {
    let response: APIErrorResponse<RouteNotFound, ()> = APIErrorResponse {
        message: "Not Found".into(),
        details: Some(RouteNotFound {
            uri: request.uri().clone(),
            method: request.method().clone(),
        }),
        ..Default::default()
    };
    ResponseBuilder::not_found()
        .error_reason("Route not found")
        .json(&response)
}

#[cfg(test)]
mod tests;
