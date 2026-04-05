use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use couch_db::CouchDBLoginResponse;
use derive_more::derive::From;
use http::StatusCode;
use nr_core::storage::StoragePath;

use crate::repository::RepoResponse;
pub mod couch_db;
pub mod web_login;

/// Returns true if the path corresponds to an npm login endpoint that should accept
/// credential-bearing requests without prior authentication (CouchDB style or web login).
pub fn is_npm_login_path(path: &StoragePath) -> bool {
    let path = path.to_string();
    path.starts_with("-/user/org.couchdb.user:") || path == "-/v1/login"
}

#[derive(Debug, From)]
pub enum LoginResponse {
    ValidCouchDBLogin(CouchDBLoginResponse),
    UnsupportedLogin,
}

impl IntoResponse for LoginResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            LoginResponse::ValidCouchDBLogin(login) => match serde_json::to_string(&login) {
                Ok(body) => Response::builder()
                    .status(StatusCode::CREATED)
                    .body(body.into())
                    .unwrap_or_default(),
                Err(_) => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap_or_default(),
            },
            LoginResponse::UnsupportedLogin => Response::builder()
                .status(StatusCode::IM_A_TEAPOT)
                .body("Unsupported Login Type".into())
                .unwrap_or_default(),
        }
    }
}
impl From<LoginResponse> for RepoResponse {
    fn from(value: LoginResponse) -> Self {
        RepoResponse::Other(value.into_response())
    }
}
