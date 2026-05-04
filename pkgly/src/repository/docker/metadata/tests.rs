// ABOUTME: Verifies Docker metadata helpers against real local storage objects.
// ABOUTME: Covers referenced manifest and blob size accounting for admin package listings.
use super::*;

use nr_core::storage::StoragePath;
use nr_storage::{FileContent, Storage};
use serde_json::json;
use uuid::Uuid;

use crate::repository::test_helpers::test_storage;

const CONFIG_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const LAYER_DIGEST: &str =
    "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const CHILD_DIGEST: &str =
    "sha256:3333333333333333333333333333333333333333333333333333333333333333";
const MISSING_CHILD_DIGEST: &str =
    "sha256:4444444444444444444444444444444444444444444444444444444444444444";

async fn save_bytes(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    storage
        .save_file(
            repository_id,
            FileContent::from(bytes),
            &StoragePath::from(path),
        )
        .await?;
    Ok(())
}

fn image_manifest(config_size: u64, layer_size: u64) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": CONFIG_DIGEST,
            "size": config_size
        },
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": LAYER_DIGEST,
                "size": layer_size
            },
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": LAYER_DIGEST,
                "size": layer_size
            }
        ]
    }))
    .expect("serialize manifest")
}

#[tokio::test]
async fn referenced_manifest_size_uses_stored_blob_sizes_and_dedupes_digests() -> anyhow::Result<()>
{
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let repository_name = "library/alpine";
    let manifest_path = StoragePath::from("v2/library/alpine/manifests/latest");
    let manifest = image_manifest(10, 20);
    let config = b"stored config";
    let layer = b"stored layer bytes";

    save_bytes(
        &storage,
        repository_id,
        "v2/library/alpine/manifests/latest",
        &manifest,
    )
    .await?;
    save_bytes(
        &storage,
        repository_id,
        &format!("v2/{repository_name}/blobs/{CONFIG_DIGEST}"),
        config,
    )
    .await?;
    save_bytes(
        &storage,
        repository_id,
        &format!("v2/{repository_name}/blobs/{LAYER_DIGEST}"),
        layer,
    )
    .await?;

    let size = calculate_referenced_manifest_size(&storage, repository_id, &manifest_path)
        .await?
        .expect("size should calculate");

    assert_eq!(
        size,
        manifest.len() as u64 + config.len() as u64 + layer.len() as u64
    );
    Ok(())
}

#[tokio::test]
async fn referenced_manifest_size_skips_missing_blobs() -> anyhow::Result<()> {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let manifest_path = StoragePath::from("v2/library/alpine/manifests/latest");
    let manifest = image_manifest(10, 20);

    save_bytes(
        &storage,
        repository_id,
        "v2/library/alpine/manifests/latest",
        &manifest,
    )
    .await?;

    let size = calculate_referenced_manifest_size(&storage, repository_id, &manifest_path)
        .await?
        .expect("size should calculate");

    assert_eq!(size, manifest.len() as u64);
    Ok(())
}

#[tokio::test]
async fn referenced_manifest_size_recurses_into_manifest_lists() -> anyhow::Result<()> {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let index_path = StoragePath::from("v2/library/alpine/manifests/latest");
    let child = image_manifest(10, 20);
    let index = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [
            {
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": CHILD_DIGEST,
                "size": 99
            },
            {
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": CHILD_DIGEST,
                "size": 99
            },
            {
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": MISSING_CHILD_DIGEST,
                "size": 88
            }
        ]
    }))?;

    save_bytes(
        &storage,
        repository_id,
        "v2/library/alpine/manifests/latest",
        &index,
    )
    .await?;
    save_bytes(
        &storage,
        repository_id,
        &format!("v2/library/alpine/manifests/{CHILD_DIGEST}"),
        &child,
    )
    .await?;
    save_bytes(
        &storage,
        repository_id,
        &format!("v2/library/alpine/blobs/{CONFIG_DIGEST}"),
        b"stored config",
    )
    .await?;
    save_bytes(
        &storage,
        repository_id,
        &format!("v2/library/alpine/blobs/{LAYER_DIGEST}"),
        b"stored layer",
    )
    .await?;

    let size = calculate_referenced_manifest_size(&storage, repository_id, &index_path)
        .await?
        .expect("size should calculate");

    assert_eq!(
        size,
        index.len() as u64
            + child.len() as u64
            + b"stored config".len() as u64
            + b"stored layer".len() as u64
    );
    Ok(())
}

#[tokio::test]
async fn referenced_manifest_size_returns_none_for_invalid_top_manifest() -> anyhow::Result<()> {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let manifest_path = StoragePath::from("v2/library/alpine/manifests/latest");

    save_bytes(
        &storage,
        repository_id,
        "v2/library/alpine/manifests/latest",
        br#"{"schemaVersion":"not-valid"}"#,
    )
    .await?;

    let size = calculate_referenced_manifest_size(&storage, repository_id, &manifest_path).await?;

    assert!(size.is_none());
    Ok(())
}
