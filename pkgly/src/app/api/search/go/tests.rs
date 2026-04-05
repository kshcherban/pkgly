#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use bytes::Bytes;
use nr_storage::{FileContent, Storage};
use uuid::Uuid;

use super::*;
use crate::app::api::search::query_parser::SearchQuery;
use crate::repository::test_helpers::test_storage;

#[tokio::test]
async fn search_go_modules_includes_hosted_packages() {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(b"mod-zip")),
            &StoragePath::from("packages/github.com/example/project/@v/v1.2.3.info"),
        )
        .await
        .unwrap();

    let summary = RepositorySummary {
        repository_id,
        repository_name: "go-hosted".into(),
        storage_name: "local".into(),
        repository_type: "go".into(),
    };
    let query = SearchQuery {
        terms: vec!["project".to_string()],
        ..SearchQuery::default()
    };

    let result = search_go_modules(&storage, &summary, &query, 10).await;
    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].file_name, "github.com/example/project@v1.2.3");
}

#[tokio::test]
async fn search_go_modules_limits_proxy_results() {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(b"info-json")),
            &StoragePath::from("go-proxy-cache/github.com/example/project/@v/v1.0.0.info"),
        )
        .await
        .unwrap();
    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(b"info-json")),
            &StoragePath::from("go-proxy-cache/github.com/example/other/@v/v1.0.0.info"),
        )
        .await
        .unwrap();

    let summary = RepositorySummary {
        repository_id,
        repository_name: "go-proxy".into(),
        storage_name: "local".into(),
        repository_type: "go".into(),
    };
    let query = SearchQuery {
        terms: vec!["github.com/example".to_string()],
        ..SearchQuery::default()
    };

    let result = search_go_modules(&storage, &summary, &query, 1).await;
    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(packages.len(), 1);
}
