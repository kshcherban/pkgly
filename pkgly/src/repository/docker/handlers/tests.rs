#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::test_helpers::test_storage;
use futures::stream;
use http_body_util::BodyExt;
use nr_core::storage::StoragePath;
use nr_storage::{FileContent, Storage, StorageFile, StorageFileMeta, StorageFileReader};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::tempdir;
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncReadExt, BufWriter},
    time::{Duration, sleep},
};
use uuid::Uuid;

fn test_stream_from_bytes(
    data: &[u8],
    chunk_size: usize,
) -> impl futures::Stream<Item = Result<Bytes, RepositoryHandlerError>> {
    let chunks = data
        .chunks(chunk_size)
        .map(|chunk| Bytes::copy_from_slice(chunk))
        .collect::<Vec<_>>();
    stream::iter(chunks.into_iter().map(|bytes| Ok(bytes)))
}

#[tokio::test]
async fn stream_writer_persists_full_payload() -> anyhow::Result<()> {
    let payload = (0u32..(512 * 1024))
        .map(|value| (value % 251) as u8)
        .collect::<Vec<u8>>();
    let dir = tempdir()?;
    let file_path = dir.path().join("payload.bin");
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&file_path)
        .await?;
    let mut writer = BufWriter::with_capacity(16 * 1024, file);
    let mut observed = Vec::new();
    let mut total_written = 0usize;

    stream_to_writer(
        test_stream_from_bytes(&payload, 1024),
        &mut writer,
        |chunk| {
            total_written += chunk.len();
            observed.extend_from_slice(chunk.as_ref());
            async { Ok(()) }
        },
    )
    .await?;

    drop(writer);

    let mut saved = Vec::new();
    fs::File::open(&file_path)
        .await?
        .read_to_end(&mut saved)
        .await?;

    assert_eq!(total_written, payload.len());
    assert_eq!(observed, payload);
    assert_eq!(saved, payload);

    Ok(())
}

#[tokio::test]
async fn stream_writer_handles_async_chunk_hooks() -> anyhow::Result<()> {
    let payload = vec![13u8; 128 * 1024];
    let dir = tempdir()?;
    let file_path = dir.path().join("async.bin");
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&file_path)
        .await?;
    let mut writer = BufWriter::with_capacity(8 * 1024, file);

    let processed_bytes = Arc::new(AtomicUsize::new(0));

    stream_to_writer(test_stream_from_bytes(&payload, 2048), &mut writer, {
        let processed = Arc::clone(&processed_bytes);
        move |chunk| {
            let processed = Arc::clone(&processed);
            async move {
                sleep(Duration::from_millis(1)).await;
                processed.fetch_add(chunk.len(), Ordering::SeqCst);
                Ok(())
            }
        }
    })
    .await?;

    assert_eq!(processed_bytes.load(Ordering::SeqCst), payload.len());
    Ok(())
}

#[tokio::test]
async fn streaming_finalization_matches_sha256() -> anyhow::Result<()> {
    let payload = (0u32..(256 * 1024))
        .map(|value| (value % 251) as u8)
        .collect::<Vec<u8>>();

    let dir = tempdir()?;
    let file_path = dir.path().join("stream.bin");
    std::fs::write(&file_path, &payload)?;

    let reader = StorageFileReader::from(std::fs::File::open(&file_path)?);
    let meta = StorageFileMeta::read_from_file(&file_path)?;
    let storage_file = StorageFile::File {
        meta,
        content: reader,
    };

    let finalized = recompute_finalized_upload_from_storage_file(storage_file).await?;
    assert_eq!(finalized.length as usize, payload.len());
    let expected = format!("sha256:{:x}", Sha256::digest(&payload));
    assert_eq!(finalized.digest, expected);

    Ok(())
}

#[tokio::test]
async fn catalog_collection_lists_unique_sorted_repositories() -> anyhow::Result<()> {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let manifest = br#"{
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "size": 1, "digest": "sha256:bead"},
        "layers": []
    }"#;

    for path in [
        "v2/acme/tools/api/manifests/latest",
        "v2/acme/tools/api/manifests/v1",
        "v2/acme/agent/manifests/dev",
        "v2/zz/omega/manifests/main",
    ] {
        let storage_path = StoragePath::from(path);
        storage
            .save_file(
                repository_id,
                FileContent::from(manifest.as_ref()),
                &storage_path,
            )
            .await?;
    }

    let repositories = collect_catalog_repositories(&storage, repository_id).await?;
    assert_eq!(
        repositories,
        vec![
            "acme/agent".to_string(),
            "acme/tools/api".to_string(),
            "zz/omega".to_string()
        ]
    );

    Ok(())
}

