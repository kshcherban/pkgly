#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::proxy_indexing::{ProxyIndexing, ProxyIndexingError};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{FixedOffset, TimeZone, Utc};
use nr_core::ConfigTimeStamp;
use nr_core::repository::project::{ProxyArtifactKey, ProxyArtifactMeta};
use nr_storage::{
    DynStorage, FileContent, StaticStorageFactory,
    local::{LocalConfig, LocalStorageFactory},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::{
    sync::{Barrier, Mutex},
    time::{Duration, sleep, timeout},
};

async fn local_storage() -> Result<(DynStorage, TempDir)> {
    let tempdir = tempfile::tempdir()?;
    let storage_config = nr_storage::StorageConfig {
        storage_config: nr_storage::StorageConfigInner {
            storage_name: "test-storage".into(),
            storage_id: Uuid::new_v4(),
            storage_type: "Local".into(),
            created_at: ConfigTimeStamp::from(Utc::now()),
        },
        type_config: nr_storage::StorageTypeConfig::Local(LocalConfig {
            path: tempdir.path().to_path_buf(),
        }),
    };
    let local =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(storage_config)
            .await?;
    Ok((DynStorage::Local(local), tempdir))
}

#[derive(Clone, Default)]
struct RecordingIndexer {
    recorded: Arc<Mutex<Vec<ProxyArtifactMeta>>>,
    evicted: Arc<Mutex<Vec<ProxyArtifactKey>>>,
}

impl RecordingIndexer {
    #[allow(dead_code)]
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
async fn map_ordered_concurrent_executes_tasks_in_parallel() -> Result<()> {
    let barrier = Arc::new(Barrier::new(2));
    let inputs = vec![1, 2];
    let fut = super::map_ordered_concurrent(inputs.clone(), 2, move |value| {
        let barrier = barrier.clone();
        async move {
            barrier.wait().await;
            Ok::<_, ()>(value)
        }
    });

    let values = timeout(Duration::from_millis(250), fut)
        .await
        .expect("tasks should complete in parallel")
        .expect("task execution should succeed");
    assert_eq!(values, inputs);
    Ok(())
}

#[tokio::test]
async fn map_ordered_concurrent_preserves_input_order() -> Result<()> {
    let inputs = vec![1, 2, 3, 4];
    let results = super::map_ordered_concurrent(inputs.clone(), 4, move |value| async move {
        let delay = Duration::from_millis((5 - value) as u64 * 5);
        sleep(delay).await;
        Ok::<_, ()>(value * 2)
    })
    .await
    .expect("task execution should succeed");

    assert_eq!(results, inputs.iter().map(|v| v * 2).collect::<Vec<_>>());
    Ok(())
}

#[test]
fn package_file_entry_serializes_blob_digest_for_helm() {
    let modified = FixedOffset::east_opt(0)
        .expect("offset")
        .with_ymd_and_hms(2025, 11, 5, 9, 30, 0)
        .single()
        .expect("datetime");

    let entry_with_digest = PackageFileEntry {
        package: "acme".to_string(),
        name: "1.2.3".to_string(),
        cache_path: "charts/acme-1.2.3.tgz".to_string(),
        blob_digest: Some("sha256:deadbeef".to_string()),
        size: 4096,
        modified,
    };

    let with_value = serde_json::to_value(&entry_with_digest).expect("serialize entry");
    assert_eq!(
        with_value
            .get("blob_digest")
            .and_then(|value| value.as_str()),
        Some("sha256:deadbeef")
    );

    let entry_without_digest = PackageFileEntry {
        blob_digest: None,
        ..entry_with_digest
    };
    let without_value = serde_json::to_value(&entry_without_digest).expect("serialize entry");
    assert!(
        without_value.get("blob_digest").is_none(),
        "blob_digest should be omitted when not present"
    );
}

#[test]
fn sha256_digest_from_base64_formats_oci_style() {
    // base64(sha256("")) where sha256("") is e3b0c442...b855
    let base64 = "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=";
    let digest = super::sha256_digest_from_base64(base64).expect("digest should decode");
    assert_eq!(
        digest,
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn normalize_sha256_digest_prefixes_plain_values() {
    assert_eq!(
        super::normalize_sha256_digest("deadbeef").as_deref(),
        Some("sha256:deadbeef")
    );
    assert_eq!(
        super::normalize_sha256_digest("sha256:deadbeef").as_deref(),
        Some("sha256:deadbeef")
    );
    assert!(super::normalize_sha256_digest("  ").is_none());
}

#[test]
fn cargo_package_entry_exposes_checksum_as_blob_digest() {
    let updated_at = FixedOffset::east_opt(0)
        .expect("offset")
        .with_ymd_and_hms(2025, 11, 5, 9, 30, 0)
        .single()
        .expect("datetime");

    let metadata = CargoPackageMetadata {
        checksum: "deadbeef".to_string(),
        crate_size: 42,
        yanked: false,
        features: Default::default(),
        dependencies: Vec::new(),
        extra: None,
    };
    let entry = super::build_cargo_package_entry("acme", "acme", "1.2.3", updated_at, &metadata);
    assert_eq!(entry.blob_digest.as_deref(), Some("sha256:deadbeef"));
}

#[tokio::test]
async fn gather_package_dirs_lists_nested_packages() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();
    storage
        .save_file(
            repository,
            FileContent::from(b"wheel"),
            &nr_core::storage::StoragePath::from("packages/example/example-1.0.0.whl"),
        )
        .await?;
    storage
        .save_file(
            repository,
            FileContent::from(b"tarball"),
            &nr_core::storage::StoragePath::from("packages/@scope/pkg/pkg-2.3.4.tgz"),
        )
        .await?;

    let mut packages = gather_package_dirs(&storage, repository, Some("packages/")).await?;
    packages.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        packages,
        vec![
            ("@scope/pkg".to_string(), "@scope/pkg".to_string()),
            ("example".to_string(), "example".to_string())
        ]
    );
    Ok(())
}

#[tokio::test]
async fn gather_package_dirs_root_lists_python_packages() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();
    storage
        .save_file(
            repository,
            FileContent::from(b"wheel"),
            &nr_core::storage::StoragePath::from("example_pkg/1.0.0/example_pkg-1.0.0.whl"),
        )
        .await?;

    let mut packages = gather_package_dirs(&storage, repository, None).await?;
    packages.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        packages,
        vec![(
            "example_pkg/1.0.0".to_string(),
            "example_pkg/1.0.0".to_string()
        )]
    );
    Ok(())
}

#[tokio::test]
async fn gather_package_dirs_handles_go_proxy_layout() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();
    storage
        .save_file(
            repository,
            FileContent::from(b"info-json"),
            &nr_core::storage::StoragePath::from(
                "go-proxy-cache/github.com/example/module/@v/v1.0.0.info",
            ),
        )
        .await?;

    let mut packages = gather_package_dirs(&storage, repository, Some("go-proxy-cache/")).await?;
    packages.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        packages,
        vec![(
            "github.com/example/module".to_string(),
            "github.com/example/module/@v".to_string()
        )]
    );
    Ok(())
}

#[tokio::test]
async fn directory_package_pagination_respects_page_window() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();

    for idx in 0..3 {
        let pkg = format!("pkg-{idx}");
        let path = format!("packages/{pkg}/{pkg}.tar.gz");
        storage
            .save_file(
                repository,
                FileContent::from(b"archive".as_slice()),
                &nr_core::storage::StoragePath::from(path),
            )
            .await?;
    }

    let response =
        super::collect_directory_package_page(&storage, repository, Some("packages/"), 2, 1, None)
            .await?;

    assert_eq!(response.total_packages, 3);
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].package, "pkg-1");
    Ok(())
}

