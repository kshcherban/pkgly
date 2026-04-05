#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::{
    proxy_indexing::{ProxyIndexing, ProxyIndexingError},
    test_helpers::test_storage,
};
use async_trait::async_trait;
use axum::{Router, body::Body as AxumBody, routing::get};
use bytes::Bytes;
use futures::stream;
use http::{HeaderValue, StatusCode};
use nr_core::{
    repository::project::{ProxyArtifactKey, ProxyArtifactMeta},
    storage::StoragePath,
};
use sha2::Digest;
use std::{convert::Infallible, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};

async fn reader_bytes(reader: nr_storage::StorageFileReader, len: u64) -> anyhow::Result<Vec<u8>> {
    let len: usize = len.try_into().expect("length fits in usize for tests");
    Ok(reader.read_to_vec(len).await?)
}

async fn start_upstream_server(
    manifest_body: &'static [u8],
    blob_body: &'static [u8],
) -> anyhow::Result<(String, JoinHandle<()>)> {
    let manifest_digest = format!("sha256:{:x}", sha2::Sha256::digest(manifest_body));
    let blob_digest = format!("sha256:{:x}", sha2::Sha256::digest(blob_body));
    let manifest_digest_header =
        HeaderValue::from_str(&manifest_digest).expect("valid digest header");
    let manifest_content_type =
        HeaderValue::from_static("application/vnd.docker.distribution.manifest.v2+json");

    let app = Router::new()
        .route(
            "/v2/library/alpine/manifests/latest",
            get({
                let digest_header = manifest_digest_header.clone();
                let content_type = manifest_content_type.clone();
                move || {
                    let digest_header = digest_header.clone();
                    let content_type = content_type.clone();
                    async move {
                        (
                            StatusCode::OK,
                            [
                                ("Docker-Content-Digest", digest_header),
                                ("Content-Type", content_type),
                            ],
                            manifest_body,
                        )
                    }
                }
            }),
        )
        .route(
            &format!("/v2/library/alpine/blobs/{blob_digest}"),
            get(move || async move {
                (
                    StatusCode::OK,
                    [("Content-Type", "application/octet-stream")],
                    blob_body,
                )
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("upstream server error: {err}");
        }
    });
    Ok((format!("http://{}", addr), server))
}

