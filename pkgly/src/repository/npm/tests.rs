#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::{RepositoryType, test_helpers::test_storage};
use ahash::HashMap;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn create_new_npm_proxy_accepts_route_config() {
    let storage = test_storage().await;
    let mut configs = HashMap::default();
    configs.insert(
        NPMRegistryConfigType::get_type_static().to_string(),
        json!({
            "type": "Proxy",
            "config": {
                "routes": [
                    {
                        "url": "https://registry.npmjs.org",
                        "name": "npmjs"
                    }
                ]
            }
        }),
    );
    let result = NpmRegistryType::default()
        .create_new("npm-proxy".into(), Uuid::new_v4(), configs, storage)
        .await;
    let repository = result.expect("proxy repository to be created");
    assert_eq!(repository.repository_type, "npm");
}
