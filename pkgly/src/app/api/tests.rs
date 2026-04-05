#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use http_body_util::BodyExt;
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
