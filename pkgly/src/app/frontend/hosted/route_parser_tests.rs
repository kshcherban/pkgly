// ABOUTME: Tests frontend route parsing and browser navigation matching.
// ABOUTME: Keeps backend SPA refresh behavior aligned with generated routes.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::app::{config::Mode, state::Instance};
use axum::body::Body;
use http::{Method, Request, header::ACCEPT};
use semver::Version;
#[test]
pub fn basic_test() {
    let route = FrontendRoute::try_from("/test/:id".to_string()).unwrap();
    println!("{:?}", route);
    assert_eq!(route.parts.len(), 2);
    assert_eq!(
        route.parts[0],
        FrontendRouteComponent::String("test".to_string())
    );
    assert_eq!(
        route.parts[1],
        FrontendRouteComponent::Param {
            key: "id".to_string(),
            optional: false,
            catch_all: false
        }
    );

    assert!(route.matches_path("/test/123"));
    assert!(route.matches_path("/test/123/"));

    assert!(!route.matches_path("/test/"));
}
#[test]
pub fn browse_test() {
    let route = FrontendRoute::try_from("/browse/:id/:catchAll(.*)?".to_string()).unwrap();
    println!("{:?}", route);
    assert_eq!(route.parts.len(), 3);
    assert_eq!(
        route.parts[0],
        FrontendRouteComponent::String("browse".to_string())
    );
    assert_eq!(
        route.parts[1],
        FrontendRouteComponent::Param {
            key: "id".to_string(),
            optional: false,
            catch_all: false
        }
    );
    assert_eq!(
        route.parts[2],
        FrontendRouteComponent::Param {
            key: "catchAll".to_string(),
            optional: true,
            catch_all: true
        }
    );
    assert!(route.matches_path("/browse/123/456"));
    assert!(route.matches_path("/browse/123/456/"));
    assert!(route.matches_path("/browse/123/456/789"));
    assert!(route.matches_path("/browse/123/456/789/"));
    assert!(!route.matches_path("/not_browse/"));
}

#[test]
fn parse_all() {
    let file = include_str!("../../../../../site/src/router/routes.json");
    let routes: Vec<RouteItem> = serde_json::from_str(file).unwrap();

    for route in routes {
        println!("{:?}", route);
    }
}

fn request(method: Method, path: &str, accept: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(accept) = accept {
        builder = builder.header(ACCEPT, accept);
    }
    builder.body(Body::empty()).unwrap()
}

fn routes() -> Vec<RouteItem> {
    serde_json::from_str(include_str!("../../../../../site/src/router/routes.json")).unwrap()
}

#[test]
fn spa_navigation_detection_requires_html_get_or_head_for_known_route() {
    let routes = routes();

    assert!(is_spa_navigation_request(
        &request(Method::GET, "/page/repository/123", Some("text/html")),
        &routes,
    ));
    assert!(is_spa_navigation_request(
        &request(
            Method::HEAD,
            "/admin/system/webhooks",
            Some("application/xhtml+xml,text/html;q=0.9"),
        ),
        &routes,
    ));
}

#[test]
fn spa_navigation_detection_rejects_package_and_static_requests() {
    let routes = routes();

    assert!(!is_spa_navigation_request(
        &request(Method::POST, "/page/repository/123", Some("text/html")),
        &routes,
    ));
    assert!(!is_spa_navigation_request(
        &request(
            Method::GET,
            "/page/repository/123",
            Some("application/json")
        ),
        &routes,
    ));
    assert!(!is_spa_navigation_request(
        &request(Method::GET, "/page/repository/123", None),
        &routes,
    ));
    assert!(!is_spa_navigation_request(
        &request(Method::GET, "/assets/app.js", Some("text/html")),
        &routes,
    ));
}

#[test]
fn routes_json_covers_refreshable_pages() {
    let routes = routes();
    for path in [
        "/page/repository/123",
        "/admin/repository/123",
        "/admin/user/123",
        "/admin/system/sso",
        "/admin/system/webhooks",
        "/browse/123/packages/pkg",
        "/projects/123",
        "/projects/123/1.0.0",
        "/projects/storage/repository/package",
        "/projects/storage/repository/package/1.0.0",
    ] {
        assert!(
            routes.iter().any(|route| route.path.matches_path(path)),
            "{path} should match a frontend route"
        );
    }
}

fn instance_with_app_url(app_url: &str) -> Instance {
    Instance {
        app_url: app_url.to_string(),
        name: "Pkgly".to_string(),
        description: "An Open Source artifact manager.".to_string(),
        is_https: false,
        is_installed: true,
        version: Version::new(0, 0, 0),
        mode: Mode::Debug,
        password_rules: None,
        sso: None,
        oauth2: None,
    }
}

#[test]
fn index_template_data_uses_root_when_app_url_is_blank() {
    let rendered = index_template_data(instance_with_app_url(""));

    assert_eq!(rendered.app_url, "/");
}

#[test]
fn index_template_data_keeps_configured_app_url() {
    let rendered = index_template_data(instance_with_app_url("https://repo.pkgly.dev/pkgly/"));

    assert_eq!(rendered.app_url, "https://repo.pkgly.dev/pkgly/");
}
