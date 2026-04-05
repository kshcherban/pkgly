#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use super::proxy::{DEFAULT_ROUTE, normalize_routes, rewrite_proxy_metadata};
use serde_json::Value;

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

#[test]
fn rewrite_metadata_rewrites_dist_and_collects_proxy_meta() {
    let body = br#"{
        "packages": {
            "acme/example": [{
                "name": "acme/example",
                "version": "1.2.3",
                "dist": {
                    "url": "https://files.example.com/dist/pkg-1.2.3.zip",
                    "shasum": "abc123"
                }
            }]
        }
    }"#;

    let (rewritten, metas) =
        rewrite_proxy_metadata(body, "/repositories/main/php-proxy").expect("rewrite succeeds");

    let value: Value = serde_json::from_slice(&rewritten).expect("json");
    let dist_url = value["packages"]["acme/example"][0]["dist"]["url"]
        .as_str()
        .expect("dist url");
    assert_eq!(
        dist_url,
        "/repositories/main/php-proxy/dist/acme/example/1.2.3/pkg-1.2.3.zip"
    );

    assert_eq!(metas.len(), 1);
    let meta = &metas[0];
    assert_eq!(meta.package_key, "acme/example");
    assert_eq!(meta.package_name, "acme/example");
    assert_eq!(meta.version.as_deref(), Some("1.2.3"));
    assert_eq!(
        meta.cache_path,
        "dist/acme/example/1.2.3/pkg-1.2.3.zip".to_string()
    );
    assert_eq!(
        meta.upstream_url.as_deref(),
        Some("https://files.example.com/dist/pkg-1.2.3.zip")
    );
}
