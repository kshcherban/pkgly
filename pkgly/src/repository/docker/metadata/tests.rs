// ABOUTME: Verifies Docker metadata helpers against real local storage objects.
// ABOUTME: Covers manifest references and stored manifest sizes for repository catalogs.
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

#[test]
fn manifest_references_deduplicate_paths_without_using_declared_sizes() {
    let paths = manifest_references("library/alpine", &image_manifest(999, 999));
    assert_eq!(
        paths,
        vec![
            format!("v2/library/alpine/blobs/{CONFIG_DIGEST}"),
            format!("v2/library/alpine/blobs/{LAYER_DIGEST}"),
        ]
    );
    let index = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": CHILD_DIGEST, "size": 999}]
    }))
    .unwrap();
    assert_eq!(
        manifest_references("library/alpine", &index),
        vec![format!("v2/library/alpine/manifests/{CHILD_DIGEST}")]
    );
    assert!(manifest_references("library/alpine", b"invalid").is_empty());
}

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
async fn catalog_entries_report_manifest_bytes_without_summing_layers() -> anyhow::Result<()> {
    let storage = test_storage().await;
    let repository_id = Uuid::new_v4();
    let manifest = image_manifest(10, 20);
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
        &format!("v2/library/alpine/blobs/{LAYER_DIGEST}"),
        b"layer",
    )
    .await?;
    let entries = collect_manifest_entries(&storage, repository_id).await?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, manifest.len() as u64);
    Ok(())
}