#[tokio::test]
async fn directory_package_pagination_allows_large_page_sizes() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();

    for idx in 0..120 {
        let pkg = format!("pkg-{idx:03}");
        let path = format!("packages/{pkg}/{pkg}.tar.gz");
        storage
            .save_file(
                repository,
                FileContent::from(b"archive".as_slice()),
                &nr_core::storage::StoragePath::from(path),
            )
            .await?;
    }

    let response = super::collect_directory_package_page(
        &storage,
        repository,
        Some("packages/"),
        1,
        500,
        None,
    )
    .await?;

    assert_eq!(response.total_packages, 120);
    assert_eq!(response.items.len(), 120);
    Ok(())
}

#[tokio::test]
async fn go_package_pagination_respects_page_window() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();

    let module = "github.com/example/module";
    for version in ["v1.0.0", "v1.1.0", "v2.0.0"] {
        let base = format!("go-proxy-cache/{module}/@v/{version}");
        storage
            .save_file(
                repository,
                FileContent::from(b"info".as_slice()),
                &nr_core::storage::StoragePath::from(format!("{base}.info")),
            )
            .await?;
    }

    let response =
        super::collect_go_package_page(&storage, repository, "go-proxy-cache/", 2, 1, None).await?;

    assert_eq!(response.total_packages, 3);
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].name, "v1.1.0");
    Ok(())
}

#[tokio::test]
async fn build_maven_proxy_package_list_exposes_cached_files() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();

    let base_dir = "com/example/demo/1.0.0";
    storage
        .save_file(
            repository,
            FileContent::from(b"jar-bytes"),
            &nr_core::storage::StoragePath::from(format!("{base_dir}/demo-1.0.0.jar")),
        )
        .await?;
    storage
        .save_file(
            repository,
            FileContent::from(b"pom-bytes"),
            &nr_core::storage::StoragePath::from(format!("{base_dir}/demo-1.0.0.pom")),
        )
        .await?;
    storage
        .save_file(
            repository,
            FileContent::from(b"metadata"),
            &nr_core::storage::StoragePath::from("com/example/demo/maven-metadata.xml"),
        )
        .await?;

    let response = super::build_maven_proxy_package_list(&storage, repository, 1, 50, None).await?;
    assert_eq!(response.total_packages, 1);
    assert_eq!(response.items.len(), 2);

    let mut names: Vec<&str> = response
        .items
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["demo-1.0.0.jar", "demo-1.0.0.pom"]);
    assert!(
        response
            .items
            .iter()
            .all(|item| item.package == "com.example:demo:1.0.0")
    );
    assert!(
        response
            .items
            .iter()
            .all(|item| item.cache_path.starts_with("com/example/demo/1.0.0/"))
    );
    Ok(())
}

#[tokio::test]
async fn build_maven_proxy_package_list_paginates_versions() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();

    let versions = [("1.0.0", b"v1"), ("1.1.0", b"v2")];
    for (version, data) in versions.iter() {
        let base_dir = format!("com/example/demo/{version}");
        storage
            .save_file(
                repository,
                FileContent::from(*data),
                &nr_core::storage::StoragePath::from(format!("{base_dir}/demo-{version}.jar")),
            )
            .await?;
        storage
            .save_file(
                repository,
                FileContent::from(*data),
                &nr_core::storage::StoragePath::from(format!("{base_dir}/demo-{version}.pom")),
            )
            .await?;
    }

    let first_page =
        super::build_maven_proxy_package_list(&storage, repository, 1, 1, None).await?;
    assert_eq!(first_page.total_packages, 2);
    assert!(
        first_page
            .items
            .iter()
            .all(|item| item.package.ends_with(":1.0.0"))
    );

    let second_page =
        super::build_maven_proxy_package_list(&storage, repository, 2, 1, None).await?;
    assert_eq!(second_page.total_packages, 2);
    assert!(
        second_page
            .items
            .iter()
            .all(|item| item.package.ends_with(":1.1.0"))
    );
    Ok(())
}

#[tokio::test]
async fn collect_go_package_entries_deduplicates_versions() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();
    storage
        .save_file(
            repository,
            FileContent::from(b"info"),
            &nr_core::storage::StoragePath::from(
                "go-proxy-cache/github.com/example/module/@v/v1.0.0.info",
            ),
        )
        .await?;
    storage
        .save_file(
            repository,
            FileContent::from(b"zip"),
            &nr_core::storage::StoragePath::from(
                "go-proxy-cache/github.com/example/module/@v/v1.0.0.zip",
            ),
        )
        .await?;
    storage
        .save_file(
            repository,
            FileContent::from(b"mod"),
            &nr_core::storage::StoragePath::from(
                "go-proxy-cache/github.com/example/module/@v/v1.1.0.mod",
            ),
        )
        .await?;

    let entries =
        super::collect_go_package_entries(&storage, repository, "go-proxy-cache/").await?;
    assert_eq!(entries.len(), 2);
    let mut versions: Vec<_> = entries.iter().map(|entry| entry.name.clone()).collect();
    versions.sort();
    assert_eq!(versions, vec!["v1.0.0".to_string(), "v1.1.0".to_string()]);
    let zip_entry = entries
        .iter()
        .find(|entry| entry.name == "v1.0.0")
        .expect("zip entry present");
    assert!(
        zip_entry.cache_path.ends_with(".zip"),
        "expected zip cache path but found {}",
        zip_entry.cache_path
    );
    Ok(())
}

#[tokio::test]
async fn collect_go_package_entries_handles_hosted_storage() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository = Uuid::new_v4();
    storage
        .save_file(
            repository,
            FileContent::from(b"zip"),
            &nr_core::storage::StoragePath::from(
                "packages/github.com/example/module/@v/v2.3.4.zip",
            ),
        )
        .await?;

    let entries = super::collect_go_package_entries(&storage, repository, "packages/").await?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].package, "github.com/example/module");
    assert_eq!(entries[0].name, "v2.3.4");
    assert!(
        entries[0].cache_path.ends_with(".zip"),
        "expected zip cache path but found {}",
        entries[0].cache_path
    );
    Ok(())
}

#[tokio::test]
async fn delete_docker_manifest_removes_all_payloads() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let repository_name = "library/alpine";

    let config_bytes = b"config-json";
    let layer_a = b"layer-a";
    let layer_b = b"layer-b";
    let config_digest = format!("sha256:{:x}", Sha256::digest(config_bytes));
    let layer_a_digest = format!("sha256:{:x}", Sha256::digest(layer_a));
    let layer_b_digest = format!("sha256:{:x}", Sha256::digest(layer_b));

    let manifest_json = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
        "config": {
            "mediaType": "application/vnd.docker.container.image.v1+json",
            "size": config_bytes.len(),
            "digest": config_digest,
        },
        "layers": [
            {
                "mediaType": "application/vnd.docker.image.rootfs.diff.tar",
                "size": layer_a.len(),
                "digest": layer_a_digest,
            },
            {
                "mediaType": "application/vnd.docker.image.rootfs.diff.tar",
                "size": layer_b.len(),
                "digest": layer_b_digest,
            }
        ]
    });
    let manifest_bytes = serde_json::to_vec(&manifest_json)?;
    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));

    let tag_path =
        nr_core::storage::StoragePath::from(format!("v2/{}/manifests/latest", repository_name));
    storage
        .save_file(
            repository_id,
            FileContent::from(manifest_bytes.clone()),
            &tag_path,
        )
        .await?;

    let digest_path_str = format!("v2/{}/manifests/{}", repository_name, manifest_digest);
    let digest_path = nr_core::storage::StoragePath::from(digest_path_str.clone());
    storage
        .save_file(
            repository_id,
            FileContent::from(manifest_bytes.clone()),
            &digest_path,
        )
        .await?;

    let blobs = [
        (&config_digest, config_bytes.as_slice()),
        (&layer_a_digest, layer_a.as_slice()),
        (&layer_b_digest, layer_b.as_slice()),
    ];

    for (digest, content) in blobs {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        storage
            .save_file(
                repository_id,
                FileContent::from(content.to_vec()),
                &blob_path,
            )
            .await?;
    }

    let tag_cache_path = tag_path.to_string();
    let result =
        delete_docker_package(&storage, repository_id, tag_cache_path.as_str(), None).await?;
    assert_eq!(result.removed_manifests, 2);
    assert_eq!(result.removed_blobs, 3);

    assert!(!storage.file_exists(repository_id, &tag_path).await?);
    assert!(!storage.file_exists(repository_id, &digest_path).await?);

    for digest in [config_digest, layer_a_digest, layer_b_digest] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        assert!(!storage.file_exists(repository_id, &blob_path).await?);
    }

    Ok(())
}

