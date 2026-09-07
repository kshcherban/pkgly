// ABOUTME: Exposes S3 storage administration endpoints.
// ABOUTME: Returns raw region identifiers for editable provider configuration.
use axum::{
    response::{IntoResponse, Response},
    routing::get,
};
use nr_core::user::permissions::HasPermissions;
use nr_storage::s3::regions::KNOWN_S3_REGIONS;
use tracing::instrument;
use utoipa::OpenApi;

use crate::{
    app::{Pkgly, authentication::Authentication, responses::MissingPermission},
    error::InternalError,
    utils::ResponseBuilder,
};

#[derive(OpenApi)]
#[openapi(paths(region_list))]
pub struct S3StorageAPI;
pub fn s3_storage_api() -> axum::Router<Pkgly> {
    axum::Router::new().route("/regions", get(region_list))
}

#[utoipa::path(
    get,
    path = "/regions",
    responses(
        (status = 200, description = "A list of suggested raw region identifiers for S3 storage", body = Vec<String>)
    )
)]
#[instrument(skip(auth), fields(user = %auth.id))]
pub async fn region_list(auth: Authentication) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(MissingPermission::StorageManager.into_response());
    }
    let regions: Vec<String> = KNOWN_S3_REGIONS
        .iter()
        .map(|region| (*region).to_owned())
        .collect();
    Ok(ResponseBuilder::ok().json(&regions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::Response;
    use chrono::DateTime;
    use http_body_util::BodyExt;
    use nr_core::{
        database::entities::user::{UserSafeData, auth_token::AuthToken},
        user::{Email, Username},
    };

    fn admin_auth() -> Authentication {
        let timestamp = DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").unwrap();
        let user = UserSafeData {
            id: 1,
            name: "Admin".into(),
            username: Username::new("admin".into()).unwrap(),
            email: Some(Email::new("admin@example.com".into()).unwrap()),
            require_password_change: false,
            active: true,
            admin: true,
            user_manager: true,
            system_manager: true,
            default_repository_actions: vec![],
            updated_at: timestamp,
            created_at: timestamp,
        };
        let token = AuthToken {
            id: 1,
            user_id: 1,
            name: Some("test".into()),
            description: None,
            token: "token".into(),
            active: true,
            source: "test".into(),
            expires_at: None,
            created_at: timestamp,
        };
        Authentication::AuthToken(token, user)
    }

    #[tokio::test]
    async fn region_list_returns_raw_identifiers() {
        let response: Response = region_list(admin_auth()).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let regions: Vec<String> = serde_json::from_slice(&body).unwrap();
        assert!(regions.iter().any(|region| region == "us-east-1"));
        assert!(!regions.iter().any(|region| region == "UsEast1"));
    }
}
