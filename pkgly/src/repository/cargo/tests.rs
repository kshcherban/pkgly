#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::{RepositoryType, test_helpers::test_storage};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn create_new_cargo_hosted_requires_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        configs::CargoRepositoryConfigType::get_type_static().to_string(),
        json!({ "type": "Hosted" }),
    );

    let result = CargoRepositoryType::default()
        .create_new(
            "crates".into(),
            Uuid::new_v4(),
            configs.clone(),
            storage.clone(),
        )
        .await;

    assert!(
        result.is_ok(),
        "expected cargo create_new to accept Hosted config, got {result:?}"
    );

    let repository = result.unwrap();
    assert_eq!(repository.repository_type, "cargo");
    assert!(
        repository
            .configs
            .contains_key(configs::CargoRepositoryConfigType::get_type_static()),
        "expected cargo config to be persisted",
    );
}

#[tokio::test]
async fn create_new_cargo_rejects_missing_config() {
    let storage = test_storage().await;
    let configs = HashMap::default();

    let result = CargoRepositoryType::default()
        .create_new("crates".into(), Uuid::new_v4(), configs, storage.clone())
        .await;

    match result {
        Err(RepositoryFactoryError::MissingConfig(key)) => {
            assert_eq!(key, configs::CargoRepositoryConfigType::get_type_static());
        }
        other => panic!("expected missing config error, got {other:?}"),
    }
}
