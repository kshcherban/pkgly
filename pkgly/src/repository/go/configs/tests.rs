#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use serde_json::json;

#[test]
fn test_go_repository_config_default() {
    let config_type = GoRepositoryConfigType;
    let default = config_type.default().unwrap();
    let parsed: GoRepositoryConfig = serde_json::from_value(default).unwrap();
    assert_eq!(parsed, GoRepositoryConfig::Hosted);
}

#[test]
fn test_go_proxy_config_validation() {
    let config_type = GoRepositoryConfigType;

    // Valid proxy config
    let valid_config = json!({
        "type": "Proxy",
        "config": {
            "routes": [
                {
                    "url": "https://proxy.golang.org",
                    "name": "official",
                    "priority": 1
                }
            ],
            "go_module_cache_ttl": 3600
        }
    });

    assert!(config_type.validate_config(valid_config).is_ok());
}

#[test]
fn test_go_proxy_config_invalid_url() {
    let config_type = GoRepositoryConfigType;

    // Invalid URL
    let invalid_config = json!({
        "type": "Proxy",
        "config": {
            "routes": [
                {
                    "url": "not-a-url",
                    "name": "invalid"
                }
            ]
        }
    });

    assert!(config_type.validate_config(invalid_config).is_err());
}

#[test]
fn test_go_proxy_route_priority_default() {
    let route = GoProxyRoute {
        url: ProxyURL::try_from("https://proxy.golang.org".to_string()).unwrap(),
        name: Some("test".to_string()),
        priority: None,
    };

    assert_eq!(route.priority(), 0);
}

#[test]
fn test_go_proxy_route_priority_custom() {
    let route = GoProxyRoute {
        url: ProxyURL::try_from("https://proxy.golang.org".to_string()).unwrap(),
        name: Some("test".to_string()),
        priority: Some(5),
    };

    assert_eq!(route.priority(), 5);
}

#[test]
fn test_go_repository_config_type_description() {
    let config_type = GoRepositoryConfigType;
    let description = config_type.get_description();

    assert_eq!(description.name, "Go Repository Config");
    assert!(description.description.is_some());
    assert!(description.documentation_link.is_some());
}

#[test]
fn test_go_repository_config_schema() {
    let config_type = GoRepositoryConfigType;
    let schema = config_type.schema();
    assert!(schema.is_some());
}