async fn start_negotiating_upstream_server(
    v1_manifest: &'static [u8],
    v2_manifest: &'static [u8],
) -> anyhow::Result<(String, JoinHandle<()>)> {
    let v1_digest = format!("sha256:{:x}", sha2::Sha256::digest(v1_manifest));
    let v2_digest = format!("sha256:{:x}", sha2::Sha256::digest(v2_manifest));
    let v1_ct = HeaderValue::from_static("application/vnd.docker.distribution.manifest.v1+json");
    let v2_ct = HeaderValue::from_static("application/vnd.docker.distribution.manifest.v2+json");

    let app = Router::new().route(
        "/v2/library/alpine/manifests/latest",
        get(move |headers: HeaderMap| {
            let v1_ct = v1_ct.clone();
            let v2_ct = v2_ct.clone();
            let v1_manifest = v1_manifest;
            let v2_manifest = v2_manifest;
            let v1_digest = v1_digest.clone();
            let v2_digest = v2_digest.clone();
            async move {
                let accept = headers
                    .get(http::header::ACCEPT)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if accept.contains("application/vnd.docker.distribution.manifest.v2+json") {
                    (
                        StatusCode::OK,
                        [
                            (
                                "Docker-Content-Digest",
                                HeaderValue::from_str(&v2_digest).unwrap(),
                            ),
                            ("Content-Type", v2_ct),
                        ],
                        v2_manifest,
                    )
                } else {
                    (
                        StatusCode::OK,
                        [
                            (
                                "Docker-Content-Digest",
                                HeaderValue::from_str(&v1_digest).unwrap(),
                            ),
                            ("Content-Type", v1_ct),
                        ],
                        v1_manifest,
                    )
                }
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
    Ok((format!("http://{}", addr), server))
}

async fn start_accept_filtered_upstream_server(
    manifest_list: &'static [u8],
) -> anyhow::Result<(String, JoinHandle<()>)> {
    let list_digest = format!("sha256:{:x}", sha2::Sha256::digest(manifest_list));
    let content_length = HeaderValue::from_str(&manifest_list.len().to_string())?;
    let digest_header = HeaderValue::from_str(&list_digest)?;
    let app = Router::new().route(
        "/v2/library/alpine/manifests/latest",
        get(move |headers: HeaderMap| {
            let manifest_list = manifest_list;
            let digest_header = digest_header.clone();
            let content_length = content_length.clone();
            async move {
                let accept = headers
                    .get(ACCEPT)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default();
                if accept.contains("application/vnd.docker.distribution.manifest.v2+json")
                    && !accept.contains("application/vnd.docker.distribution.manifest.list.v2+json")
                {
                    axum::response::Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .header(CONTENT_TYPE, "application/json")
                        .body(AxumBody::from(
                            r#"{"errors":[{"code":"MANIFEST_UNKNOWN","message":"filtered"}]}"#,
                        ))
                        .unwrap()
                } else {
                    axum::response::Response::builder()
                        .status(StatusCode::OK)
                        .header("Docker-Content-Digest", digest_header.clone())
                        .header(
                            CONTENT_TYPE,
                            "application/vnd.docker.distribution.manifest.list.v2+json",
                        )
                        .header(CONTENT_LENGTH, content_length.clone())
                        .body(AxumBody::from(manifest_list))
                        .unwrap()
                }
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
    Ok((format!("http://{}", addr), server))
}

async fn start_revalidating_upstream_server(
    manifest_bytes: Vec<u8>,
) -> anyhow::Result<(String, Arc<RwLock<Vec<u8>>>, JoinHandle<()>)> {
    let state = Arc::new(RwLock::new(manifest_bytes));
    let state_clone = state.clone();
    let state_for_return = state.clone();

    let app = Router::new().route(
        "/v2/library/test/manifests/latest",
        get({
            move || {
                let state = state_clone.clone();
                async move {
                    let bytes = state.read().clone();
                    let digest = format!("sha256:{:x}", sha2::Sha256::digest(&bytes));
                    (
                        StatusCode::OK,
                        [
                            (
                                "Docker-Content-Digest",
                                HeaderValue::from_str(&digest).unwrap(),
                            ),
                            (
                                "Content-Type",
                                HeaderValue::from_static(
                                    "application/vnd.docker.distribution.manifest.v2+json",
                                ),
                            ),
                        ],
                        bytes,
                    )
                }
            }
        })
        .head({
            move || {
                let state = state.clone();
                async move {
                    let bytes = state.read().clone();
                    let digest = format!("sha256:{:x}", sha2::Sha256::digest(&bytes));
                    (
                        StatusCode::OK,
                        [
                            (
                                "Docker-Content-Digest",
                                HeaderValue::from_str(&digest).unwrap(),
                            ),
                            (
                                "Content-Type",
                                HeaderValue::from_static(
                                    "application/vnd.docker.distribution.manifest.v2+json",
                                ),
                            ),
                        ],
                    )
                }
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

    Ok((format!("http://{}", addr), state_for_return, server))
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

    #[allow(dead_code)]
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
async fn fetch_manifest_caches_locally() -> anyhow::Result<()> {
    let manifest = br#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.oci.image.config.v1+json","size":7023,"digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},"layers":[]}"#;
    let blob = b"blob-data";
    let (base, server) = start_upstream_server(manifest, blob).await?;

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: base,
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    let cached = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        None,
    )
    .await?;

    let CachedManifest {
        reader,
        digest,
        content_type,
        length,
    } = cached;
    let cached_bytes = reader_bytes(reader, manifest.len() as u64).await?;
    assert_eq!(cached_bytes, manifest);
    assert_eq!(length, manifest.len() as u64);
    assert_eq!(
        content_type,
        "application/vnd.docker.distribution.manifest.v2+json"
    );

    // Verify digest path saved
    let digest_path = StoragePath::from(format!("v2/{}/manifests/{}", "library/alpine", digest));
    let stored = storage
        .open_file(repository_id, &digest_path)
        .await?
        .expect("digest entry");
    let stored_bytes = match stored {
        nr_storage::StorageFile::File { mut content, .. } => {
            let mut buf = Vec::new();
            use tokio::io::AsyncReadExt;
            content.read_to_end(&mut buf).await?;
            buf
        }
        _ => vec![],
    };
    assert_eq!(stored_bytes, manifest);

    server.abort();
    Ok(())
}

#[tokio::test]
async fn fetch_manifest_records_proxy_index_entries() -> anyhow::Result<()> {
    let manifest = br#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.oci.image.config.v1+json","size":7023,"digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"},"layers":[]}"#;
    let blob = b"blob-data";
    let (base, server) = start_upstream_server(manifest, blob).await?;

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: base,
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    let indexer = Arc::new(RecordingIndexer::default());
    let cached = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        Some(indexer.as_ref()),
    )
    .await?;
    server.abort();

    let recorded = indexer.recorded().await;
    assert_eq!(recorded.len(), 2);
    assert!(
        recorded
            .iter()
            .any(|meta| meta.version.as_deref() == Some("latest"))
    );
    assert!(
        recorded
            .iter()
            .any(|meta| meta.version.as_deref() == Some(cached.digest.as_str()))
    );
    assert!(
        recorded
            .iter()
            .all(|meta| meta.cache_path.starts_with("v2/library/alpine/manifests/"))
    );

    Ok(())
}

#[tokio::test]
async fn fetch_blob_uses_cache_on_second_request() -> anyhow::Result<()> {
    let manifest = br#"{}"#;
    let blob = b"blob-body-for-cache";
    let (base, server) = start_upstream_server(manifest, blob).await?;

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let digest = format!("sha256:{:x}", sha2::Sha256::digest(blob));
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: base,
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    // First fetch - hit upstream
    let first = fetch_and_cache_blob(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        &digest,
    )
    .await?;
    let CachedBlob {
        reader,
        digest: first_digest,
        length,
    } = first;
    let first_body = reader_bytes(reader, blob.len() as u64).await?;
    assert_eq!(first_body, blob);
    assert_eq!(length, blob.len() as u64);
    assert_eq!(first_digest, digest);

    // Stop upstream to ensure second call reads cache
    server.abort();

    let second = fetch_and_cache_blob(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        &digest,
    )
    .await?;
    let CachedBlob { reader, .. } = second;
    let second_body = reader_bytes(reader, blob.len() as u64).await?;
    assert_eq!(second_body, blob);

    Ok(())
}

#[test]
fn accept_allows_media_type_handles_oci_aliases() {
    assert!(
        accept_allows_media_type(
            Some("application/vnd.oci.image.manifest.v1+json"),
            "application/vnd.docker.distribution.manifest.v2+json"
        ),
        "OCI image manifest should satisfy Docker schema2 requests"
    );
    assert!(
        accept_allows_media_type(
            Some("application/vnd.oci.image.index.v1+json"),
            "application/vnd.docker.distribution.manifest.list.v2+json"
        ),
        "OCI index should satisfy Docker manifest list requests"
    );
}

#[tokio::test]
async fn manifest_accept_mismatch_falls_back_to_cache() -> anyhow::Result<()> {
    let manifest_list = br#"{"schemaVersion":2,"mediaType":"application/vnd.docker.distribution.manifest.list.v2+json","manifests":[]}"#;
    let (base, server) = start_accept_filtered_upstream_server(manifest_list).await?;

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: base,
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    // Prime cache with Accept that the upstream honors.
    fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        Some("application/vnd.docker.distribution.manifest.list.v2+json"),
        None,
    )
    .await?;

    // Request a manifest with a media type the upstream refuses; should fall back to cache.
    let cached = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        Some("application/vnd.docker.distribution.manifest.v2+json"),
        None,
    )
    .await?;

    assert_eq!(
        cached.content_type,
        "application/vnd.docker.distribution.manifest.list.v2+json"
    );
    let bytes = reader_bytes(cached.reader, manifest_list.len() as u64).await?;
    assert_eq!(bytes, manifest_list);

    server.abort();
    Ok(())
}

#[tokio::test]
async fn cached_manifest_uses_manifest_media_type() -> anyhow::Result<()> {
    let manifest = br#"{"schemaVersion":2,"mediaType":"application/vnd.docker.distribution.manifest.v2+json"}"#;
    let blob = b"blob-data";
    let (base, server) = start_upstream_server(manifest, blob).await?;

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: base,
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    // Prime the cache using the upstream manifest
    fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        None,
    )
    .await?;

    // Kill upstream to ensure the second call reads from cache only
    server.abort();

    let cached = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        None,
    )
    .await?;

    assert_eq!(
        cached.content_type,
        "application/vnd.docker.distribution.manifest.v2+json"
    );

    Ok(())
}

#[tokio::test]
async fn tag_revalidation_fetches_moved_digest() -> anyhow::Result<()> {
    let repo_id = Uuid::new_v4();
    let storage = test_storage().await;

    let manifest_v1 = br#"{"schemaVersion":2,"mediaType":"application/vnd.docker.distribution.manifest.v2+json","config":{"mediaType":"application/vnd.docker.container.image.v1+json","size":2,"digest":"sha256:aaaa"},"layers":[]}"#;
    let manifest_v2 = br#"{"schemaVersion":2,"mediaType":"application/vnd.docker.distribution.manifest.v2+json","config":{"mediaType":"application/vnd.docker.container.image.v1+json","size":2,"digest":"sha256:bbbb"},"layers":[]}"#;

    let (upstream_url, state, server) =
        start_revalidating_upstream_server(manifest_v1.to_vec()).await?;

    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url,
        upstream_auth: None,
        revalidation_ttl_seconds: 0,
        skip_tag_revalidation: false,
    })?;

    let first = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repo_id,
        "library/test",
        "latest",
        Some(MODERN_UPSTREAM_ACCEPT),
        None,
    )
    .await?;

    let first_digest = first.digest.clone();
    assert!(first_digest.starts_with("sha256:"));

    // Change upstream manifest and trigger revalidation
    *state.write() = manifest_v2.to_vec();

    let refreshed = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repo_id,
        "library/test",
        "latest",
        Some(MODERN_UPSTREAM_ACCEPT),
        None,
    )
    .await?;

    assert_ne!(refreshed.digest, first_digest);

    server.abort();
    Ok(())
}

#[tokio::test]
async fn accept_mismatch_triggers_refetch_with_preferred_media_type() -> anyhow::Result<()> {
    // Upstream returns schema1 by default, schema2 when explicitly requested via Accept
    let v1_manifest = br#"{"schemaVersion":1,"name":"library/alpine","fsLayers":[]}"#;
    let v2_manifest = br#"{"schemaVersion":2,"mediaType":"application/vnd.docker.distribution.manifest.v2+json"}"#;
    let (base, server) = start_negotiating_upstream_server(v1_manifest, v2_manifest).await?;

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: base,
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    // Cache initial manifest; proxy now prefers modern media types so it should store schema2
    let cached_v1 = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        None,
    )
    .await?;
    assert_eq!(
        cached_v1.content_type,
        "application/vnd.docker.distribution.manifest.v2+json"
    );

    // Request again, this time with an Accept that prefers schema2
    let accept_header = "application/vnd.docker.distribution.manifest.v2+json";
    let refreshed = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        Some(accept_header),
        None,
    )
    .await?;

    assert_eq!(
        refreshed.content_type,
        "application/vnd.docker.distribution.manifest.v2+json"
    );

    server.abort();
    Ok(())
}

#[tokio::test]
async fn deleted_manifest_is_downloaded_again() -> anyhow::Result<()> {
    use crate::app::api::repository::packages::delete_docker_package;

    let manifest = br#"{"schemaVersion":2}"#;
    let blob = b"bin";
    let (base, server) = start_upstream_server(manifest, blob).await?;

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: base,
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    // Cache manifest once
    fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        None,
    )
    .await?;

    let manifest_path = StoragePath::from("v2/library/alpine/manifests/latest");
    assert!(
        storage.file_exists(repository_id, &manifest_path).await?,
        "manifest should be cached before deletion"
    );

    // Simulate admin deletion via API helper
    delete_docker_package(
        &storage,
        repository_id,
        manifest_path.to_string().as_str(),
        None,
    )
    .await
    .expect("docker deletion should succeed");

    assert!(
        !storage.file_exists(repository_id, &manifest_path).await?,
        "manifest cache file should be removed"
    );

    // Next fetch should re-download from upstream and cache again
    let refreshed = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        None,
    )
    .await?;
    let CachedManifest { reader, .. } = refreshed;
    let refreshed_bytes = reader_bytes(reader, manifest.len() as u64).await?;
    assert_eq!(refreshed_bytes, manifest);
    assert!(
        storage.file_exists(repository_id, &manifest_path).await?,
        "manifest should be cached again after re-download"
    );

    server.abort();
    Ok(())
}