#[tokio::test]
async fn delete_docker_manifest_handles_digest_path() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let repository_name = "library/busybox";

    let config_bytes = b"config-blob";
    let layer_bytes = b"layer-blob";
    let config_digest = format!("sha256:{:x}", Sha256::digest(config_bytes));
    let layer_digest = format!("sha256:{:x}", Sha256::digest(layer_bytes));

    let manifest_json = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
        "config": {
            "mediaType": "application/vnd.docker.container.image.v1+json",
            "size": config_bytes.len(),
            "digest": config_digest,
        },
        "layers": [
            {
                "mediaType": "application/vnd.docker.image.rootfs.diff.tar",
                "size": layer_bytes.len(),
                "digest": layer_digest,
            }
        ]
    });
    let manifest_bytes = serde_json::to_vec(&manifest_json)?;
    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));

    let digest_path = nr_core::storage::StoragePath::from(format!(
        "v2/{}/manifests/{}",
        repository_name, manifest_digest
    ));
    storage
        .save_file(
            repository_id,
            FileContent::from(manifest_bytes.clone()),
            &digest_path,
        )
        .await?;

    for (digest, content) in [
        (&config_digest, config_bytes.as_slice()),
        (&layer_digest, layer_bytes.as_slice()),
    ] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        storage
            .save_file(
                repository_id,
                FileContent::from(content.to_vec()),
                &blob_path,
            )
            .await?;
    }

    let digest_cache_path = digest_path.to_string();
    let result =
        delete_docker_package(&storage, repository_id, digest_cache_path.as_str(), None).await?;
    assert_eq!(result.removed_manifests, 1);
    assert_eq!(result.removed_blobs, 2);

    assert!(!storage.file_exists(repository_id, &digest_path).await?);
    for digest in [config_digest, layer_digest] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        assert!(!storage.file_exists(repository_id, &blob_path).await?);
    }

    Ok(())
}

#[tokio::test]
async fn delete_docker_package_notifies_indexer() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let repository_name = "library/notify";

    let config_bytes = b"config";
    let layer_bytes = b"layer";
    let config_digest = format!("sha256:{:x}", Sha256::digest(config_bytes));
    let layer_digest = format!("sha256:{:x}", Sha256::digest(layer_bytes));

    let manifest_json = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
        "config": {
            "mediaType": "application/vnd.docker.container.image.v1+json",
            "size": config_bytes.len(),
            "digest": config_digest,
        },
        "layers": [
            {
                "mediaType": "application/vnd.docker.image.rootfs.diff.tar",
                "size": layer_bytes.len(),
                "digest": layer_digest,
            }
        ],
    });
    let manifest_bytes = serde_json::to_vec(&manifest_json)?;
    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));

    let tag_path =
        nr_core::storage::StoragePath::from(format!("v2/{}/manifests/latest", repository_name));
    storage
        .save_file(
            repository_id,
            FileContent::from(manifest_bytes.clone()),
            &tag_path,
        )
        .await?;

    let digest_path = nr_core::storage::StoragePath::from(format!(
        "v2/{}/manifests/{}",
        repository_name, manifest_digest
    ));
    storage
        .save_file(
            repository_id,
            FileContent::from(manifest_bytes.clone()),
            &digest_path,
        )
        .await?;

    for (digest, bytes) in [
        (&config_digest, config_bytes.as_slice()),
        (&layer_digest, layer_bytes.as_slice()),
    ] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        storage
            .save_file(repository_id, FileContent::from(bytes.to_vec()), &blob_path)
            .await?;
    }

    let indexer = Arc::new(RecordingIndexer::default());
    delete_docker_package(
        &storage,
        repository_id,
        tag_path.to_string().as_str(),
        Some(indexer.as_ref()),
    )
    .await?;

    let evicted = indexer.evicted().await;
    assert!(
        evicted
            .iter()
            .any(|key| key.version.as_deref() == Some("latest"))
    );
    assert!(
        evicted
            .iter()
            .any(|key| key.version.as_deref() == Some(manifest_digest.as_str()))
    );

    Ok(())
}

#[tokio::test]
async fn collect_docker_deletions_batch_deduplicates_shared_layers() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let repository_name = "library/shared";

    let config_bytes = b"config-json";
    let layer_bytes = b"layer-bytes";
    let config_digest = format!("sha256:{:x}", Sha256::digest(config_bytes));
    let layer_digest = format!("sha256:{:x}", Sha256::digest(layer_bytes));

    let manifest_json = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
        "config": {
            "mediaType": "application/vnd.docker.container.image.v1+json",
            "size": config_bytes.len(),
            "digest": config_digest,
        },
        "layers": [
            {
                "mediaType": "application/vnd.docker.image.rootfs.diff.tar",
                "size": layer_bytes.len(),
                "digest": layer_digest,
            }
        ]
    });
    let manifest_bytes = serde_json::to_vec(&manifest_json)?;
    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));

    // Two tags pointing to the same manifest
    let tag_paths = [
        format!("v2/{}/manifests/latest", repository_name),
        format!("v2/{}/manifests/v1", repository_name),
    ];

    for tag in tag_paths.iter() {
        storage
            .save_file(
                repository_id,
                FileContent::from(manifest_bytes.clone()),
                &nr_core::storage::StoragePath::from(tag.as_str()),
            )
            .await?;
    }

    // Store the digest manifest and blobs
    let digest_path_str = format!("v2/{}/manifests/{}", repository_name, manifest_digest);
    let digest_path = nr_core::storage::StoragePath::from(digest_path_str.clone());
    storage
        .save_file(
            repository_id,
            FileContent::from(manifest_bytes.clone()),
            &digest_path,
        )
        .await?;

    for (digest, content) in [
        (&config_digest, config_bytes.as_slice()),
        (&layer_digest, layer_bytes.as_slice()),
    ] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        storage
            .save_file(
                repository_id,
                FileContent::from(content.to_vec()),
                &blob_path,
            )
            .await?;
    }

    let batch = super::collect_docker_deletions_batch(
        &storage,
        repository_id,
        &tag_paths.iter().cloned().collect::<Vec<_>>(),
        None,
    )
    .await?;

    assert!(batch.deleted_objects > 0);
    assert_eq!(batch.deleted_packages, 2);
    assert!(batch.missing.is_empty());
    assert!(batch.rejected.is_empty());

    for tag in tag_paths.iter() {
        let tag_storage_path = nr_core::storage::StoragePath::from(tag.as_str());
        assert!(
            !storage
                .file_exists(repository_id, &tag_storage_path)
                .await?
        );

        let sidecar = nr_core::storage::StoragePath::from(format!("{tag}.nr-docker-tagmeta"));
        assert!(!storage.file_exists(repository_id, &sidecar).await?);
    }

    assert!(!storage.file_exists(repository_id, &digest_path).await?);
    let digest_sidecar =
        nr_core::storage::StoragePath::from(format!("{digest_path_str}.nr-docker-tagmeta"));
    assert!(!storage.file_exists(repository_id, &digest_sidecar).await?);

    for digest in [&config_digest, &layer_digest] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        assert!(!storage.file_exists(repository_id, &blob_path).await?);
    }

    Ok(())
}

