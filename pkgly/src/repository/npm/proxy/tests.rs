#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::proxy_indexing::{ProxyIndexing, ProxyIndexingError};
use crate::repository::test_helpers::test_storage;
use async_trait::async_trait;
use axum::body::to_bytes;
use bytes::Bytes;
use http::Request;
use nr_core::repository::project::{ProxyArtifactKey, ProxyArtifactMeta};
use nr_storage::{FileContent, StorageFile};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::Mutex;
use uuid::Uuid;

#[test]
fn cache_path_for_scoped_package() {
    let path = StoragePath::from("@scope/package/-/package-1.0.0.tgz");
    let cache = cache_path_for_npm_proxy(&path).expect("cache path");
    assert_eq!(
        cache.to_string(),
        "packages/@scope/package/package-1.0.0.tgz"
    );
}

#[test]
fn cache_path_for_unscoped_package() {
    let path = StoragePath::from("left-pad/-/left-pad-1.3.0.tgz");
    let cache = cache_path_for_npm_proxy(&path).expect("cache path");
    assert_eq!(cache.to_string(), "packages/left-pad/left-pad-1.3.0.tgz");
}

#[test]
fn cache_path_requires_tarball_segment() {
    let path = StoragePath::from("left-pad/latest");
    assert!(cache_path_for_npm_proxy(&path).is_none());
}

#[test]
fn metadata_cache_path_is_separate_from_scoped_package_paths() {
    let path = StoragePath::from("@babel/code-frame");
    let cache = metadata_cache_path_for_npm_proxy(&path);

    assert_eq!(
        cache.to_string(),
        "metadata-v2/packages/@babel/code-frame/document.json"
    );
}

#[test]
fn metadata_cache_path_uses_a_file_for_the_registry_root() {
    let cache = metadata_cache_path_for_npm_proxy(&StoragePath::default());

    assert_eq!(cache.to_string(), "metadata-v2/root/document.json");
}

#[test]
fn strip_suffix_ignore_ascii_case_rejects_non_utf8_boundaries() {
    assert_eq!(strip_suffix_ignore_ascii_case("é", "x"), None);
}

#[test]
fn normalize_routes_adds_default_when_empty() {
    let routes = normalize_routes(Vec::new());
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0], DEFAULT_ROUTE.clone());
}

#[test]
fn normalize_routes_keeps_existing_entries() {
    let custom = NpmProxyRoute {
        url: ProxyURL::try_from(String::from("https://mirror.npm.internal")).expect("valid url"),
        name: Some("mirror".to_string()),
    };
    let routes = normalize_routes(vec![custom.clone()]);
    assert_eq!(routes, vec![custom]);
}

fn npm_tarball_path() -> StoragePath {
    StoragePath::from("packages/@scope/package/package-1.2.3.tgz")
}

#[test]
fn npm_proxy_meta_from_cache_path_parses_scoped_tarball() {
    let path = npm_tarball_path();
    let meta = super::npm_proxy_meta_from_cache_path(&path, 4096, None).expect("metadata");
    assert_eq!(meta.package_name, "@scope/package");
    assert_eq!(meta.package_key, "@scope/package");
    assert_eq!(meta.version.as_deref(), Some("1.2.3"));
    assert_eq!(meta.cache_path, path.to_string());
    assert_eq!(meta.size, Some(4096));
}

#[test]
fn npm_proxy_meta_from_cache_path_handles_unscoped_tarball() {
    let path = StoragePath::from("packages/left-pad/left-pad-1.3.0.tgz");
    let meta = super::npm_proxy_meta_from_cache_path(&path, 1024, None).expect("metadata");
    assert_eq!(meta.package_name, "left-pad");
    assert_eq!(meta.package_key, "left-pad");
    assert_eq!(meta.version.as_deref(), Some("1.3.0"));
}