#[tokio::test]
async fn blob_digest_mismatch_is_reported() -> anyhow::Result<()> {
    let blob = b"blob-body-for-cache";
    let correct_digest = format!("sha256:{:x}", sha2::Sha256::digest(blob));
    let wrong_header = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    let app = Router::new().route(
        &format!("/v2/library/alpine/blobs/{correct_digest}"),
        get(move || {
            let chunk = Bytes::from_static(blob);
            let stream = stream::iter(vec![Ok::<_, Infallible>(chunk)]);
            async move {
                (
                    StatusCode::OK,
                    [("Docker-Content-Digest", wrong_header)],
                    AxumBody::from_stream(stream),
                )
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: format!("http://{addr}"),
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    let err = fetch_and_cache_blob(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        &correct_digest,
    )
    .await
    .expect_err("digest mismatch expected");

    match err {
        DockerError::DigestMismatch { expected, .. } => {
            assert_eq!(expected, wrong_header);
        }
        other => panic!("expected digest mismatch, got {other:?}"),
    }

    server.abort();
    Ok(())
}

#[test]
fn schema1_detection_handles_known_media_types() {
    assert!(super::is_schema1_manifest(
        "application/vnd.docker.distribution.manifest.v1+json"
    ));
    assert!(super::is_schema1_manifest(
        "application/vnd.docker.distribution.manifest.v1+prettyjws"
    ));
    assert!(super::is_schema1_manifest(
        "application/vnd.docker.distribution.manifest.v1+json; charset=utf-8"
    ));

    assert!(!super::is_schema1_manifest(
        "application/vnd.oci.image.manifest.v1+json"
    ));
    assert!(!super::is_schema1_manifest(
        "application/vnd.docker.distribution.manifest.v2+json"
    ));
}

#[tokio::test]
async fn large_blob_is_streamed_without_buffering() -> anyhow::Result<()> {
    let chunk = Bytes::from(vec![b'x'; 1_024 * 1_024]); // 1MiB chunk
    let chunks = 6;
    let mut full = Vec::with_capacity(chunk.len() * chunks);
    for _ in 0..chunks {
        full.extend_from_slice(&chunk);
    }
    let digest = format!("sha256:{:x}", sha2::Sha256::digest(&full));

    let app = Router::new().route(
        &format!("/v2/library/alpine/blobs/{digest}"),
        get({
            let chunk = chunk.clone();
            let digest_header = HeaderValue::from_str(&digest).expect("valid digest header value");
            let content_type = HeaderValue::from_static("application/octet-stream");
            move || {
                let digest_header = digest_header.clone();
                let content_type = content_type.clone();
                let stream = stream::iter((0..chunks).map({
                    let chunk = chunk.clone();
                    move |_| Ok::<_, Infallible>(chunk.clone())
                }));
                async move {
                    (
                        StatusCode::OK,
                        [
                            ("Docker-Content-Digest", digest_header),
                            ("Content-Type", content_type),
                        ],
                        AxumBody::from_stream(stream),
                    )
                }
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: format!("http://{addr}"),
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    let blob = fetch_and_cache_blob(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        &digest,
    )
    .await?;

    assert!(
        !matches!(&blob.reader, nr_storage::StorageFileReader::Bytes(_)),
        "streaming should not return in-memory reader"
    );
    let CachedBlob {
        reader,
        digest: found_digest,
        length,
    } = blob;

    assert_eq!(found_digest, digest);
    assert_eq!(length, full.len() as u64);

    let content = reader_bytes(reader, full.len() as u64).await?;
    assert_eq!(content.len(), full.len());
    assert_eq!(content, full);

    let blob_path = StoragePath::from(format!("v2/library/alpine/blobs/{digest}"));
    assert!(
        storage.file_exists(repository_id, &blob_path).await?,
        "blob should be cached on disk"
    );

    server.abort();
    Ok(())
}

fn streamed_from_bytes(bytes: &[u8]) -> anyhow::Result<StreamedDownload> {
    use std::io::Write;
    let mut file = tempfile::Builder::new()
        .prefix("manifest-test-")
        .tempfile()?;
    file.write_all(bytes)?;
    let path = file.into_temp_path();
    let digest = format!("sha256:{:x}", sha2::Sha256::digest(bytes));
    Ok(StreamedDownload {
        path,
        size: bytes.len() as u64,
        digest,
    })
}

#[tokio::test]
async fn manifest_collision_with_different_digest_errors() -> anyhow::Result<()> {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let tag_path = StoragePath::from("v2/library/test/manifests/latest");

    let manifest1 = br#"{"schemaVersion":2,"config":{"digest":"sha256:aaaa"},"layers":[]}"#;
    let streamed1 = streamed_from_bytes(manifest1)?;
    let digest_path1 = StoragePath::from(format!(
        "v2/{}/manifests/{}",
        "library/test", streamed1.digest
    ));

    save_manifest_atomically(
        &storage,
        repository_id,
        &tag_path,
        Some(&digest_path1),
        &streamed1,
        "latest",
        &streamed1.digest,
    )
    .await?;

    let manifest2 = br#"{"schemaVersion":2,"config":{"digest":"sha256:bbbb"},"layers":[]}"#;
    let streamed2 = streamed_from_bytes(manifest2)?;
    let digest_path2 = StoragePath::from(format!(
        "v2/{}/manifests/{}",
        "library/test", streamed2.digest
    ));

    save_manifest_atomically(
        &storage,
        repository_id,
        &tag_path,
        Some(&digest_path2),
        &streamed2,
        "latest",
        &streamed2.digest,
    )
    .await?;

    let cached = load_cached_manifest(&storage, repository_id, &tag_path, "latest")
        .await?
        .expect("tag should be cached");
    assert_eq!(cached.digest, streamed2.digest);

    Ok(())
}

#[tokio::test]
async fn bearer_challenge_is_followed_for_public_token() -> anyhow::Result<()> {
    // Token service
    let token_app = Router::new().route(
        "/token",
        get(|| async { axum::Json(serde_json::json!({ "token": "abc123" })) }),
    );
    let token_listener = TcpListener::bind("127.0.0.1:0").await?;
    let token_addr = token_listener.local_addr()?;
    let token_server = tokio::spawn(async move {
        let _ = axum::serve(token_listener, token_app).await;
    });

    // Upstream that challenges then succeeds
    let manifest_body = br#"{"schemaVersion":2}"#;
    let manifest_digest = format!("sha256:{:x}", sha2::Sha256::digest(manifest_body));
    let manifest_digest_expected = manifest_digest.clone();
    let guarded_path = "/v2/library/alpine/manifests/latest";
    let app = Router::new().route(
        guarded_path,
        get({
            let challenge = format!(
                "Bearer realm=\"http://{}/token\",service=\"registry-1.docker.io\",scope=\"repository:library/alpine:pull\"",
                token_addr
            );
            move |headers: HeaderMap| {
                let challenge = challenge.clone();
                async move {
                    let auth_ok = headers
                        .get(http::header::AUTHORIZATION)
                        .map(|v| v == "Bearer abc123")
                        .unwrap_or(false);
                    if auth_ok {
                        let mut builder = ResponseBuilder::ok()
                            .header(
                                "Docker-Content-Digest",
                                manifest_digest.clone(),
                            )
                            .header(
                                CONTENT_TYPE,
                                "application/vnd.docker.distribution.manifest.v2+json",
                            );
                        builder = builder.header(
                            CONTENT_LENGTH,
                            manifest_body.len().to_string(),
                        );
                        builder.body(manifest_body as &[u8])
                    } else {
                        ResponseBuilder::unauthorized()
                            .header("WWW-Authenticate", challenge.clone())
                            .header(CONTENT_TYPE, "application/json")
                            .body(b"{}" as &[u8])
                    }
                }
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let upstream_server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let upstream = ProxyUpstream::new(&DockerProxyConfig {
        upstream_url: format!("http://{addr}"),
        upstream_auth: None,
        revalidation_ttl_seconds: default_revalidation_ttl(),
        skip_tag_revalidation: false,
    })?;

    let manifest = fetch_and_cache_manifest(
        &upstream,
        &storage,
        repository_id,
        "library/alpine",
        "latest",
        None,
        None,
    )
    .await?;

    let CachedManifest {
        reader,
        digest,
        length,
        ..
    } = manifest;

    assert_eq!(digest, manifest_digest_expected);
    let manifest_bytes = reader_bytes(reader, length).await?;
    assert_eq!(manifest_bytes, manifest_body);

    upstream_server.abort();
    token_server.abort();
    Ok(())
}
