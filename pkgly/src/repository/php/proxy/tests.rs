#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use nr_core::storage::StoragePath;
use serde_json::json;

#[test]
fn rewrite_proxy_metadata_rewrites_and_preserves_upstream() {
    let upstream = "https://example.com/dist/demo-1.0.0.zip";
    let metadata = json!({
        "packages": {
            "acme/demo": [
                {
                    "version": "1.0.0",
                    "dist": {
                        "url": upstream,
                        "shasum": "abc123"
                    }
                }
            ]
        }
    });

    let (rewritten_bytes, metas) = rewrite_proxy_metadata(
        serde_json::to_vec(&metadata)
            .expect("metadata bytes")
            .as_slice(),
        "/repositories/storage/php-proxy",
    )
    .expect("rewrite succeeds");

    let rewritten: serde_json::Value =
        serde_json::from_slice(&rewritten_bytes).expect("parse rewritten metadata");
    let dist_obj = rewritten["packages"]["acme/demo"][0]["dist"]
        .as_object()
        .expect("dist object");

    assert_eq!(
        dist_obj["url"],
        json!("/repositories/storage/php-proxy/dist/acme/demo/1.0.0/demo-1.0.0.zip")
    );
    assert_eq!(dist_obj["pkgly-upstream-url"], json!(upstream));

    assert_eq!(metas.len(), 1);
    let meta = &metas[0];
    assert_eq!(meta.cache_path, "dist/acme/demo/1.0.0/demo-1.0.0.zip");
    assert_eq!(meta.upstream_url.as_deref(), Some(upstream));
    assert_eq!(meta.upstream_digest.as_deref(), Some("abc123"));
}

#[test]
fn proxy_meta_is_recoverable_from_cached_metadata() {
    let metadata = json!({
        "packages": {
            "acme/demo": [
                {
                    "version": "1.0.0",
                    "dist": {
                        "url": "/repositories/storage/php-proxy/dist/acme/demo/1.0.0/demo-1.0.0.zip",
                        "pkgly-upstream-url": "https://example.com/dist/demo-1.0.0.zip",
                        "shasum": "abc123"
                    }
                }
            ]
        }
    });

    let cache_path = StoragePath::from("dist/acme/demo/1.0.0/demo-1.0.0.zip");
    let meta =
        proxy_meta_from_metadata_doc(&metadata, &cache_path).expect("metadata extraction succeeds");

    assert_eq!(meta.package_key, "acme/demo");
    assert_eq!(meta.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        meta.upstream_url.as_deref(),
        Some("https://example.com/dist/demo-1.0.0.zip")
    );
    assert_eq!(meta.upstream_digest.as_deref(), Some("abc123"));
}

#[test]
fn github_zipball_to_codeload_converts_api_url() {
    let original = "https://api.github.com/repos/acme/demo/zipball/v1.2.3";
    let converted =
        github_zipball_to_codeload(original).expect("github zipball URL should convert");
    assert_eq!(
        converted,
        "https://codeload.github.com/acme/demo/legacy.zip/v1.2.3"
    );
}

#[test]
fn github_zipball_to_codeload_rejects_non_github_or_non_zipball() {
    assert!(
        github_zipball_to_codeload("https://example.com/repos/acme/demo/zipball/v1.2.3").is_none()
    );
    assert!(
        github_zipball_to_codeload("https://api.github.com/repos/acme/demo/tarball/v1.2.3")
            .is_none()
    );
    assert!(github_zipball_to_codeload("not a url").is_none());
}