#[tokio::test]
async fn collect_docker_deletions_batch_streams_large_batches() -> Result<()> {
    const LARGE_DELETE_COUNT: usize = 1_200;

    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let repository_name = "library/huge";

    let config_bytes = b"config-json";
    let layer_bytes = b"layer-bytes";
    let config_digest = format!("sha256:{:x}", Sha256::digest(config_bytes));
    let layer_digest = format!("sha256:{:x}", Sha256::digest(layer_bytes));

    let manifest_json = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
        "config": {
            "mediaType": "application/vnd.docker.container.image.v1+json",
            "size": config_bytes.len(),
            "digest": config_digest,
        },
        "layers": [
            {
                "mediaType": "application/vnd.docker.image.rootfs.diff.tar",
                "size": layer_bytes.len(),
                "digest": layer_digest,
            }
        ]
    });
    let manifest_bytes = serde_json::to_vec(&manifest_json)?;
    let manifest_digest = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));

    let digest_path_str = format!("v2/{}/manifests/{}", repository_name, manifest_digest);
    let digest_path = nr_core::storage::StoragePath::from(digest_path_str.clone());
    storage
        .save_file(
            repository_id,
            FileContent::from(manifest_bytes.clone()),
            &digest_path,
        )
        .await?;

    for (digest, content) in [
        (&config_digest, config_bytes.as_slice()),
        (&layer_digest, layer_bytes.as_slice()),
    ] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        storage
            .save_file(
                repository_id,
                FileContent::from(content.to_vec()),
                &blob_path,
            )
            .await?;
    }

    let manifest_paths: Vec<String> = (0..LARGE_DELETE_COUNT)
        .map(|index| format!("v2/{}/manifests/tag-{index}", repository_name))
        .collect();

    for path in manifest_paths.iter() {
        let storage_path = nr_core::storage::StoragePath::from(path.as_str());
        storage
            .save_file(
                repository_id,
                FileContent::from(manifest_bytes.clone()),
                &storage_path,
            )
            .await?;
    }

    let batch =
        super::collect_docker_deletions_batch(&storage, repository_id, &manifest_paths, None)
            .await?;

    assert!(batch.deleted_objects > 0);
    assert_eq!(batch.deleted_packages, LARGE_DELETE_COUNT);
    assert!(batch.missing.is_empty());
    assert!(batch.rejected.is_empty());

    assert!(!storage.file_exists(repository_id, &digest_path).await?);
    let digest_sidecar =
        nr_core::storage::StoragePath::from(format!("{digest_path_str}.nr-docker-tagmeta"));
    assert!(!storage.file_exists(repository_id, &digest_sidecar).await?);

    for path in manifest_paths.iter() {
        let storage_path = nr_core::storage::StoragePath::from(path.as_str());
        assert!(!storage.file_exists(repository_id, &storage_path).await?);

        let sidecar = nr_core::storage::StoragePath::from(format!("{path}.nr-docker-tagmeta"));
        assert!(!storage.file_exists(repository_id, &sidecar).await?);
    }

    for digest in [&config_digest, &layer_digest] {
        let blob_path =
            nr_core::storage::StoragePath::from(format!("v2/{}/blobs/{}", repository_name, digest));
        assert!(!storage.file_exists(repository_id, &blob_path).await?);
    }

    Ok(())
}

#[test]
fn ignore_hidden_and_meta() {
    assert!(should_ignore(".DS_Store"));
    assert!(should_ignore("package.nr-meta"));
    assert!(!should_ignore("package.tar.gz"));
}

#[test]
fn validate_cache_path_rules() {
    assert!(is_valid_cache_path(
        "packages/example/pkg-1.0.whl",
        PackageStrategy::PackagesDirectory {
            base: Some("packages/"),
        },
    ));
    assert!(!is_valid_cache_path(
        "/etc/passwd",
        PackageStrategy::PackagesDirectory {
            base: Some("packages/"),
        },
    ));
    assert!(!is_valid_cache_path(
        "../packages/pkg.whl",
        PackageStrategy::PackagesDirectory {
            base: Some("packages/"),
        },
    ));
    assert!(!is_valid_cache_path(
        "package.zip",
        PackageStrategy::PackagesDirectory {
            base: Some("packages/"),
        },
    ));
}

#[test]
fn validate_maven_cache_paths() {
    assert!(is_valid_repository_path(
        "com/example/app/1.0.0/app-1.0.0.jar"
    ));
    assert!(!is_valid_repository_path("../com/example/app.jar"));
    assert!(!is_valid_repository_path("/absolute/path"));
    assert!(!is_valid_repository_path(""));
}

#[test]
fn validate_docker_manifest_paths() {
    assert!(is_valid_cache_path(
        "v2/library/nginx/manifests/latest",
        PackageStrategy::DockerHosted,
    ));
    assert!(!is_valid_cache_path(
        "/v2/library/nginx/manifests/latest",
        PackageStrategy::DockerHosted,
    ));
    assert!(!is_valid_cache_path(
        "v2/library/nginx/blobs/sha256:abc",
        PackageStrategy::DockerHosted,
    ));
    assert!(!is_valid_cache_path(
        "v2/library/../../etc/passwd",
        PackageStrategy::DockerHosted,
    ));
}

#[test]
fn cargo_cache_path_matches_crate_layout() {
    let path = super::cargo_cache_path("serde", "1.0.0");
    assert_eq!(path, "crates/serde/1.0.0/serde-1.0.0.crate");
}

#[test]
fn cargo_package_entry_uses_metadata() {
    let mut metadata = CargoPackageMetadata::default();
    metadata.crate_size = 1_337;
    let updated_at = chrono::DateTime::from_timestamp(1_700_000_000, 0)
        .unwrap()
        .with_timezone(&FixedOffset::east_opt(0).unwrap());
    let entry = super::build_cargo_package_entry("Serde", "serde", "1.0.0", updated_at, &metadata);
    assert_eq!(entry.package, "Serde");
    assert_eq!(entry.name, "1.0.0");
    assert_eq!(entry.size, 1_337);
    assert_eq!(entry.cache_path, "crates/serde/1.0.0/serde-1.0.0.crate");
    assert_eq!(entry.modified, updated_at);
}

#[test]
fn validate_cargo_cache_paths() {
    assert!(is_valid_cache_path(
        "crates/serde/1.0.0/serde-1.0.0.crate",
        PackageStrategy::Cargo
    ));
    assert!(!is_valid_cache_path(
        "/crates/serde/1.0.0/serde-1.0.0.crate",
        PackageStrategy::Cargo
    ));
    assert!(!is_valid_cache_path(
        "../crates/serde/1.0.0/serde-1.0.0.crate",
        PackageStrategy::Cargo
    ));
}

fn pkg_obj(key: &str, size: u64) -> PackageObject {
    PackageObject {
        key: key.to_string(),
        size,
        modified: chrono::Local::now().fixed_offset(),
    }
}

#[test]
fn build_package_page_groups_by_directory_and_ignores_meta() {
    let objects = vec![
        pkg_obj("packages/@scope/pkg/pkg-2.3.4.tgz", 42),
        // Hidden metadata object should be ignored
        pkg_obj("packages/@scope/pkg/pkg-2.3.4.tgz.nr-meta", 1),
        pkg_obj("packages/example/example-1.0.0.whl", 10),
    ];

    let response = super::build_package_page_from_objects(objects, Some("packages/"), 1, 50, None);

    assert_eq!(response.total_packages, 2);
    // Package ordering follows lexicographic directory order
    assert_eq!(response.items.len(), 2);
    assert_eq!(response.items[0].package, "@scope/pkg");
    assert_eq!(
        response.items[0].cache_path,
        "packages/@scope/pkg/pkg-2.3.4.tgz"
    );
    assert_eq!(response.items[1].package, "example");
}

