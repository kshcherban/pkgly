#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use super::*;
use crate::repository::proxy_indexing::{ProxyIndexing, ProxyIndexingError};
use crate::{repository::test_helpers::test_storage, utils::ResponseBuilder};
use async_trait::async_trait;
use axum::{Router, extract::State, http::HeaderMap, routing::get};
use bytes::Bytes;
use http::StatusCode;
use nr_core::repository::project::ProxyArtifactMeta;
use nr_core::{repository::proxy_url::ProxyURL, storage::StoragePath};
use nr_storage::{FileContent, Storage};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use uuid::Uuid;

#[derive(Clone, Default)]
struct UpstreamState {
    counter: Arc<AtomicUsize>,
    last_range: Arc<Mutex<Option<String>>>,
    payload: Bytes,
}

async fn start_upstream_server(state: UpstreamState) -> anyhow::Result<(ProxyURL, JoinHandle<()>)> {
    let app = Router::new().route(
        "/{*path}",
        get(
            |State(state): State<UpstreamState>, headers: HeaderMap| async move {
                state.counter.fetch_add(1, Ordering::SeqCst);
                if let Some(value) = headers
                    .get(http::header::RANGE)
                    .and_then(|v| v.to_str().ok())
                {
                    *state.last_range.lock().await = Some(value.to_string());
                    if let Some(start) = range_start_bytes(value) {
                        let start_u64 = start;
                        let start: usize = start.try_into().unwrap_or(usize::MAX);
                        let slice = state
                            .payload
                            .get(start..)
                            .map(|bytes| Bytes::copy_from_slice(bytes))
                            .unwrap_or_default();
                        let total = state.payload.len();
                        let end = total.saturating_sub(1);
                        let content_range = format!("bytes {start_u64}-{end}/{total}");
                        return ResponseBuilder::default()
                            .status(StatusCode::PARTIAL_CONTENT)
                            .header(http::header::ACCEPT_RANGES, "bytes")
                            .header(http::header::CONTENT_RANGE, content_range)
                            .body(slice);
                    }
                }
                ResponseBuilder::ok().body(state.payload.clone())
            },
        ),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app.with_state(state)).await {
            eprintln!("upstream server error: {err}");
        }
    });
    let url = ProxyURL::try_from(format!("http://{addr}"))?;
    Ok((url, server))
}

async fn read_cached(storage: &DynStorage, repo_id: Uuid, path: &StoragePath) -> Vec<u8> {
    let file = storage
        .open_file(repo_id, path)
        .await
        .expect("open")
        .expect("exists");
    let nr_storage::StorageFile::File { meta, content } = file else {
        panic!("expected file");
    };
    let nr_storage::FileFileType { file_size, .. } = meta.file_type();
    let len: usize = (*file_size).try_into().expect("usize");
    content.read_to_vec(len).await.expect("read")
}

#[test]
fn ruby_proxy_meta_from_cache_path_extracts_platform_when_present() {
    let path = StoragePath::from("gems/nokogiri-1.15.4-x86_64-linux.gem");
    let meta = ruby_proxy_meta_from_cache_path(&path, 2048).expect("metadata");
    assert_eq!(meta.package_name, "nokogiri");
    assert_eq!(meta.package_key, "nokogiri");
    assert_eq!(meta.version.as_deref(), Some("1.15.4-x86_64-linux"));
    assert_eq!(meta.cache_path, "gems/nokogiri-1.15.4-x86_64-linux.gem");
    assert_eq!(meta.size, Some(2048));
}

#[test]
fn ruby_proxy_meta_from_cache_path_extracts_plain_version() {
    let path = StoragePath::from("gems/rack-3.0.0.gem");
    let meta = ruby_proxy_meta_from_cache_path(&path, 128).expect("metadata");
    assert_eq!(meta.package_name, "rack");
    assert_eq!(meta.package_key, "rack");
    assert_eq!(meta.version.as_deref(), Some("3.0.0"));
}

