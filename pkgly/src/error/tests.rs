#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use axum::response::IntoResponse;

#[test]
fn illegal_state_error_uses_internal_server_error_status() {
    let err = IllegalStateError("test illegal state");
    let response = err.into_response();
    assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn internal_error_wraps_other_internal_error() {
    use std::io;

    let io_err = io::Error::new(io::ErrorKind::Other, "boom");
    let other = OtherInternalError::new(io_err);
    let internal: InternalError = InternalError::from(other);

    let response = internal.into_response();
    assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
}
