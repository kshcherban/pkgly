#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn test_normalize_routes() {
    let empty_routes: Vec<GoProxyRoute> = vec![];
    let normalized = normalize_routes(empty_routes);
    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].name.as_deref(), Some("Go Official Proxy"));

    let custom_routes = vec![
        GoProxyRoute {
            url: ProxyURL::try_from("https://custom1.example.com".to_string()).unwrap(),
            name: Some("Custom1".to_string()),
            priority: Some(10),
        },
        GoProxyRoute {
            url: ProxyURL::try_from("https://custom2.example.com".to_string()).unwrap(),
            name: Some("Custom2".to_string()),
            priority: Some(5),
        },
    ];

    let normalized = normalize_routes(custom_routes);
    assert_eq!(normalized.len(), 2);
    assert_eq!(normalized[0].priority(), 10); // Higher priority first
    assert_eq!(normalized[1].priority(), 5);
}

fn go_zip_path() -> StoragePath {
    StoragePath::from("go-proxy-cache/github.com/example/module/@v/v1.2.3.zip")
}

#[test]
fn go_proxy_meta_from_cache_path_parses_versioned_zip() {
    let path = go_zip_path();
    let meta = super::go_proxy_meta_from_cache_path(&path, 8192).expect("metadata");
    assert_eq!(meta.package_name, "github.com/example/module");
    assert_eq!(meta.package_key, "github.com/example/module");
    assert_eq!(meta.version.as_deref(), Some("v1.2.3"));
    assert_eq!(meta.cache_path, path.to_string());
    assert_eq!(meta.size, Some(8192));
}

#[test]
fn go_proxy_key_from_cache_path_handles_mod_file() {
    let path = StoragePath::from("go-proxy-cache/github.com/example/mod/@v/v2.0.0.mod");
    let key = super::go_proxy_key_from_cache_path(&path).expect("key");
    assert_eq!(key.package_key, "github.com/example/mod");
    assert_eq!(key.version.as_deref(), Some("v2.0.0"));
}
