use nr_core::repository::config::RepositoryConfigType;
use serde_json::json;

use super::RubyRepositoryConfigType;

#[test]
fn default_config_is_hosted() {
    let config = RubyRepositoryConfigType.default().expect("default config");
    assert_eq!(config, json!({ "type": "Hosted" }));
}

#[test]
fn validate_accepts_hosted_config() {
    RubyRepositoryConfigType
        .validate_config(json!({ "type": "Hosted" }))
        .expect("hosted config valid");
}

#[test]
fn validate_accepts_proxy_config() {
    RubyRepositoryConfigType
        .validate_config(json!({
            "type": "Proxy",
            "config": {
                "upstream_url": "https://rubygems.org",
                "revalidation_ttl_seconds": 300
            }
        }))
        .expect("proxy config valid");
}

#[test]
fn validate_rejects_missing_type_tag() {
    let err = RubyRepositoryConfigType
        .validate_config(json!({}))
        .expect_err("missing type should error");
    let message = err.to_string();
    assert!(
        !message.trim().is_empty(),
        "error message should not be empty"
    );
}