#[test]
fn build_package_page_respects_pagination() {
    let objects = vec![
        pkg_obj("packages/alpha/a-1.tgz", 1),
        pkg_obj("packages/bravo/b-1.tgz", 1),
        pkg_obj("packages/charlie/c-1.tgz", 1),
    ];

    let response = super::build_package_page_from_objects(objects, Some("packages/"), 2, 1, None);

    assert_eq!(response.total_packages, 3);
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].package, "bravo");
    assert_eq!(response.items[0].name, "b-1.tgz");
}

#[test]
fn build_package_page_filters_with_search_term() {
    let objects = vec![
        pkg_obj("packages/alpha/a-1.tgz", 1),
        pkg_obj("packages/bravo/b-1.tgz", 1),
        pkg_obj("packages/charlie/c-1.tgz", 1),
    ];

    let response =
        super::build_package_page_from_objects(objects, Some("packages/"), 1, 2, Some("char"));

    assert_eq!(response.total_packages, 1);
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].package, "charlie");
    assert_eq!(response.items[0].name, "c-1.tgz");
}

#[test]
fn build_package_page_trims_go_proxy_suffix() {
    let objects = vec![pkg_obj(
        "go-proxy-cache/github.com/example/module/@v/v1.0.0.zip",
        123,
    )];

    let response =
        super::build_package_page_from_objects(objects, Some("go-proxy-cache/"), 1, 10, None);

    assert_eq!(response.total_packages, 1);
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].package, "github.com/example/module");
    assert_eq!(
        response.items[0].cache_path,
        "go-proxy-cache/github.com/example/module/@v/v1.0.0.zip"
    );
}

#[test]
fn derive_version_path_handles_strip_mode() {
    let cache_path = "crates/demo/1.0.0/demo-1.0.0.crate";
    let derived =
        super::derive_version_path(cache_path, super::CatalogDeletionMode::StripLastSegment);
    assert_eq!(derived, Some("crates/demo/1.0.0".to_string()));
}

#[test]
fn derive_version_path_returns_none_for_root_objects() {
    let derived = super::derive_version_path(
        "single-segment",
        super::CatalogDeletionMode::StripLastSegment,
    );
    assert!(derived.is_none());
}

#[tokio::test]
async fn load_php_version_entries_reads_dist_file() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let dist_path = nr_core::storage::StoragePath::from("dist/acme/example/example-1.2.3.zip");
    let content = b"zip-bytes";
    storage
        .save_file(repository_id, FileContent::from(&content[..]), &dist_path)
        .await?;

    let row = super::HostedCatalogRow {
        project_key: "acme/example".into(),
        version: "1.2.3".into(),
        version_path: dist_path.to_string(),
        version_data: sqlx::types::Json(VersionData::default()),
        updated_at: chrono::Utc::now().fixed_offset(),
    };

    let entries = super::load_php_version_entries(storage, repository_id, row)
        .await
        .expect("entries");

    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.package, "acme/example");
    assert_eq!(entry.name, "1.2.3");
    assert_eq!(entry.cache_path, "dist/acme/example/example-1.2.3.zip");
    assert_eq!(entry.size, content.len() as u64);
    Ok(())
}

#[tokio::test]
async fn load_php_version_entries_skips_proxy_metadata_when_dist_missing() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let fetched_at = chrono::Utc
        .with_ymd_and_hms(2025, 1, 2, 3, 4, 5)
        .single()
        .expect("timestamp");

    let meta = ProxyArtifactMeta::builder(
        "acme/example",
        "acme/example",
        "dist/acme/example/1.2.3/pkg-1.2.3.zip",
    )
    // Metadata-only entries created from upstream composer metadata do not
    // have a size set; they must not appear in the packages list until the
    // corresponding dist file has been cached.
    .version("1.2.3")
    .upstream_url("https://files.example.com/dist/pkg-1.2.3.zip")
    .fetched_at(fetched_at)
    .build();
    let mut version_data = VersionData::default();
    version_data.set_proxy_artifact(&meta)?;

    let row = super::HostedCatalogRow {
        project_key: "acme/example".into(),
        version: "1.2.3".into(),
        version_path: meta.cache_path.clone(),
        version_data: sqlx::types::Json(version_data),
        updated_at: fetched_at.fixed_offset(),
    };

    let entries = super::load_php_version_entries(storage, repository_id, row)
        .await
        .expect("entries");

    // Without a cached dist on storage, proxy rows should be hidden.
    assert!(entries.is_empty());
    Ok(())
}

#[tokio::test]
async fn load_php_version_entries_uses_proxy_metadata_for_cached_dist() -> Result<()> {
    let (storage, _tempdir) = local_storage().await?;
    let repository_id = Uuid::new_v4();
    let fetched_at = chrono::Utc
        .with_ymd_and_hms(2025, 1, 2, 3, 4, 5)
        .single()
        .expect("timestamp");

    let cache_path = "dist/acme/example/1.2.3/pkg-1.2.3.zip";
    let storage_path = nr_core::storage::StoragePath::from(cache_path);
    let content = b"cached-zip-bytes";
    storage
        .save_file(
            repository_id,
            FileContent::from(&content[..]),
            &storage_path,
        )
        .await?;

    let meta = ProxyArtifactMeta::builder("acme/example", "acme/example", cache_path)
        .version("1.2.3")
        .upstream_url("https://files.example.com/dist/pkg-1.2.3.zip")
        .size(2048)
        .fetched_at(fetched_at)
        .build();
    let mut version_data = VersionData::default();
    version_data.set_proxy_artifact(&meta)?;

    let row = super::HostedCatalogRow {
        project_key: "acme/example".into(),
        version: "1.2.3".into(),
        version_path: cache_path.to_string(),
        version_data: sqlx::types::Json(version_data),
        updated_at: fetched_at.fixed_offset(),
    };

    let entries = super::load_php_version_entries(storage, repository_id, row)
        .await
        .expect("entries");

    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    // Package/name fields and size come from proxy metadata stored in the
    // catalog; presence of a size marks dists that have been cached.
    assert_eq!(entry.package, "acme/example");
    assert_eq!(entry.name, "1.2.3");
    assert_eq!(entry.cache_path, cache_path);
    assert_eq!(entry.size, 2048);
    assert_eq!(entry.modified, fetched_at.fixed_offset());
    Ok(())
}

#[tokio::test]
async fn delete_version_records_by_path_normalizes_and_deletes() {
    let repository_id = Uuid::new_v4();
    let mut targets = ahash::HashSet::new();
    targets.insert("Crates/Demo/1.0.0/".to_string());
    targets.insert("crates/demo/1.0.0".to_string());
    targets.insert("   ".to_string());

    let mut mock = super::MockCatalogDeletionExecutor::new();
    mock.expect_delete_paths()
        .times(1)
        .withf(move |repo, paths| {
            repo == &repository_id && paths == &vec!["crates/demo/1.0.0".to_string()]
        })
        .returning(|_, _| Box::pin(async { Ok(1) }));

    let deleted = super::delete_version_records_by_path(&mock, repository_id, &targets)
        .await
        .expect("deletion succeeds");
    assert_eq!(deleted, 1);
}

#[tokio::test]
async fn delete_version_records_by_path_skips_executor_when_empty() {
    let repository_id = Uuid::new_v4();
    let mut mock = super::MockCatalogDeletionExecutor::new();
    mock.expect_delete_paths().never();
    let targets: ahash::HashSet<String> = ahash::HashSet::new();

    let deleted = super::delete_version_records_by_path(&mock, repository_id, &targets)
        .await
        .expect("skip is ok");
    assert_eq!(deleted, 0);
}

