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
