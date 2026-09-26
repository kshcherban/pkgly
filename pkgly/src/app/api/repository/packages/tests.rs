// ABOUTME: Tests admin package listing and deletion behavior across repository formats.
// ABOUTME: Uses real storage and catalog records to verify package table API semantics.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::proxy_indexing::{ProxyIndexing, ProxyIndexingError};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{FixedOffset, TimeZone, Utc};
use nr_core::ConfigTimeStamp;
use nr_core::repository::project::{ProxyArtifactKey, ProxyArtifactMeta, VersionData};
use nr_storage::{
    DynStorage, FileContent, StaticStorageFactory,
    local::{LocalConfig, LocalStorageFactory},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

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
        delete_docker_package(&storage, repository_id, tag_cache_path.as_str(), None, None).await?;
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
    let result = delete_docker_package(
        &storage,
        repository_id,
        digest_cache_path.as_str(),
        None,
        None,
    )
    .await?;
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
        None,
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
        super::collect_docker_deletions_batch(&storage, repository_id, &manifest_paths, None, None)
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

#[test]
fn normalize_catalog_paths_normalizes_and_deduplicates() {
    let mut targets = ahash::HashSet::new();
    targets.insert("Crates/Demo/1.0.0/".to_string());
    targets.insert("crates/demo/1.0.0".to_string());
    targets.insert("   ".to_string());

    assert_eq!(
        super::normalize_catalog_paths(&targets),
        vec!["crates/demo/1.0.0".to_string()]
    );
}

#[test]
fn normalize_catalog_paths_skips_empty_paths() {
    let targets: ahash::HashSet<String> = ahash::HashSet::new();

    assert!(super::normalize_catalog_paths(&targets).is_empty());
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
    use crate::repository::docker::{DockerRegistryConfig, DockerRegistryConfigType};
    use crate::repository::maven::{MavenRepositoryConfig, MavenRepositoryConfigType};
    use crate::test_support::DB_TEST_LOCK;
    use ahash::HashMap;
    use nr_core::database::entities::user::auth_token::AuthToken;
    use nr_core::user::{Email, Username, permissions::RepositoryActions};
    use nr_core::{database::DatabaseConfig, repository::config::RepositoryConfigType};
    use sqlx::{PgPool, postgres::PgPoolOptions};
    use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
    use tower::ServiceExt;

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

    #[tokio::test]
    async fn docker_object_accounting_tracks_arrivals_deletion_and_tag_replacement() {
        use nr_core::database::entities::docker_object::DBDockerObject;
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        let storage = insert_storage(db.pool()).await;
        let repository = insert_docker_repository(db.pool(), storage).await;
        let root = "v2/image/manifests/latest";
        let child = "v2/image/manifests/sha256:child";
        let blob = "v2/image/blobs/sha256:blob";
        DBDockerObject::upsert(
            db.pool(),
            repository,
            root,
            10,
            &[child.into(), blob.into()],
        )
        .await
        .unwrap();
        assert_eq!(
            DBDockerObject::referenced_size(db.pool(), repository, root)
                .await
                .unwrap(),
            Some(10)
        );
        assert!(
            DBDockerObject::needs_backfill(db.pool(), repository, root)
                .await
                .unwrap()
        );
        DBDockerObject::upsert(
            db.pool(),
            repository,
            child,
            20,
            &[blob.into(), root.into()],
        )
        .await
        .unwrap();
        DBDockerObject::upsert(db.pool(), repository, blob, 30, &[])
            .await
            .unwrap();
        assert_eq!(
            DBDockerObject::referenced_size(db.pool(), repository, root)
                .await
                .unwrap(),
            Some(60)
        );
        assert!(
            !DBDockerObject::needs_backfill(db.pool(), repository, root)
                .await
                .unwrap()
        );
        DBDockerObject::insert_missing(db.pool(), repository, root, 999, &[])
            .await
            .unwrap();
        assert_eq!(
            DBDockerObject::referenced_size(db.pool(), repository, root)
                .await
                .unwrap(),
            Some(60)
        );
        let roots: Vec<String> = (0..5)
            .map(|n| format!("v2/concurrent/manifests/{n}"))
            .collect();
        for path in &roots {
            DBDockerObject::upsert(db.pool(), repository, path, 1, &[blob.into()])
                .await
                .unwrap();
        }
        let writes = roots
            .iter()
            .map(|path| DBDockerObject::upsert(db.pool(), repository, path, 2, &[]));
        for result in futures::future::join_all(writes).await {
            result.unwrap();
        }
        for path in roots {
            assert_eq!(
                DBDockerObject::referenced_size(db.pool(), repository, &path)
                    .await
                    .unwrap(),
                Some(2)
            );
        }
        assert!(
            DBDockerObject::upsert(db.pool(), repository, "overflow", u64::MAX, &[])
                .await
                .is_err()
        );
        DBDockerObject::delete_paths(db.pool(), repository, &[blob.into()])
            .await
            .unwrap();
        assert!(
            DBDockerObject::needs_backfill(db.pool(), repository, root)
                .await
                .unwrap()
        );
        assert_eq!(
            DBDockerObject::referenced_size(db.pool(), repository, root)
                .await
                .unwrap(),
            Some(30)
        );
        DBDockerObject::upsert(db.pool(), repository, root, 15, &[])
            .await
            .unwrap();
        assert_eq!(
            DBDockerObject::referenced_size(db.pool(), repository, root)
                .await
                .unwrap(),
            Some(15)
        );
        assert_eq!(
            DBDockerObject::referenced_size(db.pool(), repository, "missing")
                .await
                .unwrap(),
            None
        );
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

    async fn insert_maven_hosted_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
        let mut configs = HashMap::with_hasher(Default::default());
        configs.insert(
            MavenRepositoryConfigType::get_type_static().to_string(),
            serde_json::to_value(MavenRepositoryConfig::Hosted).expect("serialize maven config"),
        );
        let repo = NewRepository {
            name: "maven-hosted-test".into(),
            uuid: Uuid::new_v4(),
            repository_type: "maven".into(),
            configs,
        };
        repo.insert(storage_id, pool)
            .await
            .expect("insert maven hosted repository")
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

    async fn insert_docker_repository(pool: &PgPool, storage_id: Uuid) -> Uuid {
        let mut configs = HashMap::with_hasher(Default::default());
        configs.insert(
            DockerRegistryConfigType::get_type_static().to_string(),
            serde_json::to_value(DockerRegistryConfig::Hosted).expect("serialize docker config"),
        );
        let repo = NewRepository {
            name: "docker-hosted-test".into(),
            uuid: Uuid::new_v4(),
            repository_type: "docker".into(),
            configs,
        };
        repo.insert(storage_id, pool)
            .await
            .expect("insert docker repository")
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

    #[tokio::test]
    async fn api_preflight_returns_no_cors_headers() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;
        let root = tempfile::tempdir().expect("tempdir");
        let site = build_site(&db, root.path()).await;
        let app = crate::app::api::api_routes().with_state(site);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("OPTIONS")
                    .uri("/api/user/token/create")
                    .header("origin", "https://evil.example")
                    .header("access-control-request-method", "POST")
                    .body(axum::body::Body::empty())
                    .expect("build preflight request"),
            )
            .await
            .expect("send preflight request");

        assert!(
            !response
                .headers()
                .contains_key(http::header::ACCESS_CONTROL_ALLOW_ORIGIN),
            "foreign-origin preflight must not receive Access-Control-Allow-Origin"
        );
        assert!(
            !response
                .headers()
                .contains_key(http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            "foreign-origin preflight must not receive Access-Control-Allow-Credentials"
        );
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
            email: Some(Email::new("admin@example.com".into()).expect("email")),
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
        let mut webhook_security = SecuritySettings::default();
        webhook_security
            .egress
            .allowed_cidrs
            .push("127.0.0.0/8".into());
        crate::utils::egress::install(&webhook_security.egress).expect("test egress policy");
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
    async fn docker_package_listing_reports_referenced_manifest_and_blob_size() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;
        let root = tempfile::tempdir().expect("tempdir");
        let storage_id = insert_storage_at(db.pool(), root.path()).await;
        let repository_id = insert_docker_repository(db.pool(), storage_id).await;
        let fetched = chrono::Utc
            .with_ymd_and_hms(2025, 1, 2, 12, 0, 0)
            .single()
            .unwrap();
        let config_digest =
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let layer_digest =
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let manifest = serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": config_digest,
                "size": 1
            },
            "layers": [
                {
                    "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                    "digest": layer_digest,
                    "size": 2
                }
            ]
        }))
        .expect("serialize docker manifest");
        let config = b"stored config bytes";
        let layer = b"stored layer bytes";

        insert_proxy_version(
            db.pool(),
            repository_id,
            "library/alpine",
            "library/alpine",
            "latest",
            "v2/library/alpine/manifests/latest",
            manifest.len() as u64,
            fetched,
        )
        .await;

        let site = build_site(&db, root.path()).await;
        let repository = site
            .get_repository(repository_id)
            .expect("repository should be loaded");
        let storage = repository.get_storage();
        storage
            .save_file(
                repository_id,
                FileContent::from(manifest.as_slice()),
                &StoragePath::from("v2/library/alpine/manifests/latest"),
            )
            .await
            .expect("save manifest");
        storage
            .save_file(
                repository_id,
                FileContent::from(config.as_slice()),
                &StoragePath::from(format!("v2/library/alpine/blobs/{config_digest}")),
            )
            .await
            .expect("save config");
        storage
            .save_file(
                repository_id,
                FileContent::from(layer.as_slice()),
                &StoragePath::from(format!("v2/library/alpine/blobs/{layer_digest}")),
            )
            .await
            .expect("save layer");

        let response = super::list_cached_packages(
            State(site.clone()),
            Some(sample_auth()),
            Path(repository_id),
            Query(PackageListQuery {
                page: 1,
                per_page: 50,
                q: None,
                sort_by: PackageSortBy::Modified,
                sort_dir: PackageSortDirection::Desc,
            }),
        )
        .await
        .expect("list packages succeeds");

        assert_eq!(response.status(), http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json response");
        let size = payload["items"][0]["size"].as_u64().expect("size value");

        assert_eq!(
            size,
            manifest.len() as u64 + config.len() as u64 + layer.len() as u64
        );
        site.close().await;
    }

    #[tokio::test]
    async fn docker_package_listing_uses_persisted_referenced_size_without_storage_access() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;
        let root = tempfile::tempdir().expect("tempdir");
        let storage_id = insert_storage_at(db.pool(), root.path()).await;
        let repository_id = insert_docker_repository(db.pool(), storage_id).await;
        let fetched = chrono::Utc
            .with_ymd_and_hms(2025, 1, 2, 12, 0, 0)
            .single()
            .unwrap();
        insert_proxy_version(
            db.pool(),
            repository_id,
            "library/alpine",
            "library/alpine",
            "latest",
            "v2/library/alpine/manifests/latest",
            10,
            fetched,
        )
        .await;

        nr_core::database::entities::docker_object::DBDockerObject::upsert(
            db.pool(),
            repository_id,
            "v2/library/alpine/manifests/latest",
            4242,
            &[],
        )
        .await
        .expect("persist object size");

        insert_proxy_version(
            db.pool(),
            repository_id,
            "library/small",
            "library/small",
            "latest",
            "v2/library/small/manifests/latest",
            99999,
            fetched,
        )
        .await;
        nr_core::database::entities::docker_object::DBDockerObject::upsert(
            db.pool(),
            repository_id,
            "v2/library/small/manifests/latest",
            1,
            &[],
        )
        .await
        .expect("persist smaller image");

        let site = build_site(&db, root.path()).await;

        let response = super::list_cached_packages(
            State(site.clone()),
            Some(sample_auth()),
            Path(repository_id),
            Query(PackageListQuery {
                page: 1,
                per_page: 1,
                q: None,
                sort_by: PackageSortBy::Size,
                sort_dir: PackageSortDirection::Desc,
            }),
        )
        .await
        .expect("list packages succeeds");

        assert_eq!(response.status(), http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json response");
        let size = payload["items"][0]["size"].as_u64().expect("size value");

        // No manifest or blobs were stored; the size must come from the object inventory.
        assert_eq!(size, 4242);
        site.close().await;
    }

    #[tokio::test]
    async fn maven_package_listing_reports_stored_version_directory_size() {
        let _guard = DB_TEST_LOCK.lock().await;
        let db = fresh_pool().await;
        reset_database(&db).await;
        let root = tempfile::tempdir().expect("tempdir");
        let storage_id = insert_storage_at(db.pool(), root.path()).await;
        let repository_id = insert_maven_hosted_repository(db.pool(), storage_id).await;
        let version_path = "org/example/demo/1.0.0";

        insert_maven_version(
            db.pool(),
            repository_id,
            "org.example:demo",
            "1.0.0",
            version_path,
        )
        .await;

        let site = build_site(&db, root.path()).await;
        let repository = site
            .get_repository(repository_id)
            .expect("repository should be loaded");
        let storage = repository.get_storage();
        let jar = b"jar bytes";
        let pom = b"pom bytes";
        let checksum = b"checksum";
        for (name, bytes) in [
            ("demo-1.0.0.jar", jar.as_slice()),
            ("demo-1.0.0.pom", pom.as_slice()),
            ("demo-1.0.0.jar.sha1", checksum.as_slice()),
        ] {
            storage
                .save_file(
                    repository_id,
                    FileContent::from(bytes),
                    &StoragePath::from(format!("{version_path}/{name}")),
                )
                .await
                .expect("save maven file");
        }

        let response = super::list_cached_packages(
            State(site.clone()),
            Some(sample_auth()),
            Path(repository_id),
            Query(PackageListQuery {
                page: 1,
                per_page: 50,
                q: None,
                sort_by: PackageSortBy::Modified,
                sort_dir: PackageSortDirection::Desc,
            }),
        )
        .await
        .expect("list packages succeeds");

        assert_eq!(response.status(), http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json response");
        let size = payload["items"][0]["size"].as_u64().expect("size value");

        assert_eq!(
            size,
            jar.len() as u64 + pom.len() as u64 + checksum.len() as u64
        );
        site.close().await;
    }
}
