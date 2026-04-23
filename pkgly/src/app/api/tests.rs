#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use http_body_util::BodyExt;
use nr_core::user::Username;
use serde_json::Value;

#[tokio::test]
async fn scopes_returns_json_list() {
    let response = scopes().await;
    assert_eq!(response.status(), StatusCode::OK);
    let collected = response.into_body().collect().await.unwrap();
    let body = collected.to_bytes();
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert!(parsed.is_array());
}

#[test]
fn install_request_accepts_username_and_password_only() {
    let request: InstallRequest = serde_json::from_value(serde_json::json!({
        "user": {
            "username": "admin",
            "password": "change-me"
        }
    }))
    .expect("install request should deserialize");

    let user = request
        .user
        .into_new_user_request()
        .expect("default first admin email should be valid");

    assert_eq!(user.name, "admin");
    assert_eq!(user.username, Username::new("admin".to_string()).unwrap());
    assert_eq!(user.email.as_ref(), DEFAULT_FIRST_ADMIN_EMAIL);
    assert_eq!(user.password.as_deref(), Some("change-me"));
}