#[tokio::test]
async fn cache_through_returns_hit_without_contacting_upstream() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let path = StoragePath::from("gems/rack-3.0.0.gem");
    storage
        .save_file(
            repo_id,
            FileContent::Bytes(Bytes::from_static(b"cached")),
            &path,
        )
        .await
        .expect("seed cache");

    let counter = Arc::new(AtomicUsize::new(0));
    let state = UpstreamState {
        counter: counter.clone(),
        last_range: Arc::new(Mutex::new(None)),
        payload: Bytes::from_static(b"upstream"),
    };
    let (upstream, _server) = start_upstream_server(state).await.expect("upstream");

    let client = reqwest::Client::new();
    let outcome = fetch_and_cache_if_missing(&client, &storage, repo_id, &upstream, &path, None)
        .await
        .expect("cache-through succeeds");
    assert_eq!(outcome, CacheThroughOutcome::Hit);
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn record_ruby_proxy_cache_hit_invokes_indexer() {
    let indexer = RecordingIndexer::default();
    let path = StoragePath::from("gems/rack-3.0.0.gem");

    record_ruby_proxy_cache_hit(&indexer, &path, 42)
        .await
        .expect("indexing succeeds");

    let recorded = indexer.recorded().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].package_key, "rack");
    assert_eq!(recorded[0].version.as_deref(), Some("3.0.0"));
}

#[tokio::test]
async fn cache_through_fetches_upstream_on_miss_and_persists() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let path = StoragePath::from("info/rack");

    let counter = Arc::new(AtomicUsize::new(0));
    let state = UpstreamState {
        counter: counter.clone(),
        last_range: Arc::new(Mutex::new(None)),
        payload: Bytes::from_static(b"---\n3.0.0|checksum:deadbeef\n"),
    };
    let (upstream, _server) = start_upstream_server(state).await.expect("upstream");

    let client = reqwest::Client::new();
    let outcome = fetch_and_cache_if_missing(&client, &storage, repo_id, &upstream, &path, None)
        .await
        .expect("cache-through succeeds");
    assert!(matches!(outcome, CacheThroughOutcome::Fetched { .. }));
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let cached = read_cached(&storage, repo_id, &path).await;
    assert_eq!(cached, b"---\n3.0.0|checksum:deadbeef\n");
}

#[tokio::test]
async fn range_fetch_forwards_header_and_appends_when_suffix_matches_cache_size() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let path = StoragePath::from("versions");
    storage
        .save_file(
            repo_id,
            FileContent::Bytes(Bytes::from_static(b"abc")),
            &path,
        )
        .await
        .expect("seed cache");

    let last_range = Arc::new(Mutex::new(None));
    let state = UpstreamState {
        counter: Arc::new(AtomicUsize::new(0)),
        last_range: last_range.clone(),
        payload: Bytes::from_static(b"abcdef"),
    };
    let (upstream, _server) = start_upstream_server(state).await.expect("upstream");

    let client = reqwest::Client::new();
    let outcome = fetch_range_and_maybe_append(
        &client, &storage, repo_id, &upstream, &path, None, "bytes=3-",
    )
    .await
    .expect("range succeeds");

    assert_eq!(outcome.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(outcome.body, Bytes::from_static(b"def"));
    assert_eq!(
        last_range.lock().await.as_deref(),
        Some("bytes=3-"),
        "expected Range header to reach upstream"
    );

    let cached = read_cached(&storage, repo_id, &path).await;
    assert_eq!(cached, b"abcdef");
}

#[derive(Clone, Default)]
struct RecordingIndexer {
    recorded: Arc<Mutex<Vec<ProxyArtifactMeta>>>,
}

impl RecordingIndexer {
    async fn recorded(&self) -> Vec<ProxyArtifactMeta> {
        self.recorded.lock().await.clone()
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

    async fn evict_cached_artifact(
        &self,
        _key: nr_core::repository::project::ProxyArtifactKey,
    ) -> Result<(), ProxyIndexingError> {
        Ok(())
    }
}
