use std::sync::Arc;

use async_trait::async_trait;
use nr_core::repository::project::{ProxyArtifactKey, ProxyArtifactMeta};
use tokio::sync::Mutex;

use crate::repository::{
    docker::proxy::DockerProxy,
    go::proxy::GoProxy,
    maven::proxy::MavenProxy,
    npm::proxy::NpmProxyRegistry,
    proxy::base_proxy::{ProxyRepository, evict_proxy_cache_entry, record_proxy_cache_hit},
    proxy_indexing::{ProxyIndexing, ProxyIndexingError},
    python::proxy::PythonProxy,
};

// Compile-time assertion that the main proxy types implement the marker trait.
fn assert_is_proxy<T: ProxyRepository>() {}

#[test]
fn proxy_repositories_implement_marker_trait() {
    assert_is_proxy::<GoProxy>();
    assert_is_proxy::<MavenProxy>();
    assert_is_proxy::<NpmProxyRegistry>();
    assert_is_proxy::<PythonProxy>();
    assert_is_proxy::<DockerProxy>();
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

#[tokio::test]
async fn record_proxy_cache_hit_invokes_indexer_when_meta_present() {
    let indexer = RecordingIndexer::default();
    let meta = ProxyArtifactMeta::builder("example", "example-key", "cache/path").build();

    record_proxy_cache_hit(&indexer, Some(meta.clone()))
        .await
        .expect("recording succeeds");

    let recorded = indexer.recorded().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].package_key, meta.package_key);
    assert_eq!(recorded[0].cache_path, meta.cache_path);
}

#[tokio::test]
async fn record_proxy_cache_hit_ignores_none_meta() {
    let indexer = RecordingIndexer::default();

    record_proxy_cache_hit(&indexer, None)
        .await
        .expect("recording succeeds");

    let recorded = indexer.recorded().await;
    assert!(recorded.is_empty());
}

#[tokio::test]
async fn evict_proxy_cache_entry_invokes_indexer_when_key_present() {
    let indexer = RecordingIndexer::default();
    let key = ProxyArtifactKey {
        package_key: "example-key".to_string(),
        version: Some("1.0.0".to_string()),
        cache_path: Some("cache/path".to_string()),
    };

    evict_proxy_cache_entry(&indexer, Some(key.clone()))
        .await
        .expect("eviction succeeds");

    let evicted = indexer.evicted().await;
    assert_eq!(evicted.len(), 1);
    assert_eq!(evicted[0].package_key, key.package_key);
    assert_eq!(evicted[0].version, key.version);
    assert_eq!(evicted[0].cache_path, key.cache_path);
}

#[tokio::test]
async fn evict_proxy_cache_entry_ignores_none_key() {
    let indexer = RecordingIndexer::default();

    evict_proxy_cache_entry(&indexer, None)
        .await
        .expect("eviction succeeds");

    let evicted = indexer.evicted().await;
    assert!(evicted.is_empty());
}
