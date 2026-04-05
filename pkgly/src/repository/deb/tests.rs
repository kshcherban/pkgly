#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::{RepositoryType, test_helpers::test_storage};
use nr_core::repository::config::RepositoryConfigType;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn create_new_deb_accepts_default_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        DebRepositoryConfigType::get_type_static().to_string(),
        json!(DebRepositoryConfig::default()),
    );
    let repo = DebRepositoryType::default()
        .create_new("deb-hosted".into(), Uuid::new_v4(), configs, storage)
        .await;
    assert!(
        repo.is_ok(),
        "expected repository creation to succeed: {repo:?}"
    );
}

#[tokio::test]
async fn create_new_deb_fails_without_config() {
    let storage = test_storage().await;
    let repo = DebRepositoryType::default()
        .create_new(
            "deb-hosted".into(),
            Uuid::new_v4(),
            HashMap::default(),
            storage,
        )
        .await;
    assert!(matches!(
        repo,
        Err(super::RepositoryFactoryError::MissingConfig("deb"))
    ));
}
