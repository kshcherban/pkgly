#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::proxy_indexing::{ProxyIndexing, ProxyIndexingError};
use async_trait::async_trait;
use nr_core::repository::project::{ProxyArtifactKey, ProxyArtifactMeta};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

#[test]
fn cache_path_for_simple_package() {
    let path = StoragePath::from("simple/Example_Pkg/example-1.0.0.whl");
    let cache = cache_path_for_python_proxy(&path).expect("cache path");
    assert_eq!(cache.to_string(), "packages/example-pkg/example-1.0.0.whl");
}

#[test]
fn cache_path_for_directory_returns_none() {
    let path = StoragePath::from("simple/example/");
    assert!(cache_path_for_python_proxy(&path).is_none());
}

#[test]
fn cache_path_preserves_existing_packages_path() {
    let path = StoragePath::from("packages/example/example-1.0.0.whl");
    let cache = cache_path_for_python_proxy(&path).expect("cache path");
    assert_eq!(cache.to_string(), "packages/example/example-1.0.0.whl");
}

#[test]
fn normalize_routes_injects_default_when_empty() {
    let routes = normalize_routes(Vec::new());
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0], DEFAULT_ROUTE.clone());
}

#[test]
fn normalize_routes_retains_existing_entries() {
    let custom = PythonProxyRoute {
        url: ProxyURL::try_from(String::from("https://internal.example/simple"))
            .expect("valid url"),
        name: Some("Internal".to_string()),
    };
    let routes = normalize_routes(vec![custom.clone()]);
    assert_eq!(routes, vec![custom]);
}

#[test]
fn derive_request_base_path_strips_request_path_suffix() {
    let uri_path = "/repositories/storage/python-proxy/simple/pkg/";
    let storage_path = StoragePath::from("simple/pkg/");
    let base = derive_request_base_path(uri_path, &storage_path).expect("base path");
    assert_eq!(base, "/repositories/storage/python-proxy");
}

fn metadata_path() -> StoragePath {
    StoragePath::from(
        "packages/bc/66/875d449b23194f45debb8a2b70c704217f0aa2700d967098b2e1b812dd44/parallel_ssh-2.12.0-py3-none-any.whl",
    )
}

#[test]
fn python_proxy_meta_from_cache_path_extracts_wheel_fields() {
    let path = metadata_path();
    let url = Url::parse("https://pypi.org/packages/example.whl").expect("url");
    let meta = super::python_proxy_meta_from_cache_path(&path, 2048, Some(&url)).expect("metadata");
    assert_eq!(meta.package_name, "parallel_ssh");
    assert_eq!(meta.package_key, "parallel-ssh");
    assert_eq!(meta.version.as_deref(), Some("2.12.0"));
    assert_eq!(meta.cache_path, path.to_string());
    assert_eq!(meta.size, Some(2048));
    assert_eq!(
        meta.upstream_url.as_deref(),
        Some("https://pypi.org/packages/example.whl")
    );
}

#[test]
fn python_proxy_meta_from_cache_path_handles_sdist_suffix() {
    let path = StoragePath::from("packages/ab/cd/ef/pkg-name-0.9.0.tar.gz");
    let meta = super::python_proxy_meta_from_cache_path(&path, 512, None).expect("metadata");
    assert_eq!(meta.package_name, "pkg-name");
    assert_eq!(meta.package_key, "pkg-name");
    assert_eq!(meta.version.as_deref(), Some("0.9.0"));
    assert_eq!(meta.cache_path, path.to_string());
    assert_eq!(meta.size, Some(512));
}

#[test]
fn python_proxy_meta_from_cache_path_accepts_v_prefixed_versions() {
    let path = StoragePath::from("packages/example/example-v1.0.0.whl");
    let meta = super::python_proxy_meta_from_cache_path(&path, 256, None).expect("metadata");
    assert_eq!(meta.version.as_deref(), Some("1.0.0"));
}

#[tokio::test]
async fn record_python_proxy_cache_hit_invokes_indexer() {
    let path = metadata_path();
    let url = Url::parse("https://pypi.org/packages/example.whl").expect("url");
    let indexer = RecordingIndexer::default();

    super::record_python_proxy_cache_hit(&indexer, &path, 1024, Some(&url))
        .await
        .expect("indexing succeeds");
    let recorded = indexer.recorded().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].package_key, "parallel-ssh");
    assert_eq!(recorded[0].version.as_deref(), Some("2.12.0"));
}

#[tokio::test]
async fn evict_python_proxy_cache_entry_invokes_indexer() {
    let path = metadata_path();
    let indexer = RecordingIndexer::default();

    super::evict_python_proxy_cache_entry(&indexer, &path)
        .await
        .expect("eviction succeeds");
    let evicted = indexer.evicted().await;
    assert_eq!(evicted.len(), 1);
    assert_eq!(evicted[0].package_key, "parallel-ssh");
    assert_eq!(evicted[0].version.as_deref(), Some("2.12.0"));
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
