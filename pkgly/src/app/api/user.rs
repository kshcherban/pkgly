// ABOUTME: Implements user account API routes and session lifecycle responses.
// ABOUTME: Handles login, logout, profile data, password changes, and tokens.
use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Extension, State},
    response::{IntoResponse, Response},
    routing::post,
};
use axum_extra::{
    TypedHeader,
    extract::{
        CookieJar,
        cookie::{Cookie, Expiration, SameSite},
    },
    headers::UserAgent,
};
use http::{StatusCode, header::SET_COOKIE};
use nr_core::{
    database::entities::user::{
        ChangePasswordNoCheck, ChangePasswordWithCheck, User, UserSafeData, UserType,
        auth_token::{AuthTokenRepositoryScope, AuthTokenScope},
        permissions::FullUserPermissions,
        user_utils,
    },
    user::{
        Email,
        token::{AuthTokenFullResponse, AuthTokenResponse},
    },
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::{OpenApi, ToSchema};
mod oauth;
mod password_reset;
mod sso;
mod tokens;
use crate::{
    app::{
        Pkgly,
        authentication::{
            Authentication, MeWithSession,
            password::{self, verify_password},
            session::{Session, SessionError},
            verify_login,
        },
    },
    error::InternalError,
    utils::{
        ResponseBuilder, conflict::ConflictResponse, request_logging::access_log::AccessLogContext,
    },
};
#[derive(OpenApi)]
#[openapi(
    paths(
        me,
        whoami,
        login,
        sso::login,
        oauth::list_providers,
        oauth::authorize,
        oauth::callback,
        get_sessions,
        logout,
        change_email,
        change_password,
        password_reset::request_password_reset,
        password_reset::does_exist,
        password_reset::perform_password_change,
        tokens::create,
        tokens::list,
        tokens::get_token
    ),
    components(schemas(
        UserSafeData,
        MeWithSession,
        Session,
        password_reset::RequestPasswordReset,
        ChangePasswordWithCheck,
        ChangeEmailRequest,
        ChangePasswordNoCheck,
        AuthTokenFullResponse,
        AuthTokenResponse,
        AuthTokenRepositoryScope,
        AuthTokenScope
    ))
)]
pub struct UserAPI;
pub fn user_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/me", axum::routing::get(me))
        .route("/me/permissions", axum::routing::get(me_permissions))
        .route("/change-email", post(change_email))
        .route("/change-password", post(change_password))
        .route("/whoami", axum::routing::get(whoami))
        .route("/login", axum::routing::post(login))
        .route("/sso/login", axum::routing::get(sso::login))
        .route(
            "/oauth2/providers",
            axum::routing::get(oauth::list_providers),
        )
        .route(
            "/oauth2/login/{provider}",
            axum::routing::get(oauth::authorize),
        )
        .route("/oauth2/callback", axum::routing::get(oauth::callback))
        .route("/sessions", axum::routing::get(get_sessions))
        .route("/logout", axum::routing::post(logout))
        .nest("/password-reset", password_reset::password_reset_routes())
        .nest("/token", tokens::token_routes())
}
#[utoipa::path(
    get,
    path = "/me",
    responses(
        (status = 200, description = "List Current User with Session", body = [MeWithSession])
    ),
    security(
        ("session" = [])
    )
)]
#[instrument(skip(auth), fields(user = %auth.id))]
pub async fn me(auth: Authentication) -> Response {
    match auth {
        Authentication::AuthToken(_, _) => plain_response(
            StatusCode::BAD_REQUEST,
            "Use whoami instead of me for Auth Tokens",
        ),
        Authentication::Session(session, user) => {
            let response = Json(MeWithSession::from((session, user)));
            response.into_response()
        }
    }
}
#[utoipa::path(
    get,
    path = "/me/permissions",
    responses(
        (status = 200, description = "Get All the permissions for the current user", body = [FullUserPermissions])
    )
)]
#[instrument(skip(auth, site), fields(user = %auth.id))]
pub async fn me_permissions(
    auth: Authentication,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    let Some(user) = FullUserPermissions::get_by_id(auth.get_id(), site.as_ref()).await? else {
        return Ok(plain_response(
            http::StatusCode::NOT_FOUND,
            "User not found",
        ));
    };
    Ok(Json(user).into_response())
}
#[instrument(skip(auth), fields(user = %auth.id))]
#[utoipa::path(
    get,
    path = "/whoami",
    responses(
        (status = 200, description = "Get current user data", body = UserSafeData)
    ),
    security(
        ("api_key" = []),
        ("session" = [])
    )
)]
pub async fn whoami(auth: Authentication) -> Json<UserSafeData> {
    match auth {
        Authentication::AuthToken(_, user) => Json(user),
        Authentication::Session(_, user) => Json(user),
    }
}
#[utoipa::path(
    get,
    path = "/sessions",
    responses(
        (status = 200, description = "List All Active Sessions", body = [Session])
    )
)]
#[instrument(skip(auth, site), fields(user = %auth.id))]
pub async fn get_sessions(
    auth: Authentication,
    State(site): State<Pkgly>,
) -> Result<Response, SessionError> {
    let sessions = site
        .session_manager
        .filter_table(false, |session| session.user_id == auth.id)?;
    let response = Json(sessions).into_response();
    Ok(response)
}
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct LoginRequest {
    pub email_or_username: String,
    pub password: String,
}

fn plain_response(status: StatusCode, body: impl Into<axum::body::Body>) -> Response {
    ResponseBuilder::default().status(status).body(body)
}

fn empty_response(status: StatusCode) -> Response {
    ResponseBuilder::default().status(status).empty()
}

