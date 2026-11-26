#![allow(clippy::unwrap_used)]

use super::*;
use http::{Request, header::HOST};

#[test]
fn public_base_prefers_the_configured_url() -> Result<(), NPMRegistryError> {
    let (parts, ()) = Request::builder()
        .header(HOST, "internal.test:6742")
        .header("x-forwarded-proto", "http")
        .body(())
        .unwrap()
        .into_parts();

    let base = public_base("https://registry.test/pkgly/", false, &parts)?;
    assert_eq!(base.as_str(), "https://registry.test/pkgly/");
    Ok(())
}

#[test]
fn public_base_uses_the_forwarded_scheme_when_unconfigured() -> Result<(), NPMRegistryError> {
    let (parts, ()) = Request::builder()
        .uri("http://registry.test:8443/repositories/local/npm")
        .header("x-forwarded-proto", "https")
        .body(())
        .unwrap()
        .into_parts();

    let base = public_base("", false, &parts)?;
    assert_eq!(base.as_str(), "https://registry.test:8443/");
    Ok(())
}

#[test]
fn public_base_rejects_a_host_with_credentials() {
    let (mut parts, ()) = Request::new(()).into_parts();
    parts
        .headers
        .insert(HOST, http::HeaderValue::from_static("user@registry.test"));
    assert!(matches!(
        public_base("", false, &parts),
        Err(NPMRegistryError::InvalidTarballBase(
            "request host contains user information"
        ))
    ));
}

#[test]
fn tarball_url_keeps_the_scope_and_full_prerelease_version() -> Result<(), NPMRegistryError> {
    let base = Url::parse("https://registry.test/pkgly/repositories/local/npm").unwrap();
    let url = tarball_url(base, "@scope/demo", "1.0.0-beta.1+Build")?;
    assert_eq!(
        url,
        "https://registry.test/pkgly/repositories/local/npm/@scope/demo/-/demo-1.0.0-beta.1+Build.tgz"
    );
    Ok(())
}

#[test]
fn public_base_requires_a_host_when_unconfigured() {
    let (parts, ()) = Request::new(()).into_parts();
    assert!(matches!(
        public_base("", false, &parts),
        Err(NPMRegistryError::MissingTarballHost)
    ));
}

#[tokio::test]
async fn invalid_app_url_details_stay_out_of_the_response() {
    let (parts, ()) = Request::new(()).into_parts();
    let error = public_base("invalid", false, &parts).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Configured npm tarball app_url is invalid"
    );
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), 128)
        .await
        .unwrap();
    assert_eq!(&body[..], b"Could not build the tarball URL");
}
