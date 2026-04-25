use axum::{
    Json,
    extract::{Path, State},
    response::Response,
    routing::{delete, get, post},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use chrono::{DateTime, FixedOffset, Local};
use nr_core::{
    database::entities::user::{
        UserType,
        auth_token::{AuthToken, NewAuthToken},
    },
    user::{permissions::RepositoryActions, scopes::NRScope, token::AuthTokenFullResponse},
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::{Pkgly, authentication::OnlySessionAllowedAuthentication},
    error::InternalError,
    utils::ResponseBuilder,
};

pub fn token_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/create", post(create))
        .route("/list", get(list))
        .route("/get/{id}", get(get_token))
        .route("/delete/{id}", delete(delete_token))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewAuthTokenRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub expires_in_days: Option<serde_json::Value>,
    #[serde(default)]
    pub scopes: Vec<NRScope>,
    #[serde(default)]
    pub repository_scopes: Vec<NewRepositoryScope>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewRepositoryScope {
    pub repository_id: Uuid,
    pub scopes: Vec<RepositoryActions>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewAuthTokenResponse {
    pub id: i32,
    pub token: String,
    pub expires_at: Option<DateTime<FixedOffset>>,
}
#[utoipa::path(
    post,
    path = "/token/create",
    //request_body = NewAuthToken,
    responses(
        (status = 200, description = "A New Auth Token was created"),
    ),
)]
pub(super) async fn create(
    auth: OnlySessionAllowedAuthentication,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    State(site): State<Pkgly>,
    Json(new_token): Json<NewAuthTokenRequest>,
) -> Result<Response, InternalError> {
    let source = format!("API Request ({})", user_agent);
    if new_token.repository_scopes.is_empty() && new_token.scopes.is_empty() {
        return Ok(ResponseBuilder::bad_request().body("No Scopes Provided"));
    }
    let expires_at = match expires_at_from_days(
        new_token.expires_in_days.as_ref(),
        Local::now().fixed_offset(),
    ) {
        Ok(expires_at) => expires_at,
        Err(()) => return Ok(ResponseBuilder::bad_request().body("Invalid expires_in_days")),
    };
    let repositories: Vec<(Uuid, Vec<RepositoryActions>)> = new_token
        .repository_scopes
        .into_iter()
        .map(|scope| (scope.repository_id, scope.scopes))
        .collect();
    let new_token = NewAuthToken {
        user_id: auth.get_id(),
        name: new_token.name,
        description: new_token.description,
        source,
        expires_at,
        scopes: new_token.scopes,
        repositories,
    };
    let (id, token) = new_token.insert(site.as_ref()).await?;
    let response = NewAuthTokenResponse {
        id,
        token,
        expires_at,
    };

    Ok(ResponseBuilder::ok().json(&response))
}

pub(super) fn expires_at_from_days(
    expires_in_days: Option<&serde_json::Value>,
    now: DateTime<FixedOffset>,
) -> Result<Option<DateTime<FixedOffset>>, ()> {
    let Some(value) = expires_in_days else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(days) = value.as_i64() else {
        return Err(());
    };
    if days <= 0 {
        return Err(());
    }
    let duration = chrono::Duration::try_days(days).ok_or(())?;
    now.checked_add_signed(duration).ok_or(()).map(Some)
}
#[utoipa::path(
    get,
    path = "/token/list",
    responses(
        (status = 200, description = "A New Auth Token was created", body=[AuthTokenFullResponse]),
    ),
)]
#[instrument(skip(auth, site), fields(user = %auth.get_id()))]
async fn list(
    auth: OnlySessionAllowedAuthentication,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    let tokens = AuthTokenFullResponse::get_all_for_user(auth.get_id(), site.as_ref()).await?;

    Ok(ResponseBuilder::ok().json(&tokens))
}
#[utoipa::path(
    get,
    path = "/token/get/{id}",
    responses(
        (status = 200, description = "A New Auth Token was created", body=AuthTokenFullResponse),
    ),
)]
#[instrument(skip(auth, site), fields(user = %auth.get_id(), token_id = %id))]

async fn get_token(
    auth: OnlySessionAllowedAuthentication,
    Path(id): Path<i32>,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    let tokens =
        AuthTokenFullResponse::find_by_id_and_user_id(id, auth.get_id(), site.as_ref()).await?;
    Ok(ResponseBuilder::ok().json(&tokens))
}
#[utoipa::path(
    delete,
    path = "/token/delete/{id}",
    responses(
        (status = 200, description = "Token Deleted"),
    ),
)]
#[instrument(skip(auth, site), fields(user = %auth.get_id(), token_id = %id))]
async fn delete_token(
    auth: OnlySessionAllowedAuthentication,
    Path(id): Path<i32>,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    let Some(token) = AuthToken::get_by_id_and_user_id(id, auth.get_id(), site.as_ref()).await?
    else {
        return Ok(ResponseBuilder::not_found().empty());
    };
    token.delete(site.as_ref()).await?;
    Ok(ResponseBuilder::no_content().empty())
}
