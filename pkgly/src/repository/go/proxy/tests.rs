#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::proxy_indexing::{ProxyIndexing, ProxyIndexingError};
use async_trait::async_trait;
use nr_core::repository::project::{ProxyArtifactKey, ProxyArtifactMeta};
use std::sync::Arc;
use tokio::sync::Mutex;

#[test]
fn test_proxy_route_sorting() {
    let mut routes = vec![
        GoProxyRoute {
            url: ProxyURL::try_from("https://low.example.com".to_string()).unwrap(),
            name: Some("Low Priority".to_string()),
            priority: Some(1),
        },
        GoProxyRoute {
            url: ProxyURL::try_from("https://high.example.com".to_string()).unwrap(),
            name: Some("High Priority".to_string()),
            priority: Some(10),
        },
        GoProxyRoute {
            url: ProxyURL::try_from("https://medium.example.com".to_string()).unwrap(),
            name: Some("Medium Priority".to_string()),
            priority: Some(5),
        },
    ];

    routes.sort_by_key(|route| -route.priority());

    assert_eq!(routes[0].priority(), 10);
    assert_eq!(routes[1].priority(), 5);
    assert_eq!(routes[2].priority(), 1);
}

#[test]
fn test_normalize_routes() {
    let empty_routes: Vec<GoProxyRoute> = vec![];
    let normalized = normalize_routes(empty_routes);
    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].name.as_deref(), Some("Go Official Proxy"));

    let custom_routes = vec![
        GoProxyRoute {
            url: ProxyURL::try_from("https://custom1.example.com".to_string()).unwrap(),
            name: Some("Custom1".to_string()),
            priority: Some(10),
        },
        GoProxyRoute {
            url: ProxyURL::try_from("https://custom2.example.com".to_string()).unwrap(),
            name: Some("Custom2".to_string()),
            priority: Some(5),
        },
    ];

    let normalized = normalize_routes(custom_routes);
    assert_eq!(normalized.len(), 2);
    assert_eq!(normalized[0].priority(), 10); // Higher priority first
    assert_eq!(normalized[1].priority(), 5);
}

fn go_zip_path() -> StoragePath {
    StoragePath::from("go-proxy-cache/github.com/example/module/@v/v1.2.3.zip")
}

#[test]
fn go_proxy_meta_from_cache_path_parses_versioned_zip() {
    let path = go_zip_path();
    let meta = super::go_proxy_meta_from_cache_path(&path, 8192).expect("metadata");
    assert_eq!(meta.package_name, "github.com/example/module");
    assert_eq!(meta.package_key, "github.com/example/module");
    assert_eq!(meta.version.as_deref(), Some("v1.2.3"));
    assert_eq!(meta.cache_path, path.to_string());
    assert_eq!(meta.size, Some(8192));
}

#[test]
fn go_proxy_key_from_cache_path_handles_mod_file() {
    let path = StoragePath::from("go-proxy-cache/github.com/example/mod/@v/v2.0.0.mod");
    let key = super::go_proxy_key_from_cache_path(&path).expect("key");
    assert_eq!(key.package_key, "github.com/example/mod");
    assert_eq!(key.version.as_deref(), Some("v2.0.0"));
}

#[tokio::test]
async fn record_go_proxy_cache_hit_invokes_indexer() {
    let path = go_zip_path();
    let indexer = RecordingIndexer::default();

    super::record_go_proxy_cache_hit(&indexer, &path, 16384)
        .await
        .expect("indexing succeeds");
    let recorded = indexer.recorded().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].package_key, "github.com/example/module");
    assert_eq!(recorded[0].version.as_deref(), Some("v1.2.3"));
}

#[tokio::test]
async fn evict_go_proxy_cache_entry_invokes_indexer() {
    let path = go_zip_path();
    let indexer = RecordingIndexer::default();

    super::evict_go_proxy_cache_entry(&indexer, &path)
        .await
        .expect("eviction succeeds");
    let evicted = indexer.evicted().await;
    assert_eq!(evicted.len(), 1);
    assert_eq!(evicted[0].package_key, "github.com/example/module");
    assert_eq!(evicted[0].version.as_deref(), Some("v1.2.3"));
}

#[derive(Clone, Default)]
struct RecordingIndexer {
    recorded: Arc<Mutex<Vec<ProxyArtifactMeta>>>,
    evicted: Arc<Mutex<Vec<ProxyArtifactKey>>>,
}

impl RecordingIndexer {
    async fn recorded(&self) -> Vec<ProxyArtifactMeta> {
        self.recorded.lock().await.clone()
    }

    async fn evicted(&self) -> Vec<ProxyArtifactKey> {
        self.evicted.lock().await.clone()
    }
}

#[async_trait]
impl ProxyIndexing for RecordingIndexer {
    async fn record_cached_artifact(
        &self,
        meta: ProxyArtifactMeta,
    ) -> Result<(), ProxyIndexingError> {
        self.recorded.lock().await.push(meta);
        Ok(())
    }

    async fn evict_cached_artifact(&self, key: ProxyArtifactKey) -> Result<(), ProxyIndexingError> {
        self.evicted.lock().await.push(key);
        Ok(())
    }
}
