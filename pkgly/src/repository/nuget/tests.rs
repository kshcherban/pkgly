#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::repository::{RepositoryType, test_helpers::test_storage};
use ahash::HashMap;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn create_new_nuget_hosted_accepts_default_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        NugetRepositoryConfigType::get_type_static().to_string(),
        json!({ "type": "Hosted" }),
    );

    let result = NugetRepositoryType::default()
        .create_new("nuget-hosted".into(), Uuid::new_v4(), configs, storage)
        .await;

    let repository = result.expect("hosted repository to be created");
    assert_eq!(repository.repository_type, "nuget");
    assert!(
        repository
            .configs
            .contains_key(NugetRepositoryConfigType::get_type_static()),
        "expected nuget config to be persisted",
    );
}

#[tokio::test]
async fn create_new_nuget_proxy_requires_upstream_url() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        NugetRepositoryConfigType::get_type_static().to_string(),
        json!({ "type": "Proxy", "config": {} }),
    );

    let result = NugetRepositoryType::default()
        .create_new("nuget-proxy".into(), Uuid::new_v4(), configs, storage)
        .await;

    match result {
        Err(RepositoryFactoryError::InvalidConfig(repository, message)) => {
            assert_eq!(repository, "nuget");
            assert!(
                message.contains("upstream_url"),
                "expected upstream_url validation error, got: {message}"
            );
        }
        other => panic!("expected invalid config error, got: {other:?}"),
    }
}

#[tokio::test]
async fn create_new_nuget_virtual_rejects_empty_member_list() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        NugetRepositoryConfigType::get_type_static().to_string(),
        json!({ "type": "Virtual", "config": { "member_repositories": [] } }),
    );

    let result = NugetRepositoryType::default()
        .create_new("nuget-virtual".into(), Uuid::new_v4(), configs, storage)
        .await;

    match result {
        Err(RepositoryFactoryError::InvalidConfig(repository, message)) => {
            assert_eq!(repository, "nuget");
            assert!(
                message.contains("member"),
                "expected error message to mention members, got: {message}"
            );
        }
        other => panic!("expected invalid config error, got: {other:?}"),
    }
}

#[tokio::test]
async fn create_new_nuget_virtual_accepts_valid_config_shape() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        NugetRepositoryConfigType::get_type_static().to_string(),
        json!({
            "type": "Virtual",
            "config": {
                "member_repositories": [
                    {
                        "repository_id": Uuid::new_v4(),
                        "repository_name": "member-1",
                        "priority": 0,
                        "enabled": true
                    }
                ],
                "resolution_order": "Priority",
                "cache_ttl_seconds": 60,
                "publish_to": null
            }
        }),
    );

    let result = NugetRepositoryType::default()
        .create_new("nuget-virtual".into(), Uuid::new_v4(), configs, storage)
        .await;

    let repository = result.expect("virtual repository to be created");
    assert_eq!(repository.repository_type, "nuget");
}
