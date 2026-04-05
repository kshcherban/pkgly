#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::{RepositoryType, test_helpers::test_storage};
use ahash::HashMap;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn create_new_go_hosted_accepts_default_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        GoRepositoryConfigType::get_type_static().to_string(),
        json!({ "type": "Hosted" }),
    );
    let result = GoRepositoryType::default()
        .create_new("go-hosted".into(), Uuid::new_v4(), configs, storage)
        .await;
    let repository = result.expect("hosted repository to be created");
    assert_eq!(repository.repository_type, "go");
    assert!(
        repository
            .configs
            .contains_key(GoRepositoryConfigType::get_type_static()),
        "expected go config to be persisted",
    );
}

#[tokio::test]
async fn create_new_go_proxy_accepts_valid_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        GoRepositoryConfigType::get_type_static().to_string(),
        json!({
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
        }),
    );
    let result = GoRepositoryType::default()
        .create_new("go-proxy".into(), Uuid::new_v4(), configs, storage)
        .await;
    let repository = result.expect("proxy repository to be created");
    assert_eq!(repository.repository_type, "go");
    assert!(
        repository
            .configs
            .contains_key(GoRepositoryConfigType::get_type_static()),
        "expected go config to be persisted",
    );
}

#[tokio::test]
async fn create_new_go_rejects_config_missing_type() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        GoRepositoryConfigType::get_type_static().to_string(),
        json!({}),
    );
    let result = GoRepositoryType::default()
        .create_new("go-invalid".into(), Uuid::new_v4(), configs, storage)
        .await;
    match result {
        Err(RepositoryFactoryError::InvalidConfig(repository, message)) => {
            assert_eq!(repository, "go");
            assert!(
                message.contains("type"),
                "expected error message to mention missing type, got: {message}"
            );
        }
        other => panic!("expected invalid config error, got: {other:?}"),
    }
}

#[tokio::test]
async fn create_new_go_proxy_rejects_invalid_url() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        GoRepositoryConfigType::get_type_static().to_string(),
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
    );
    let result = GoRepositoryType::default()
        .create_new("go-invalid".into(), Uuid::new_v4(), configs, storage)
        .await;
    assert!(
        result.is_err(),
        "expected repository creation to fail with invalid URL"
    );
}
