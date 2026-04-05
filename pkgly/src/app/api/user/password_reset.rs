use std::{net::SocketAddr, str::FromStr};

use axum::{
    Json,
    extract::{ConnectInfo, Extension, Path, State},
    response::Response,
    routing::{get, post},
};
use axum_extra::{
    TypedHeader,
    headers::{Origin, UserAgent},
};
use lettre::Address;
use nr_core::database::entities::user::{
    ChangePasswordNoCheck, User, UserSafeData, UserType,
    password_reset::{RequestDetails, UserPasswordReset},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use utoipa::ToSchema;

use crate::{
    app::{
        Pkgly,
        authentication::password,
        email_service::{Email, EmailDebug, template},
    },
    error::InternalError,
    utils::{ResponseBuilder, request_logging::access_log::AccessLogContext},
};

pub fn password_reset_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/request", post(request_password_reset))
        .route("/check/{token}", get(does_exist))
        .route("/{token}", post(perform_password_change))
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RequestPasswordReset {
    pub email: String,
}
#[derive(Debug, Serialize)]
pub struct PasswordResetEmail {
    pub token: UserPasswordReset,
    pub panel_url: String,
    pub username: String,
    pub required: bool,
}

impl Email for PasswordResetEmail {
    template!("password_reset");

    fn subject() -> &'static str {
        "Password Reset"
    }

    fn debug_info(self) -> EmailDebug {
        EmailDebug {
            to: self.username,
            subject: Self::subject(),
        }
    }
}
#[utoipa::path(
    post,
    path = "/password-reset/request",
    request_body = RequestPasswordReset,
    responses(
        (status = 200, description = "Returns a JSON Schema for the config type")
    ),
)]
async fn request_password_reset(
    State(site): State<Pkgly>,
    Extension(access_log): Extension<AccessLogContext>,
    TypedHeader(origin): TypedHeader<Origin>,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(password_reset): Json<RequestPasswordReset>,
) -> Result<Response, InternalError> {
    let address = match Address::from_str(&password_reset.email) {
        Ok(ok) => ok,
        Err(err) => {
            warn!("Invalid email address: {}", err);
            return Ok(ResponseBuilder::bad_request().empty());
        }
    };
    let request_details = RequestDetails {
        ip_address: addr.ip().to_string(),
        user_agent: user_agent.to_string(),
    };
    let origin = if origin.is_null() {
        return Ok(ResponseBuilder::bad_request().empty());
    } else {
        origin.to_string()
    };
    debug!(?request_details, ?origin, "Requesting password reset");
    let user = User::get_by_email(&password_reset.email, &site.database).await?;
    if let Some(user) = user {
        access_log.set_user(user.username.as_ref().to_string());
        access_log.set_user_id(user.id);
        let token = UserPasswordReset::create(user.id, request_details, &site.database).await?;
        let email: PasswordResetEmail = PasswordResetEmail {
            token,
            panel_url: origin,
            username: user.username.into(),
            required: false,
        };
        site.email_access.send_one_fn(address, email)
    }
    Ok(ResponseBuilder::ok().empty())
}
#[utoipa::path(
    get,
    path = "/password-reset/check/{token}",
    responses(
        (status = 204, description = "Token Exists"),
        (status = 404, description = "Token Does Not Exist")
    ),
)]
async fn does_exist(
    State(site): State<Pkgly>,
    Path(token): Path<String>,
) -> Result<Response, InternalError> {
    let token = UserPasswordReset::does_token_exist_and_valid(&token, &site.database).await?;
    if token {
        Ok(ResponseBuilder::no_content().empty())
    } else {
        Ok(ResponseBuilder::not_found().empty())
    }
}

#[utoipa::path(
    post,
    request_body = ChangePasswordNoCheck,
    path = "/password-reset/{token}",
    responses(
        (status = 204, description = "Password Changed"),
        (status = 404, description = "Token Does Not Exist")
    ),
)]
async fn perform_password_change(
    State(site): State<Pkgly>,
    Extension(access_log): Extension<AccessLogContext>,
    Path(token): Path<String>,
    Json(password_reset): Json<ChangePasswordNoCheck>,
) -> Result<Response, InternalError> {
    let Some(request) = UserPasswordReset::get_if_valid(&token, &site.database).await? else {
        return Ok(ResponseBuilder::not_found().empty());
    };

    let Some(encrypted_password) = password::encrypt_password(&password_reset.password) else {
        return Ok(ResponseBuilder::bad_request().empty());
    };
    let Some(user) = UserSafeData::get_by_id(request.user_id, &site.database).await? else {
        return Ok(ResponseBuilder::not_found().empty());
    };
    access_log.set_user(user.username.as_ref().to_string());
    access_log.set_user_id(user.id);
    user.update_password(Some(encrypted_password), &site.database)
        .await?;

    request.set_used(&site.database).await?;

    Ok(ResponseBuilder::no_content().empty())
}