#[test]
fn docker_manifest_proxy_meta_populates_expected_fields() {
    let cache_path = StoragePath::from("v2/acme/app/manifests/latest");
    let meta = super::docker_manifest_proxy_meta(
        "acme/app",
        "acme/app",
        "latest",
        &cache_path,
        "sha256:deadbeef",
        42,
    );

    assert_eq!(
        meta.kind(),
        nr_core::repository::project::ProxyMetadataKind::ProxyArtifact
    );
    assert_eq!(meta.package_name, "acme/app");
    assert_eq!(meta.package_key, "acme/app");
    assert_eq!(meta.version.as_deref(), Some("latest"));
    assert_eq!(meta.cache_path, cache_path.to_string());
    assert_eq!(meta.upstream_digest.as_deref(), Some("sha256:deadbeef"));
    assert_eq!(meta.size, Some(42));
}

#[test]
fn catalog_pagination_respects_limit_and_last() {
    let repositories = vec![
        "alpha".to_string(),
        "bravo".to_string(),
        "charlie".to_string(),
        "delta".to_string(),
    ];
    let params = Pagination {
        limit: Some(2),
        last: Some("alpha".to_string()),
    };
    let page = paginate_lexically(&repositories, &params);

    assert_eq!(page.values, vec!["bravo", "charlie"]);
    assert_eq!(
        page.next,
        Some(PaginationCursor {
            last: "charlie".to_string(),
            limit: 2,
        })
    );
}

#[test]
fn parse_catalog_query_rejects_zero_limit() {
    let err = parse_pagination_params(Some("n=0")).expect_err("zero limit should be rejected");
    assert!(matches!(err, PaginationError::InvalidLimit));
}

#[test]
fn parse_catalog_query_extracts_values() {
    let params =
        parse_pagination_params(Some("n=5&last=acme%2Fapi")).expect("query should be parsed");
    assert_eq!(params.limit, Some(5));
    assert_eq!(params.last.as_deref(), Some("acme/api"));
}

#[test]
fn pagination_error_response_sets_docker_headers() {
    let response = PaginationError::InvalidLimit
        .into_response()
        .into_response_default();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("Docker-Distribution-API-Version")
            .and_then(|value| value.to_str().ok()),
        Some("registry/2.0")
    );
    assert_eq!(
        headers
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
}

#[test]
fn catalog_response_sets_link_header_when_cursor_present() {
    let page = PaginatedList {
        values: vec!["alpha".into()],
        next: Some(PaginationCursor {
            last: "alpha".into(),
            limit: 3,
        }),
    };
    let response = build_catalog_response(page).into_response_default();
    let headers = response.headers();
    assert_eq!(
        headers.get("Link").and_then(|value| value.to_str().ok()),
        Some("</v2/_catalog?last=alpha&n=3>; rel=\"next\"")
    );
}

#[tokio::test]
async fn catalog_response_serializes_repository_list() {
    let page = PaginatedList {
        values: vec!["alpha".into(), "beta".into()],
        next: None,
    };
    let response = build_catalog_response(page).into_response_default();
    let collected = response.into_body().collect().await.unwrap();
    let body = collected.to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload, json!({"repositories": ["alpha", "beta"]}));
}

#[tokio::test]
async fn collect_repository_tags_ignores_digest_entries() -> anyhow::Result<()> {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let manifest_bytes = br#"{"schemaVersion":2}"#;
    for tag in ["latest", "dev", "sha256:abcdef"] {
        let path = StoragePath::from(format!("v2/example/app/manifests/{tag}"));
        storage
            .save_file(
                repository_id,
                FileContent::from(manifest_bytes.as_ref()),
                &path,
            )
            .await?;
    }

    let tags = collect_repository_tags(&storage, repository_id, "example/app").await?;
    assert_eq!(tags, vec!["dev".to_string(), "latest".to_string()]);
    Ok(())
}

#[test]
fn upload_range_header_formats_values() {
    assert_eq!(upload_range_header(0), "0-0");
    assert_eq!(upload_range_header(1), "0-0");
    assert_eq!(upload_range_header(10), "0-9");
}