fn login_success_response(cookie: Cookie<'static>, user_with_session: MeWithSession) -> Response {
    let cookie_value = cookie.encoded().to_string();
    ResponseBuilder::ok()
        .header("Content-Type", "application/json")
        .header(SET_COOKIE, cookie_value)
        .json(&user_with_session)
}

fn session_cookie(session_id: String, is_https: bool) -> Cookie<'static> {
    Cookie::build(("session", session_id))
        .secure(is_https)
        .same_site(session_same_site(is_https))
        .path("/")
        .http_only(true)
        .expires(Expiration::Session)
        .build()
}

fn session_removal_cookie(is_https: bool) -> Cookie<'static> {
    Cookie::build("session")
        .secure(is_https)
        .same_site(session_same_site(is_https))
        .path("/")
        .http_only(true)
        .removal()
        .build()
}

fn session_same_site(is_https: bool) -> SameSite {
    if is_https {
        SameSite::None
    } else {
        SameSite::Lax
    }
}

#[utoipa::path(
    post,
    path = "/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "List All Active Sessions", body = MeWithSession),
        (status = 400, description = "Bad Request. Note: This request requires a User-Agent Header"),
        (status = 401, description = "Unauthorized"),
    )
)]
#[instrument(skip(site, user_agent, addr, login))]
pub async fn login(
    State(site): State<Pkgly>,
    Extension(access_log): Extension<AccessLogContext>,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(login): axum::Json<LoginRequest>,
) -> Result<Response, InternalError> {
    let LoginRequest {
        email_or_username,
        password,
    } = login;
    let user = match verify_login(email_or_username, password, &site.database).await {
        Ok(ok) => ok,
        Err(err) => {
            return Ok(err.into_response());
        }
    };
    access_log.set_user(user.username.as_ref().to_string());
    access_log.set_user_id(user.id);
    let duration = chrono::Duration::days(1);
    let user_agent = user_agent.to_string();
    let ip = addr.ip().to_string();
    let session = site
        .session_manager
        .create_session(user.id, user_agent, ip, duration)?;
    let is_https = site.instance.lock().is_https;
    let cookie = session_cookie(session.session_id.clone(), is_https);
    let user_with_session = MeWithSession::from((session.clone(), user));
    Ok(login_success_response(cookie, user_with_session))
}
#[utoipa::path(
    post,
    path = "/logout",
    responses(
        (status = 204, description = "Successfully Logged Out"),
        (status = 400, description = "Bad Request. Must be a session")
    )
)]
pub async fn logout(
    auth: Authentication,
    State(site): State<Pkgly>,
    cookie: CookieJar,
) -> Result<Response, InternalError> {
    match auth {
        Authentication::AuthToken(_, _) => {
            Ok(plain_response(StatusCode::BAD_REQUEST, "Must be a session"))
        }
        Authentication::Session(session, _) => {
            site.session_manager.delete_session(&session.session_id)?;
            let is_https = site.instance.lock().is_https;
            let empty_session_cookie = session_removal_cookie(is_https);
            let cookies = cookie.add(empty_session_cookie);
            Ok((cookies, StatusCode::NO_CONTENT).into_response())
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ChangeEmailRequest {
    pub email: String,
}

#[utoipa::path(
    post,
    path = "/change-email",
    request_body = ChangeEmailRequest,
    responses(
        (status = 200, description = "Successfully Changed Email", body = UserSafeData),
        (status = 400, description = "Bad Request. Must be a session or email is invalid"),
        (status = 409, description = "Email already in use")
    )
)]
pub async fn change_email(
    auth: Authentication,
    State(site): State<Pkgly>,
    Json(change_email): Json<ChangeEmailRequest>,
) -> Result<Response, InternalError> {
    let Authentication::Session(_, user) = auth else {
        return Ok(plain_response(StatusCode::BAD_REQUEST, "Must be a session"));
    };

    let new_email = match Email::new(change_email.email) {
        Ok(email) => email,
        Err(err) => return Ok(plain_response(StatusCode::BAD_REQUEST, err.to_string())),
    };

    if new_email == user.email {
        return Ok(Json(user).into_response());
    }

    if user_utils::is_email_taken_by_other(new_email.as_ref(), user.id, &site.database).await? {
        return Ok(ConflictResponse::from("email").into_response());
    }

    user.update_email_address(new_email.as_ref(), &site.database)
        .await?;

    let Some(updated_user) = UserSafeData::get_by_id(user.id, &site.database).await? else {
        return Ok(plain_response(StatusCode::NOT_FOUND, "User not found"));
    };

    Ok(Json(updated_user).into_response())
}

#[utoipa::path(
    post,
    path = "/change-password",
    request_body = ChangePasswordWithCheck,
    responses(
        (status = 204, description = "Successfully Changed Password"),
        (status = 400, description = "Bad Request. Must be a session")
    )
)]
pub async fn change_password(
    auth: Authentication,
    State(site): State<Pkgly>,
    Json(change_password): Json<ChangePasswordWithCheck>,
) -> Result<Response, InternalError> {
    let Authentication::Session(_, user) = auth else {
        return Ok(plain_response(StatusCode::BAD_REQUEST, "Must be a session"));
    };
    let ChangePasswordWithCheck {
        old_password,
        new_password,
    } = change_password;
    let Some(user_password) = User::get_password_by_id(user.id, &site.database).await? else {
        return Ok(plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "User password not found",
        ));
    };
    if let Err(err) = verify_password(&old_password, Some(user_password.as_str())) {
        return Ok(err.into_response());
    }
    let Some(new_password) = password::encrypt_password(&new_password) else {
        return Ok(plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to encrypt password",
        ));
    };
    user.update_password(Some(new_password), &site.database)
        .await?;
    Ok(empty_response(StatusCode::NO_CONTENT))
}

#[cfg(test)]
mod tests;
