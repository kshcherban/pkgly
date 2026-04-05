#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::test_helpers::test_storage;
use axum::{Router, routing::get};
use bytes::Bytes;
use nr_core::{repository::proxy_url::ProxyURL, storage::StoragePath};
use nr_storage::FileContent;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

async fn start_upstream_server(
    body: &'static [u8],
    counter: Arc<AtomicUsize>,
) -> anyhow::Result<(ProxyURL, JoinHandle<()>)> {
    let app = Router::new().route(
        "/{*path}",
        get(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                body
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("upstream server error: {err}");
        }
    });
    let url = ProxyURL::try_from(format!("http://{addr}"))?;
    Ok((url, server))
}

async fn start_upstream_server_with_prefix(
    prefix: &'static str,
    body: &'static [u8],
    counter: Arc<AtomicUsize>,
) -> anyhow::Result<(ProxyURL, JoinHandle<()>)> {
    let route = format!("/{prefix}/{{*path}}");
    let app = Router::new().route(
        route.as_str(),
        get(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                body
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("upstream server error: {err}");
        }
    });
    let url = ProxyURL::try_from(format!("http://{addr}/{prefix}"))?;
    Ok((url, server))
}

#[tokio::test]
async fn cache_through_returns_hit_without_contacting_upstream() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let path = StoragePath::from("dists/stable/Release");
    storage
        .save_file(
            repo_id,
            FileContent::Bytes(Bytes::from_static(b"cached")),
            &path,
        )
        .await
        .expect("seed cache");

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream, _server) = start_upstream_server(b"upstream", counter.clone())
        .await
        .expect("start upstream");

    let client = reqwest::Client::new();
    let outcome = fetch_and_cache_if_missing(&client, &storage, repo_id, &upstream, &path, None)
        .await
        .expect("cache-through succeeds");
    assert_eq!(outcome, CacheThroughOutcome::Hit);
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cache_through_fetches_upstream_on_miss_and_persists() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let path = StoragePath::from("dists/stable/Release");

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream, _server) = start_upstream_server(b"upstream", counter.clone())
        .await
        .expect("start upstream");

    let client = reqwest::Client::new();
    let outcome = fetch_and_cache_if_missing(&client, &storage, repo_id, &upstream, &path, None)
        .await
        .expect("cache-through succeeds");
    assert!(matches!(outcome, CacheThroughOutcome::Fetched(_)));
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let cached = storage
        .open_file(repo_id, &path)
        .await
        .expect("open cache")
        .expect("cache exists");
    let nr_storage::StorageFile::File { meta, content } = cached else {
        panic!("expected file");
    };
    let nr_storage::FileFileType { file_size, .. } = meta.file_type();
    let len: usize = (*file_size)
        .try_into()
        .expect("file size fits in usize for tests");
    let bytes = content.read_to_vec(len).await.expect("read cached bytes");
    assert_eq!(bytes, b"upstream");
}

#[tokio::test]
async fn cache_through_supports_by_hash_paths() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let path = StoragePath::from("dists/stable/main/binary-amd64/by-hash/SHA256/deadbeef");

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream, _server) = start_upstream_server(b"hash-bytes", counter.clone())
        .await
        .expect("start upstream");

    let client = reqwest::Client::new();
    let outcome = fetch_and_cache_if_missing(&client, &storage, repo_id, &upstream, &path, None)
        .await
        .expect("cache-through succeeds");
    assert!(matches!(outcome, CacheThroughOutcome::Fetched(_)));
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    assert!(
        storage
            .get_file_information(repo_id, &path)
            .await
            .expect("meta request")
            .is_some(),
        "expected by-hash path to be cached"
    );
}

#[tokio::test]
async fn cache_through_preserves_upstream_path_prefix() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let path = StoragePath::from("dists/stable/Release");

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream, _server) =
        start_upstream_server_with_prefix("packages", b"upstream", counter.clone())
            .await
            .expect("start upstream");

    let client = reqwest::Client::new();
    let outcome = fetch_and_cache_if_missing(&client, &storage, repo_id, &upstream, &path, None)
        .await
        .expect("cache-through succeeds");
    assert!(matches!(outcome, CacheThroughOutcome::Fetched(_)));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}
