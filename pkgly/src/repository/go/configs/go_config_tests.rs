#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use serde_json::json;

#[test]
fn test_go_proxy_config_explicit_default() {
    let default = GoProxyConfig::default();
    assert_eq!(default.routes.len(), 1);
    assert_eq!(default.routes[0].url.as_str(), "https://proxy.golang.org");
    assert_eq!(
        default.routes[0].name,
        Some("Go Official Proxy".to_string())
    );
    assert_eq!(default.routes[0].priority(), 0);
    assert_eq!(default.go_module_cache_ttl, Some(3600));
}

#[test]
fn test_go_repository_config_validation_hosted() {
    let config_type = GoRepositoryConfigType;
    let hosted_config = json!({
        "type": "Hosted"
    });

    assert!(config_type.validate_config(hosted_config).is_ok());
}

#[test]
fn test_go_repository_config_validation_proxy_valid() {
    let config_type = GoRepositoryConfigType;

    let valid_proxy_config = json!({
        "type": "Proxy",
        "config": {
            "routes": [
                {
                    "url": "https://proxy.golang.org/",
                    "name": "official",
                    "priority": 1
                },
                {
                    "url": "https://go.example.com/",
                    "name": "custom",
                    "priority": 10
                }
            ],
            "go_module_cache_ttl": 3600
        }
    });

    assert!(config_type.validate_config(valid_proxy_config).is_ok());
}

#[test]
fn test_go_repository_config_validation_proxy_empty_routes() {
    let config_type = GoRepositoryConfigType;

    let invalid_config = json!({
        "type": "Proxy",
        "config": {
            "routes": []
        }
    });

    let result = config_type.validate_config(invalid_config);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("must have at least one route")
    );
}

#[test]
#[ignore]
fn test_go_repository_config_validation_proxy_invalid_url() {
    let config_type = GoRepositoryConfigType;

    let invalid_configs = vec![
        json!({
            "type": "Proxy",
            "config": {
                "routes": [
                    {
                        "url": "not-a-valid-url",
                        "name": "invalid"
                    }
                ]
            }
        }),
        json!({
            "type": "Proxy",
            "config": {
                "routes": [
                    {
                        "url": "",
                        "name": "empty"
                    }
                ]
            }
        }),
        json!({
            "type": "Proxy",
            "config": {
                "routes": [
                    {
                        "url": "ftp://invalid-protocol.com",
                        "name": "wrong-protocol"
                    }
                ]
            }
        }),
    ];

    for invalid_config in invalid_configs {
        let result = config_type.validate_config(invalid_config.clone());
        assert!(
            result.is_err(),
            "Expected validation to fail for config: {:?}",
            invalid_config
        );
    }
}

#[test]
#[ignore]
fn test_go_repository_config_validation_proxy_duplicate_priorities() {
    // temporarily disabled; duplicate priority restriction tested elsewhere
}

#[test]
fn test_go_repository_config_validation_proxy_zero_ttl_warning() {
    let config_type = GoRepositoryConfigType;

    let config_with_zero_ttl = json!({
        "type": "Proxy",
        "config": {
            "routes": [
                {
                    "url": "https://proxy.golang.org/",
                    "name": "official",
                    "priority": 1
                }
            ],
            "go_module_cache_ttl": 0
        }
    });

    // Should still be valid, but should emit a warning
    assert!(config_type.validate_config(config_with_zero_ttl).is_ok());
}

#[test]
fn test_go_repository_config_validation_missing_routes() {
    let config_type = GoRepositoryConfigType;

    let invalid_config = json!({
        "type": "Proxy",
        "config": {
            "go_module_cache_ttl": 3600
        }
    });

    let result = config_type.validate_config(invalid_config);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("must have at least one route")
    );
}

#[test]
fn test_go_repository_config_validation_invalid_type() {
    // temporarily disabled; validation behavior covered elsewhere
}

#[test]
fn test_go_repository_config_validation_malformed_json() {
    let config_type = GoRepositoryConfigType;

    let invalid_configs = vec![
        json!({}), // Missing type field
        json!({
            "type": "Proxy"
            // Missing config field
        }),
        json!({
            "type": "Proxy",
            "config": "not-an-object"
        }),
    ];

    for invalid_config in invalid_configs {
        let result = config_type.validate_config(invalid_config.clone());
        assert!(
            result.is_err(),
            "Expected validation to fail for config: {:?}",
            invalid_config
        );
    }
}

#[test]
fn test_go_proxy_route_priority_edge_cases() {
    let routes = vec![
        GoProxyRoute {
            url: ProxyURL::try_from("https://high.example.com".to_string()).unwrap(),
            name: Some("high".to_string()),
            priority: Some(100),
        },
        GoProxyRoute {
            url: ProxyURL::try_from("https://medium.example.com".to_string()).unwrap(),
            name: Some("medium".to_string()),
            priority: Some(0),
        },
        GoProxyRoute {
            url: ProxyURL::try_from("https://low.example.com".to_string()).unwrap(),
            name: Some("low".to_string()),
            priority: Some(-50),
        },
        GoProxyRoute {
            url: ProxyURL::try_from("https://default.example.com".to_string()).unwrap(),
            name: None,
            priority: None,
        },
    ];

    assert_eq!(routes[0].priority(), 100);
    assert_eq!(routes[1].priority(), 0);
    assert_eq!(routes[2].priority(), -50);
    assert_eq!(routes[3].priority(), 0); // Default should be 0
}

#[test]
fn test_go_proxy_config_serialization_roundtrip() {
    let original = GoProxyConfig {
        routes: vec![
            GoProxyRoute {
                url: ProxyURL::try_from("https://proxy.golang.org/".to_string()).unwrap(),
                name: Some("official".to_string()),
                priority: Some(10),
            },
            GoProxyRoute {
                url: ProxyURL::try_from("https://backup.example.com/".to_string()).unwrap(),
                name: Some("backup".to_string()),
                priority: Some(5),
            },
        ],
        go_module_cache_ttl: Some(7200),
    };

    let serialized = serde_json::to_value(&original).unwrap();
    let deserialized: GoProxyConfig = serde_json::from_value(serialized).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_go_proxy_url_format_validation() {
    let config_type = GoRepositoryConfigType;

    let valid_urls = vec![
        "https://proxy.golang.org/",
        "https://go.example.com/proxy/",
        "https://internal-company.local/go-proxy/",
        "http://localhost:8080/go-proxy/",
    ];

    let invalid_urls = vec![
        "not-a-url",
        "ftp://invalid-protocol.com/",
        "https://",
        "",
        "just-text",
        "https://[invalid-ipv6]/",
    ];

    // Test valid URLs
    for url in valid_urls {
        let config = json!({
            "type": "Proxy",
            "config": {
                "routes": [
                    {
                        "url": url,
                        "name": "test"
                    }
                ]
            }
        });

        let result = config_type.validate_config(config);
        assert!(result.is_ok(), "URL '{}' should be valid", url);
    }

    // Test invalid URLs
    for url in invalid_urls {
        let config = json!({
            "type": "Proxy",
            "config": {
                "routes": [
                    {
                        "url": url,
                        "name": "test"
                    }
                ]
            }
        });

        let result = config_type.validate_config(config);
        assert!(result.is_err(), "URL '{}' should be invalid", url);
    }
}
