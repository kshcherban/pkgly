// ABOUTME: Verifies application-level routes that do not require authenticated state.
// ABOUTME: Covers the health response used by Kubernetes liveness probes.
use axum::{Router, extract::OriginalUri, http::Uri, routing::get};
use http::StatusCode;
use tower::ServiceExt;

use super::health;

#[tokio::test]
async fn health_returns_ok() {
    assert_eq!(health().await, StatusCode::OK);
}

async fn request_uri_probe(uri: Uri, OriginalUri(original): OriginalUri) -> String {
    format!("{}|{}", uri.path(), original.path())
}

#[tokio::test]
async fn repository_nesting_preserves_canonical_original_uri() {
    let app = Router::new().nest(
        "/repositories",
        Router::new().route("/{storage}/{repository}/{*path}", get(request_uri_probe)),
    );
    let request = http::Request::get("/repositories/test-storage/python-virtual/simple/demo/")
        .body(axum::body::Body::empty())
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .expect("body")
        .to_bytes();

    assert_eq!(
        body,
        "/test-storage/python-virtual/simple/demo/|/repositories/test-storage/python-virtual/simple/demo/"
    );
}

#[tokio::test]
async fn direct_repository_route_preserves_request_uri() {
    let app = Router::new().route("/{storage}/{repository}/{*path}", get(request_uri_probe));
    let request = http::Request::get("/test-storage/python-virtual/simple/demo/")
        .body(axum::body::Body::empty())
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .expect("body")
        .to_bytes();

    assert_eq!(
        body,
        "/test-storage/python-virtual/simple/demo/|/test-storage/python-virtual/simple/demo/"
    );
}

#[tokio::test]
async fn storage_nesting_strips_only_the_compatibility_prefix() {
    let app = Router::new().nest(
        "/storages",
        Router::new().route("/{storage}/{repository}/{*path}", get(request_uri_probe)),
    );
    let request = http::Request::get("/storages/test-storage/python-virtual/simple/demo/")
        .body(axum::body::Body::empty())
        .expect("request");

    let response = app.oneshot(request).await.expect("response");
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .expect("body")
        .to_bytes();

    assert_eq!(
        body,
        "/test-storage/python-virtual/simple/demo/|/storages/test-storage/python-virtual/simple/demo/"
    );
}