#[test]
fn npm_proxy_key_from_cache_path_parses_scoped_tarball() {
    let path = npm_tarball_path();
    let key = super::npm_proxy_key_from_cache_path(&path).expect("key");
    assert_eq!(key.package_key, "@scope/package");
    assert_eq!(key.version.as_deref(), Some("1.2.3"));
    assert_eq!(key.cache_path, Some(path.to_string()));
}

#[tokio::test]
async fn metadata_cache_hit_indexes_versions_without_prefetching_tarballs() {
    let metadata = r#"{
        "name": "left-pad",
        "versions": {
            "1.0.0": {
                "version": "1.0.0",
                "dist": {
                    "tarball": "https://registry.npmjs.org/left-pad/-/left-pad-1.0.0.tgz",
                    "integrity": "sha512-deadbeef"
                }
            },
            "2.0.0": {
                "version": "2.0.0",
                "dist": {
                    "tarball": "https://registry.npmjs.org/left-pad/-/left-pad-2.0.0.tgz"
                }
            }
        }
    }"#;

    let indexer = RecordingIndexer::default();

    super::record_npm_metadata_cache_hit(&indexer, metadata.as_bytes())
        .await
        .expect("metadata indexed");

    let mut recorded = indexer.recorded().await;
    recorded.sort_by(|a, b| a.version.cmp(&b.version));

    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[0].package_key, "left-pad");
    assert_eq!(recorded[0].version.as_deref(), Some("1.0.0"));
    assert_eq!(
        recorded[0].cache_path,
        "packages/left-pad/left-pad-1.0.0.tgz"
    );
    assert_eq!(
        recorded[0].upstream_url.as_deref(),
        Some("https://registry.npmjs.org/left-pad/-/left-pad-1.0.0.tgz")
    );
    assert_eq!(
        recorded[0].upstream_digest.as_deref(),
        Some("sha512-deadbeef")
    );

    assert_eq!(recorded[1].version.as_deref(), Some("2.0.0"));
    assert_eq!(
        recorded[1].cache_path,
        "packages/left-pad/left-pad-2.0.0.tgz"
    );

    // Tarballs are *not* prefetched from metadata; only indexed.
}

#[tokio::test]
async fn metadata_tarball_urls_rewritten_to_proxy_base() {
    let (parts, _) = Request::builder()
        .uri("https://pkgly.test/repositories/abc/npm-proxy/@scope/pkg")
        .header(http::header::HOST, "pkgly.test")
        .body(())
        .unwrap()
        .into_parts();

    let body = r#"{
        "name": "@scope/pkg",
        "versions": {
            "1.0.0": { "dist": { "tarball": "https://registry.npmjs.org/@scope/pkg/-/pkg-1.0.0.tgz" } }
        }
    }"#;

    let tempdir = tempdir().unwrap();
    let meta_path = tempdir.path().join("package.json");
    std::fs::write(&meta_path, body).unwrap();
    let meta = nr_storage::StorageFileMeta::read_from_file(&meta_path).unwrap();

    let file = StorageFile::File {
        meta,
        content: nr_storage::StorageFileReader::Bytes(nr_storage::FileContentBytes::Bytes(
            Bytes::from(body),
        )),
    };

    let path = StoragePath::from("@scope/pkg");
    let response = super::rewrite_metadata_tarballs(&parts, &path, file)
        .await
        .expect("rewrite works")
        .expect("metadata response");

    let RepoResponse::Other(resp) = response else {
        panic!("expected Other response");
    };

    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let rewritten: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let tarball = rewritten["versions"]["1.0.0"]["dist"]["tarball"]
        .as_str()
        .unwrap();
    assert_eq!(
        tarball,
        "https://pkgly.test/repositories/abc/npm-proxy/@scope/pkg/-/pkg-1.0.0.tgz"
    );
}