mod catalog_db_tests {
    use super::*;
    use crate::app::{
        authentication::session::SessionManagerConfig,
        config::{Mode, SecuritySettings, SiteSetting},
        webhooks::{UpsertWebhookInput, WebhookEventType, WebhookHeaderInput, create_webhook},
    };
    use crate::repository::NewRepository;
    use crate::repository::deb::{DebHostedConfig, DebRepositoryConfig, DebRepositoryConfigType};
    use crate::test_support::DB_TEST_LOCK;
    use ahash::HashMap;
    use nr_core::database::entities::user::auth_token::AuthToken;
    use nr_core::user::{Email, Username, permissions::RepositoryActions};
    use nr_core::{database::DatabaseConfig, repository::config::RepositoryConfigType};
    use sqlx::{PgPool, postgres::PgPoolOptions};
    use testcontainers::{Container, clients::Cli, images::generic::GenericImage};

    use nr_core::{
        database::entities::{
            project::{DBProject, NewProject, ProjectDBType, versions::NewVersion},
            storage::NewDBStorage,
        },
        repository::project::ReleaseType,
        storage::StorageName,
    };

    struct TestDb {
        pool: PgPool,
        port: u16,
        _container: Container<'static, GenericImage>,
        _docker: &'static Cli,
    }

    impl TestDb {
        fn pool(&self) -> &PgPool {
            &self.pool
        }
    }

    async fn start_postgres() -> TestDb {
        let docker: &'static Cli = Box::leak(Box::new(Cli::default()));
        let image = GenericImage::new("postgres", "18-alpine")
            .with_env_var("POSTGRES_PASSWORD", "password")
            .with_env_var("POSTGRES_USER", "postgres")
            .with_env_var("POSTGRES_DB", "postgres");
        let container = docker.run(image);
        let port = container.get_host_port_ipv4(5432);
        let url = format!("postgres://postgres:password@127.0.0.1:{port}/postgres");

        let mut last_err: Option<anyhow::Error> = None;
        for _ in 0..60 {
            match PgPoolOptions::new().max_connections(4).connect(&url).await {
                Ok(pool) => {
                    return TestDb {
                        pool,
                        port,
                        _container: container,
                        _docker: docker,
                    };
                }
                Err(err) => {
                    last_err = Some(err.into());
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
            }
        }

        panic!(
            "postgres container did not become ready: {}",
            last_err.unwrap_or_else(|| anyhow::anyhow!("unknown error"))
        );
    }

    async fn fresh_pool() -> TestDb {
        let db = start_postgres().await;

        nr_core::database::migration::run_migrations(db.pool())
            .await
            .expect("run migrations");

        db
    }

    async fn reset_database(db: &TestDb) {
        sqlx::query(
            "TRUNCATE TABLE project_versions, projects, repositories, storages RESTART IDENTITY CASCADE",
        )
        .execute(db.pool())
        .await
        .expect("truncate tables");
    }

    async fn insert_storage(pool: &PgPool) -> Uuid {
        let storage_name = StorageName::new("primary".to_string()).expect("storage name");
        let storage = NewDBStorage::new(
            "Local".into(),
            storage_name,
            serde_json::json!({ "path": "/tmp" }),
        );
        storage
            .insert(pool)
            .await
            .expect("insert storage")
            .expect("storage row")
            .id
    }

    async fn insert_storage_at(pool: &PgPool, path: &std::path::Path) -> Uuid {
        let storage_name = StorageName::new("primary".to_string()).expect("storage name");
        let storage = NewDBStorage::new(
            "Local".into(),
            storage_name,
            serde_json::json!({
                "type": "Local",
                "settings": {
                    "path": path,
                },
            }),
        );
        storage
            .insert(pool)
            .await
            .expect("insert storage")
            .expect("storage row")
            .id
    }

    async fn insert_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
        let repo = NewRepository {
            name: "maven-proxy-test".into(),
            uuid: Uuid::new_v4(),
            repository_type: "maven".into(),
            configs: HashMap::with_hasher(Default::default()),
        };
        repo.insert(storage_id, pool)
            .await
            .expect("insert repository")
            .id
    }

