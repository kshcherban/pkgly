#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::repository::{RepositoryType, test_helpers::test_storage};
use ahash::HashMap;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn create_new_ruby_hosted_accepts_default_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        RubyRepositoryConfigType::get_type_static().to_string(),
        json!({ "type": "Hosted" }),
    );

    let result = RubyRepositoryType::default()
        .create_new("ruby-hosted".into(), Uuid::new_v4(), configs, storage)
        .await;

    let repository = result.expect("hosted repository to be created");
    assert_eq!(repository.repository_type, REPOSITORY_TYPE_ID);
    assert!(
        repository
            .configs
            .contains_key(RubyRepositoryConfigType::get_type_static()),
        "expected ruby config to be persisted",
    );
}

#[tokio::test]
async fn create_new_ruby_proxy_accepts_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        RubyRepositoryConfigType::get_type_static().to_string(),
        json!({
            "type": "Proxy",
            "config": {
                "upstream_url": "https://rubygems.org"
            }
        }),
    );

    let result = RubyRepositoryType::default()
        .create_new("ruby-proxy".into(), Uuid::new_v4(), configs, storage)
        .await;

    let repository = result.expect("proxy repository to be created");
    assert_eq!(repository.repository_type, REPOSITORY_TYPE_ID);
}

#[tokio::test]
async fn create_new_ruby_rejects_config_missing_type() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        RubyRepositoryConfigType::get_type_static().to_string(),
        json!({}),
    );

    let result = RubyRepositoryType::default()
        .create_new("ruby-invalid".into(), Uuid::new_v4(), configs, storage)
        .await;

    match result {
        Err(super::RepositoryFactoryError::InvalidConfig(repository, message)) => {
            assert_eq!(repository, REPOSITORY_TYPE_ID);
            assert!(
                message.contains("type"),
                "expected error message to mention missing type, got: {message}"
            );
        }
        other => panic!("expected invalid config error, got: {other:?}"),
    }
}