#[tokio::test]
async fn metadata_tarball_urls_rewritten_for_yarn_escaped_scoped_request() {
    let (parts, _) = Request::builder()
        .uri("https://pkgly.test/abc/npm-proxy/@scope%2fpkg")
        .header(http::header::HOST, "pkgly.test")
        .body(())
        .unwrap()
        .into_parts();

    let body = r#"{
        "name": "@scope/pkg",
        "versions": {
            "1.0.0": { "dist": { "tarball": "https://registry.npmjs.org/@scope/pkg/-/pkg-1.0.0.tgz" } }
        }
    }"#;

    let tempdir = tempdir().unwrap();
    let meta_path = tempdir.path().join("package.json");
    std::fs::write(&meta_path, body).unwrap();
    let meta = nr_storage::StorageFileMeta::read_from_file(&meta_path).unwrap();

    let file = StorageFile::File {
        meta,
        content: nr_storage::StorageFileReader::Bytes(nr_storage::FileContentBytes::Bytes(
            Bytes::from(body),
        )),
    };

    let path = StoragePath::from("@scope/pkg");
    let response = super::rewrite_metadata_tarballs(&parts, &path, file)
        .await
        .expect("rewrite works")
        .expect("metadata response");

    let RepoResponse::Other(response) = response else {
        panic!("expected Other response");
    };
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let rewritten: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(
        rewritten["versions"]["1.0.0"]["dist"]["tarball"].as_str(),
        Some("https://pkgly.test/repositories/abc/npm-proxy/@scope/pkg/-/pkg-1.0.0.tgz")
    );
}

#[tokio::test]
async fn serve_cached_response_rewrites_metadata_tarballs() {
    let repository_id = Uuid::new_v4();
    let storage = test_storage().await;
    let path = StoragePath::from("@scope/pkg");
    let cache_path = metadata_cache_path_for_npm_proxy(&path);
    let metadata = br#"{
        "name": "@scope/pkg",
        "versions": {
            "1.0.0": { "dist": { "tarball": "https://registry.npmjs.org/@scope/pkg/-/pkg-1.0.0.tgz" } }
        }
    }"#;

    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(metadata)),
            &cache_path,
        )
        .await
        .expect("write metadata");

    let (parts, _) = Request::builder()
        .uri("https://pkgly.test/repositories/abc/npm-proxy/@scope/pkg")
        .header(http::header::HOST, "pkgly.test")
        .body(())
        .unwrap()
        .into_parts();

    let response =
        super::serve_cached_response(&parts, &storage, repository_id, &path, &cache_path, true)
            .await
            .expect("rewrite succeeds")
            .expect("response exists");

    let RepoResponse::Other(resp) = response else {
        panic!("expected Other response");
    };

    let body = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read rewritten body");
    let rewritten: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let tarball = rewritten["versions"]["1.0.0"]["dist"]["tarball"]
        .as_str()
        .expect("tarball string");

    assert_eq!(
        tarball,
        "https://pkgly.test/repositories/abc/npm-proxy/@scope/pkg/-/pkg-1.0.0.tgz"
    );
}

#[tokio::test]
async fn serve_cached_response_falls_back_to_legacy_metadata_cache() {
    let repository_id = Uuid::new_v4();
    let storage = test_storage().await;
    let path = StoragePath::from("@scope/pkg");
    let cache_path = metadata_cache_path_for_npm_proxy(&path);
    let metadata = br#"{
        "name": "@scope/pkg",
        "versions": {
            "1.0.0": { "dist": { "tarball": "https://registry.npmjs.org/@scope/pkg/-/pkg-1.0.0.tgz" } }
        }
    }"#;

    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(metadata)),
            &path,
        )
        .await
        .expect("write legacy metadata");

    let (parts, _) = Request::builder()
        .uri("https://pkgly.test/repositories/abc/npm-proxy/@scope/pkg")
        .header(http::header::HOST, "pkgly.test")
        .body(())
        .unwrap()
        .into_parts();

    let response =
        super::serve_cached_response(&parts, &storage, repository_id, &path, &cache_path, true)
            .await
            .expect("cache lookup succeeds")
            .expect("legacy cache response exists");

    let RepoResponse::Other(response) = response else {
        panic!("expected rewritten metadata response");
    };
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read rewritten body");
    let rewritten: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        rewritten["versions"]["1.0.0"]["dist"]["tarball"].as_str(),
        Some("https://pkgly.test/repositories/abc/npm-proxy/@scope/pkg/-/pkg-1.0.0.tgz")
    );
}

