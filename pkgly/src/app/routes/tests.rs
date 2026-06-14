// ABOUTME: Verifies application-level routes that do not require authenticated state.
// ABOUTME: Covers the health response used by Kubernetes liveness probes.
use http::StatusCode;

use super::health;

#[tokio::test]
async fn health_returns_ok() {
    assert_eq!(health().await, StatusCode::OK);
}