    async fn insert_php_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
        let repo = NewRepository {
            name: "composer-hosted-test".into(),
            uuid: Uuid::new_v4(),
            repository_type: "php".into(),
            configs: HashMap::with_hasher(Default::default()),
        };
        repo.insert(storage_id, pool)
            .await
            .expect("insert repository")
            .id
    }

    async fn insert_npm_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
        let repo = NewRepository {
            name: "npm-proxy-test".into(),
            uuid: Uuid::new_v4(),
            repository_type: "npm".into(),
            configs: HashMap::with_hasher(Default::default()),
        };
        repo.insert(storage_id, pool)
            .await
            .expect("insert npm repository")
            .id
    }

    async fn insert_deb_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
        let mut configs = HashMap::with_hasher(Default::default());
        configs.insert(
            DebRepositoryConfigType::get_type_static().to_string(),
            serde_json::to_value(DebRepositoryConfig::Hosted(DebHostedConfig::default()))
                .expect("serialize deb config"),
        );
        let repo = NewRepository {
            name: "deb-hosted-test".into(),
            uuid: Uuid::new_v4(),
            repository_type: "deb".into(),
            configs,
        };
        repo.insert(storage_id, pool)
            .await
            .expect("insert deb repository")
            .id
    }

    async fn build_site(db: &TestDb, root: &std::path::Path) -> Pkgly {
        Pkgly::new(
            Mode::Debug,
            SiteSetting::default(),
            SecuritySettings::default(),
            SessionManagerConfig {
                database_location: root.join("sessions.redb"),
                ..Default::default()
            },
            crate::repository::StagingConfig {
                staging_dir: root.join("staging"),
                ..Default::default()
            },
            None,
            DatabaseConfig {
                user: "postgres".into(),
                password: "password".into(),
                database: "postgres".into(),
                host: "127.0.0.1".into(),
                port: Some(db.port),
            },
            Some(root.join("storages")),
        )
        .await
        .expect("create site")
    }

    fn sample_auth() -> Authentication {
        let fixed_time =
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").expect("time");
        let token = AuthToken {
            id: 1,
            user_id: 1,
            name: Some("token".into()),
            description: None,
            token: "token".into(),
            active: true,
            source: "test".into(),
            expires_at: None,
            created_at: fixed_time,
        };
        let user = nr_core::database::entities::user::UserSafeData {
            id: 1,
            name: "Test Admin".into(),
            username: Username::new("test_admin".into()).expect("username"),
            email: Email::new("admin@example.com".into()).expect("email"),
            require_password_change: false,
            active: true,
            admin: true,
            user_manager: false,
            system_manager: true,
            default_repository_actions: vec![RepositoryActions::Read, RepositoryActions::Edit],
            updated_at: fixed_time,
            created_at: fixed_time,
        };
        Authentication::AuthToken(token, user)
    }

    async fn insert_maven_version(
        pool: &PgPool,
        repository_id: Uuid,
        project_key: &str,
        version: &str,
        version_path: &str,
    ) {
        insert_maven_version_named(
            pool,
            repository_id,
            project_key,
            project_key,
            version,
            version_path,
        )
        .await;
    }

    async fn insert_maven_version_named(
        pool: &PgPool,
        repository_id: Uuid,
        project_key: &str,
        project_name: &str,
        version: &str,
        version_path: &str,
    ) {
        let project = if let Some(existing) =
            DBProject::find_by_project_key(project_key, repository_id, pool)
                .await
                .expect("query project")
        {
            existing
        } else {
            NewProject {
                scope: None,
                project_key: project_key.to_string(),
                name: project_name.to_string(),
                description: None,
                repository: repository_id,
                storage_path: format!("{project_key}/"),
            }
            .insert(pool)
            .await
            .expect("insert project")
        };

        let new_version = NewVersion {
            project_id: project.id,
            repository_id,
            version: version.to_string(),
            release_type: ReleaseType::Stable,
            version_path: version_path.to_string(),
            publisher: None,
            version_page: None,
            extra: VersionData::default(),
        };
        new_version.insert(pool).await.expect("insert version");
    }

    #[tokio::test]
    async fn deb_package_delete_enqueues_webhook_before_catalog_row_is_removed() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;
        let root = tempfile::tempdir().expect("tempdir");
        let storage_id = insert_storage_at(db.pool(), root.path()).await;
        let repository_id = insert_deb_repository(db.pool(), storage_id).await;
        let package_path = "pool/main/s/sample/sample_1.0.0_amd64.deb";

        insert_maven_version(db.pool(), repository_id, "sample", "1.0.0", package_path).await;
        create_webhook(
            db.pool(),
            UpsertWebhookInput {
                name: "deb deletes".into(),
                enabled: true,
                target_url: "http://127.0.0.1:9/webhook".into(),
                events: vec![WebhookEventType::PackageDeleted],
                headers: Vec::<WebhookHeaderInput>::new(),
            },
        )
        .await
        .expect("create webhook");

        let site = build_site(&db, root.path()).await;
        let repository = site
            .get_repository(repository_id)
            .expect("repository should be loaded");
        repository
            .get_storage()
            .save_file(
                repository_id,
                FileContent::from(b"deb bytes".as_slice()),
                &nr_core::storage::StoragePath::from(package_path),
            )
            .await
            .expect("save package");

        let response = super::delete_cached_packages(
            State(site.clone()),
            sample_auth(),
            Path(repository_id),
            Json(PackageDeleteRequest {
                paths: vec![package_path.to_string()],
            }),
        )
        .await
        .expect("delete succeeds");

        assert_eq!(response.status(), http::StatusCode::OK);
        let payloads: Vec<serde_json::Value> = sqlx::query_scalar(
            r#"
            SELECT payload
            FROM webhook_deliveries
            WHERE event_type = 'package.deleted'
            "#,
        )
        .fetch_all(db.pool())
        .await
        .expect("fetch deliveries");
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["data"]["repository"]["format"], "deb");
        assert_eq!(payloads[0]["data"]["package"]["name"], "sample");
        assert_eq!(payloads[0]["data"]["package"]["version"], "1.0.0");
        assert_eq!(
            payloads[0]["data"]["package"]["canonical_path"],
            package_path
        );

        let remaining_versions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM project_versions WHERE repository_id = $1")
                .bind(repository_id)
                .fetch_one(db.pool())
                .await
                .expect("count versions");
        assert_eq!(remaining_versions, 0);
        site.close().await;
    }

    async fn insert_proxy_version(
        pool: &PgPool,
        repository_id: Uuid,
        package_key: &str,
        package_name: &str,
        version: &str,
        cache_path: &str,
        size: u64,
        fetched_at: chrono::DateTime<chrono::Utc>,
    ) {
        let project = if let Some(existing) =
            DBProject::find_by_project_key(package_key, repository_id, pool)
                .await
                .expect("query project")
        {
            existing
        } else {
            NewProject {
                scope: None,
                project_key: package_key.to_string(),
                name: package_name.to_string(),
                description: None,
                repository: repository_id,
                storage_path: format!("{package_key}/"),
            }
            .insert(pool)
            .await
            .expect("insert project")
        };

        let mut version_data = VersionData::default();
        let meta = ProxyArtifactMeta::builder(package_name, package_key, cache_path)
            .version(version)
            .size(size)
            .fetched_at(fetched_at)
            .build();
        version_data
            .set_proxy_artifact(&meta)
            .expect("store proxy metadata");

        let new_version = NewVersion {
            project_id: project.id,
            repository_id,
            version: version.to_string(),
            release_type: ReleaseType::release_type_from_version(version),
            version_path: cache_path.to_string(),
            publisher: None,
            version_page: None,
            extra: version_data,
        };
        new_version.insert(pool).await.expect("insert version");
    }

    #[tokio::test]
    async fn fetch_maven_catalog_page_respects_pagination() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;

        let storage_id = insert_storage(db.pool()).await;
        let repository_id = insert_repository(db.pool(), storage_id).await;

        insert_maven_version(
            db.pool(),
            repository_id,
            "com.example:alpha",
            "1.0.0",
            "com/example/alpha/1.0.0",
        )
        .await;
        insert_maven_version(
            db.pool(),
            repository_id,
            "com.example:alpha",
            "2.0.0",
            "com/example/alpha/2.0.0",
        )
        .await;
        insert_maven_version(
            db.pool(),
            repository_id,
            "com.example:bravo",
            "1.0.0",
            "com/example/bravo/1.0.0",
        )
        .await;

        let first_page = super::fetch_maven_catalog_page(db.pool(), repository_id, 2, 0, None)
            .await
            .expect("fetch catalog page");
        assert_eq!(first_page.len(), 2);
        assert_eq!(first_page[0].version, "1.0.0");
        assert_eq!(first_page[1].version, "2.0.0");

        let second_page = super::fetch_maven_catalog_page(db.pool(), repository_id, 2, 2, None)
            .await
            .expect("fetch second page");
        assert_eq!(second_page.len(), 1);
        assert_eq!(second_page[0].project_key, "com.example:bravo");
        assert_eq!(second_page[0].version_path, "com/example/bravo/1.0.0");
    }

    #[tokio::test]
    async fn fetch_php_catalog_page_returns_versions_for_hosted_repo() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;

        let storage_id = insert_storage(db.pool()).await;
        let repository_id = insert_php_repository(db.pool(), storage_id).await;

        insert_maven_version(
            db.pool(),
            repository_id,
            "acme/example",
            "1.2.3",
            "dist/acme/example/example-1.2.3.zip",
        )
        .await;

        let rows = super::fetch_php_catalog_page(db.pool(), repository_id, 10, 0, None)
            .await
            .expect("fetch catalog");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].project_key, "acme/example");
        assert_eq!(rows[0].version, "1.2.3");
        assert_eq!(rows[0].version_path, "dist/acme/example/example-1.2.3.zip");
    }

    #[tokio::test]
    async fn fetch_npm_proxy_catalog_page_respects_pagination() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;

        let storage_id = insert_storage(db.pool()).await;
        let repository_id = insert_npm_repository(db.pool(), storage_id).await;
        let fetched = chrono::Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .unwrap();

        insert_proxy_version(
            db.pool(),
            repository_id,
            "left-pad",
            "left-pad",
            "1.0.0",
            "packages/left-pad/left-pad-1.0.0.tgz",
            1_111,
            fetched,
        )
        .await;
        insert_proxy_version(
            db.pool(),
            repository_id,
            "left-pad",
            "left-pad",
            "2.0.0",
            "packages/left-pad/left-pad-2.0.0.tgz",
            2_222,
            fetched,
        )
        .await;
        insert_proxy_version(
            db.pool(),
            repository_id,
            "lodash",
            "lodash",
            "4.17.21",
            "packages/lodash/lodash-4.17.21.tgz",
            3_333,
            fetched,
        )
        .await;

        let first_page = super::fetch_npm_proxy_catalog_page(db.pool(), repository_id, 2, 0, None)
            .await
            .expect("fetch first page");
        assert_eq!(first_page.len(), 2);
        assert_eq!(first_page[0].project_key, "left-pad");
        assert_eq!(
            first_page[0]
                .version_data
                .0
                .proxy_artifact()
                .and_then(|meta| meta.version.clone())
                .as_deref(),
            Some("1.0.0")
        );
        assert_eq!(
            first_page[1]
                .version_data
                .0
                .proxy_artifact()
                .and_then(|meta| meta.version.clone())
                .as_deref(),
            Some("2.0.0")
        );

        let second_page = super::fetch_npm_proxy_catalog_page(db.pool(), repository_id, 2, 2, None)
            .await
            .expect("fetch second page");
        assert_eq!(second_page.len(), 1);
        assert_eq!(second_page[0].project_key, "lodash");
        assert_eq!(
            second_page[0]
                .version_data
                .0
                .proxy_artifact()
                .and_then(|meta| meta.version.clone())
                .as_deref(),
            Some("4.17.21")
        );
    }

    #[tokio::test]
    async fn fetch_npm_proxy_catalog_page_filters_by_search_term() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;

        let storage_id = insert_storage(db.pool()).await;
        let repository_id = insert_npm_repository(db.pool(), storage_id).await;
        let fetched = chrono::Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .unwrap();

        insert_proxy_version(
            db.pool(),
            repository_id,
            "left-pad",
            "left-pad",
            "1.0.0",
            "packages/left-pad/left-pad-1.0.0.tgz",
            1_111,
            fetched,
        )
        .await;
        insert_proxy_version(
            db.pool(),
            repository_id,
            "lodash",
            "lodash",
            "4.17.21",
            "packages/lodash/lodash-4.17.21.tgz",
            3_333,
            fetched,
        )
        .await;
        insert_proxy_version(
            db.pool(),
            repository_id,
            "left-pad",
            "left-pad",
            "2.0.0",
            "packages/left-pad/left-pad-2.0.0.tgz",
            2_222,
            fetched,
        )
        .await;

        let rows = super::fetch_npm_proxy_catalog_page(db.pool(), repository_id, 5, 0, Some("pad"))
            .await
            .expect("filtered page");

        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.project_key == "left-pad"));

        let lodash_rows =
            super::fetch_npm_proxy_catalog_page(db.pool(), repository_id, 2, 0, Some("lod"))
                .await
                .expect("lodash only");

        assert_eq!(lodash_rows.len(), 1);
        assert_eq!(lodash_rows[0].project_key, "lodash");
    }

    #[tokio::test]
    async fn fetch_npm_proxy_catalog_page_filters_by_project_name() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;

        let storage_id = insert_storage(db.pool()).await;
        let repository_id = insert_npm_repository(db.pool(), storage_id).await;
        let fetched = chrono::Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .unwrap();

        insert_proxy_version(
            db.pool(),
            repository_id,
            "abc123",
            "TotallyDifferentName",
            "1.0.0",
            "packages/abc123/abc123-1.0.0.tgz",
            1_111,
            fetched,
        )
        .await;

        let rows =
            super::fetch_npm_proxy_catalog_page(db.pool(), repository_id, 10, 0, Some("different"))
                .await
                .expect("filtered page");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].project_key, "abc123");
    }

    #[tokio::test]
    async fn npm_proxy_rows_convert_to_package_entries() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;

        let storage_id = insert_storage(db.pool()).await;
        let repository_id = insert_npm_repository(db.pool(), storage_id).await;
        let fetched = chrono::Utc
            .with_ymd_and_hms(2025, 1, 2, 12, 0, 0)
            .single()
            .unwrap();

        insert_proxy_version(
            db.pool(),
            repository_id,
            "left-pad",
            "left-pad",
            "3.0.0",
            "packages/left-pad/left-pad-3.0.0.tgz",
            4_444,
            fetched,
        )
        .await;

        let rows = super::fetch_npm_proxy_catalog_page(db.pool(), repository_id, 1, 0, None)
            .await
            .expect("fetch rows");
        assert_eq!(rows.len(), 1);
        let entry = super::proxy_entry_from_row(&rows[0]).expect("proxy entry");

        let expected_modified: chrono::DateTime<chrono::FixedOffset> = fetched.into();
        assert_eq!(entry.package, "left-pad");
        assert_eq!(entry.name, "left-pad-3.0.0.tgz");
        assert_eq!(entry.cache_path, "packages/left-pad/left-pad-3.0.0.tgz");
        assert_eq!(entry.size, 4_444);
        assert_eq!(entry.modified, expected_modified);
    }

    #[tokio::test]
    async fn catalog_pagination_and_search_match_between_hosted_and_proxy_queries() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;

        let storage_id = insert_storage(db.pool()).await;
        let hosted_repository_id = insert_repository(db.pool(), storage_id).await;
        let proxy_repository_id = insert_npm_repository(db.pool(), storage_id).await;
        let fetched = chrono::Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .unwrap();

        for (key, name, version) in [
            ("aaa", "TotallyDifferentName", "1.0.0"),
            ("bbb", "bbb", "1.0.0"),
            ("bbb", "bbb", "2.0.0"),
            ("ccc", "ccc", "1.0.0"),
        ] {
            let hosted_path = format!("packages/{key}/{key}-{version}.tgz");
            let proxy_path = hosted_path.clone();

            insert_maven_version_named(
                db.pool(),
                hosted_repository_id,
                key,
                name,
                version,
                &hosted_path,
            )
            .await;

            insert_proxy_version(
                db.pool(),
                proxy_repository_id,
                key,
                name,
                version,
                &proxy_path,
                1_111,
                fetched,
            )
            .await;
        }

        let hosted_first =
            super::fetch_maven_catalog_page(db.pool(), hosted_repository_id, 2, 0, None)
                .await
                .expect("hosted first page");
        let proxy_first =
            super::fetch_proxy_catalog_page(db.pool(), proxy_repository_id, 2, 0, None)
                .await
                .expect("proxy first page");
        assert_eq!(
            hosted_first
                .iter()
                .map(|row| (row.project_key.clone(), row.version.clone()))
                .collect::<Vec<_>>(),
            proxy_first
                .iter()
                .map(|row| (row.project_key.clone(), row.version.clone()))
                .collect::<Vec<_>>()
        );

        let hosted_second =
            super::fetch_maven_catalog_page(db.pool(), hosted_repository_id, 2, 2, None)
                .await
                .expect("hosted second page");
        let proxy_second =
            super::fetch_proxy_catalog_page(db.pool(), proxy_repository_id, 2, 2, None)
                .await
                .expect("proxy second page");
        assert_eq!(
            hosted_second
                .iter()
                .map(|row| (row.project_key.clone(), row.version.clone()))
                .collect::<Vec<_>>(),
            proxy_second
                .iter()
                .map(|row| (row.project_key.clone(), row.version.clone()))
                .collect::<Vec<_>>()
        );

        let hosted_named = super::fetch_maven_catalog_page(
            db.pool(),
            hosted_repository_id,
            10,
            0,
            Some("different"),
        )
        .await
        .expect("hosted name search");
        let proxy_named = super::fetch_proxy_catalog_page(
            db.pool(),
            proxy_repository_id,
            10,
            0,
            Some("different"),
        )
        .await
        .expect("proxy name search");
        assert_eq!(hosted_named.len(), 1);
        assert_eq!(proxy_named.len(), 1);
        assert_eq!(hosted_named[0].project_key, "aaa");
        assert_eq!(proxy_named[0].project_key, "aaa");

        let hosted_version =
            super::fetch_maven_catalog_page(db.pool(), hosted_repository_id, 10, 0, Some("2.0.0"))
                .await
                .expect("hosted version search");
        let proxy_version =
            super::fetch_proxy_catalog_page(db.pool(), proxy_repository_id, 10, 0, Some("2.0.0"))
                .await
                .expect("proxy version search");
        assert_eq!(hosted_version.len(), 1);
        assert_eq!(proxy_version.len(), 1);
        assert_eq!(hosted_version[0].project_key, "bbb");
        assert_eq!(proxy_version[0].project_key, "bbb");
        assert_eq!(hosted_version[0].version, "2.0.0");
        assert_eq!(proxy_version[0].version, "2.0.0");
    }
}