#[tokio::test]
async fn cached_file_information_falls_back_to_legacy_metadata_cache() {
    let repository_id = Uuid::new_v4();
    let storage = test_storage().await;
    let path = StoragePath::from("@scope/pkg");
    let cache_path = metadata_cache_path_for_npm_proxy(&path);

    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(b"legacy metadata")),
            &path,
        )
        .await
        .expect("write legacy metadata");

    let metadata =
        super::cached_file_information(&storage, repository_id, &cache_path, Some(&path))
            .await
            .expect("cache lookup succeeds")
            .expect("legacy metadata exists");

    assert_eq!(metadata.name(), "pkg");
}

#[tokio::test]
async fn serve_cached_response_ignores_a_directory_at_cache_path() {
    let repository_id = Uuid::new_v4();
    let storage = test_storage().await;
    let path = StoragePath::from("@scope/pkg");
    let cache_path = metadata_cache_path_for_npm_proxy(&path);
    let nested_path = cache_path.clone().push("unexpected.json");

    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(b"unexpected")),
            &nested_path,
        )
        .await
        .expect("seed malformed cache directory");

    let (parts, _) = Request::builder()
        .uri("https://pkgly.test/repositories/abc/npm-proxy/@scope%2fpkg")
        .body(())
        .unwrap()
        .into_parts();

    let response =
        super::serve_cached_response(&parts, &storage, repository_id, &path, &cache_path, true)
            .await
            .expect("cache lookup succeeds");

    assert!(response.is_none());
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
        _key: ProxyArtifactKey,
    ) -> Result<(), ProxyIndexingError> {
        Ok(())
    }
}

#[test]
fn prerelease_cache_index_keeps_the_complete_version() {
    let path = StoragePath::from("packages/@scope/my-lib/my-lib-1.2.3-beta.1+build.4.tgz");
    let key = super::npm_proxy_key_from_cache_path(&path).expect("prerelease cache key");
    assert_eq!(key.package_key, "@scope/my-lib");
    assert_eq!(key.version.as_deref(), Some("1.2.3-beta.1+build.4"));
}

#[test]
fn percent_escape_case_keeps_the_same_registry_base() {
    for path in [
        "/Store/Npm/@scope%2fpkg",
        "/Store/Npm/@scope%2Fpkg",
        "/Store/Npm/@scope/pkg",
        "/Store/Npm/%40scope%2Fpkg",
        "/Store/Npm/%40scope/pkg",
    ] {
        assert_eq!(
            super::repository_base_path(path, &StoragePath::from("@scope/pkg")),
            Some("/repositories/Store/Npm".into())
        );
    }
}

#[tokio::test]
async fn metadata_tarballs_preserve_https_behind_a_reverse_proxy() {
    let (parts, _) = Request::builder()
        .uri("/repositories/store/npm/demo")
        .header(http::header::HOST, "registry.test")
        .header("x-forwarded-proto", "https")
        .body(())
        .unwrap()
        .into_parts();
    let body = r#"{"versions":{"1.0.0":{"dist":{"tarball":"https://registry.npmjs.org/demo/-/demo-1.0.0.tgz"}}}}"#;
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("metadata.json");
    std::fs::write(&file_path, body).unwrap();
    let file = StorageFile::File {
        meta: nr_storage::StorageFileMeta::read_from_file(&file_path).unwrap(),
        content: nr_storage::StorageFileReader::Bytes(nr_storage::FileContentBytes::Bytes(
            Bytes::from_static(body.as_bytes()),
        )),
    };
    let response = super::rewrite_metadata_tarballs(&parts, &StoragePath::from("demo"), file)
        .await
        .unwrap()
        .unwrap();
    let RepoResponse::Other(response) = response else {
        panic!("expected metadata")
    };
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value["versions"]["1.0.0"]["dist"]["tarball"],
        "https://registry.test/repositories/store/npm/demo/-/demo-1.0.0.tgz"
    );
}

