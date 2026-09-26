#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use super::proxy::{DEFAULT_ROUTE, normalize_routes};

#[test]
fn normalize_routes_injects_packagist_default() {
    let routes = normalize_routes(Vec::new());
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0], DEFAULT_ROUTE.clone());
    assert_eq!(
        routes[0].url.to_string().trim_end_matches('/'),
        "https://repo.packagist.org"
    );
    assert_eq!(routes[0].name.as_deref(), Some("Packagist"));
}