#[test]
fn proxy_cache_keys_preserve_prerelease_and_build_case() {
    for package in ["my-hyphenated-lib", "@scope/my-hyphenated-lib"] {
        for version in ["1.2.3-RC.1+Build.X", "1.2.3-rc.1+build.x"] {
            let name = package.rsplit('/').next().unwrap();
            let path = StoragePath::from(format!("packages/{package}/{name}-{version}.tgz"));
            let key = super::npm_proxy_key_from_cache_path(&path).unwrap();
            assert_eq!(key.package_key, package);
            assert_eq!(key.version.as_deref(), Some(version));
        }
    }
}

#[test]
fn metadata_cache_files_never_block_other_package_or_version_paths() {
    for (first, second) in [
        ("demo", "demo/1.0.0-beta.1"),
        ("demo", "demo.json/1.0.0-beta.1"),
        ("@scope/demo", "@scope/demo/1.0.0-beta.1"),
    ] {
        let first = metadata_cache_path_for_npm_proxy(&StoragePath::from(first)).to_string();
        let second = metadata_cache_path_for_npm_proxy(&StoragePath::from(second)).to_string();
        assert!(
            !second.starts_with(&format!("{first}/")),
            "{first} blocks {second}"
        );
        assert_ne!(first, second);
    }
}

#[tokio::test]
async fn exact_version_metadata_rewrites_its_tarball_url() {
    let (parts, _) = Request::builder()
        .uri("https://pkgly.test/repositories/store/npm-proxy/@scope/pkg/1.2.3-rc.1+Build.7")
        .header(http::header::HOST, "pkgly.test")
        .body(())
        .unwrap()
        .into_parts();
    let body = br#"{"name":"@scope/pkg","version":"1.2.3-rc.1+Build.7","dist":{"tarball":"https://upstream.test/@scope/pkg/-/pkg-1.2.3-rc.1+Build.7.tgz"}}"#;
    let root = tempdir().unwrap();
    let path = root.path().join("manifest.json");
    std::fs::write(&path, body).unwrap();
    let file = StorageFile::File {
        meta: nr_storage::StorageFileMeta::read_from_file(&path).unwrap(),
        content: nr_storage::StorageFileReader::Bytes(nr_storage::FileContentBytes::Bytes(
            Bytes::from_static(body),
        )),
    };
    let response = super::rewrite_metadata_tarballs(
        &parts,
        &StoragePath::from("@scope/pkg/1.2.3-rc.1+Build.7"),
        file,
    )
    .await
    .unwrap()
    .unwrap()
    .into_response_default();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        metadata["dist"]["tarball"],
        "https://pkgly.test/repositories/store/npm-proxy/@scope/pkg/-/pkg-1.2.3-rc.1+Build.7.tgz"
    );
}

#[tokio::test]
async fn metadata_cache_upgrade_reads_previous_namespace_without_upstream() {
    let repository_id = Uuid::new_v4();
    let storage = test_storage().await;
    let path = StoragePath::from("@scope/pkg");
    storage
        .save_file(
            repository_id,
            FileContent::Bytes(Bytes::from_static(b"cached document")),
            &StoragePath::from("metadata/@scope/pkg"),
        )
        .await
        .unwrap();
    let cached = super::open_cached_file(
        &storage,
        repository_id,
        &metadata_cache_path_for_npm_proxy(&path),
        Some(&path),
    )
    .await
    .unwrap();
    assert!(
        cached.is_some(),
        "old metadata cache must remain usable without an upstream"
    );
    let version_path = StoragePath::from("@scope/pkg/1.0.0-beta.1");
    assert!(
        super::open_cached_file(
            &storage,
            repository_id,
            &metadata_cache_path_for_npm_proxy(&version_path),
            Some(&version_path)
        )
        .await
        .unwrap()
        .is_none(),
        "an older package cache file must not block a version cache miss"
    );
}
