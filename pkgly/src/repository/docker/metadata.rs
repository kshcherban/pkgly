use std::collections::VecDeque;

use chrono::{DateTime, FixedOffset};
use nr_core::storage::StoragePath;
use nr_storage::{DynStorage, FileType, Storage, StorageError, StorageFile, s3::S3Storage};
use uuid::Uuid;

use super::types::{Manifest, MediaType};

/// Represents a manifest (tag or digest) stored for a Docker image.
#[derive(Debug, Clone)]
pub struct DockerManifestEntry {
    pub repository: String,
    pub reference: String,
    pub cache_path: String,
    pub size: u64,
    pub modified: DateTime<FixedOffset>,
}

/// Resolve a logical browse path (as used by the frontend) into the underlying
/// storage path used by the registry implementation.
///
/// The Docker registry stores all content under the `v2/` namespace with
/// manifests for a repository located at `v2/<repository>/manifests/<tag>`.
/// This helper hides the internal `v2/` and `manifests/` segments when browsing.
pub async fn resolve_browse_path(
    storage: &DynStorage,
    repository_id: Uuid,
    logical_path: &StoragePath,
) -> Result<StoragePath, StorageError> {
    let mut physical = StoragePath::from("v2");

    let logical_string = logical_path.to_string();
    for segment in logical_string
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        physical.push_mut(segment);
    }

    let mut manifest_candidate = physical.clone();
    manifest_candidate.push_mut("manifests");

    if storage
        .file_exists(repository_id, &manifest_candidate)
        .await?
    {
        Ok(manifest_candidate)
    } else {
        Ok(physical)
    }
}

/// Collect every manifest stored in the registry along with metadata required
/// for administrative listings or search results.
pub async fn collect_manifest_entries(
    storage: &DynStorage,
    repository_id: Uuid,
) -> Result<Vec<DockerManifestEntry>, StorageError> {
    if let DynStorage::S3(s3) = storage {
        return collect_manifest_entries_s3(s3, repository_id).await;
    }

    let mut queue: VecDeque<(Vec<String>, StoragePath)> = VecDeque::new();
    queue.push_back((Vec::new(), StoragePath::from("v2")));

    let mut manifests = Vec::new();

    while let Some((segments, path)) = queue.pop_front() {
        let Some(StorageFile::Directory { files, .. }) =
            storage.open_file(repository_id, &path).await?
        else {
            continue;
        };

        for entry in files {
            let nr_storage::StorageFileMeta {
                name, file_type, ..
            } = entry;

            match file_type {
                FileType::Directory(_) => match name.as_str() {
                    "manifests" => {
                        let mut manifests_path = path.clone();
                        manifests_path.push_mut("manifests");

                        let Some(StorageFile::Directory { files, .. }) =
                            storage.open_file(repository_id, &manifests_path).await?
                        else {
                            continue;
                        };

                        let repository_name = segments.join("/");

                        for manifest in files {
                            if manifest.name.ends_with(".nr-docker-tagmeta") {
                                continue;
                            }
                            if let FileType::File(file_meta) = manifest.file_type {
                                let mut manifest_path = manifests_path.clone();
                                manifest_path.push_mut(&manifest.name);

                                let manifest_bytes =
                                    read_manifest_bytes(storage, repository_id, &manifest_path)
                                        .await;
                                let calculated_size = calculate_manifest_size(&manifest_bytes)
                                    .unwrap_or(file_meta.file_size);

                                manifests.push(DockerManifestEntry {
                                    repository: repository_name.clone(),
                                    reference: manifest.name.clone(),
                                    cache_path: manifest_path.to_string(),
                                    size: calculated_size,
                                    modified: manifest.modified,
                                });
                            }
                        }
                    }
                    "blobs" | "uploads" | "_uploads" => {
                        // Skip internal storage directories that do not represent images.
                    }
                    other => {
                        let mut next_segments = segments.clone();
                        next_segments.push(other.to_string());

                        let mut next_path = path.clone();
                        next_path.push_mut(other);
                        queue.push_back((next_segments, next_path));
                    }
                },
                FileType::File(_) => {
                    // Files at this level are not expected (only manifests reside in sub-directories).
                }
            }
        }
    }

    Ok(manifests)
}

pub fn split_manifest_cache_path(path: &str) -> Option<(String, String)> {
    if !path.starts_with("v2/") {
        return None;
    }
    let without_prefix = &path[3..];
    let marker = "/manifests/";
    let split_index = without_prefix.find(marker)?;
    let repository = &without_prefix[..split_index];
    let reference = &without_prefix[split_index + marker.len()..];
    if repository.is_empty() || reference.is_empty() {
        return None;
    }
    Some((repository.to_string(), reference.to_string()))
}

pub fn docker_package_key(repository_name: &str) -> String {
    repository_name.trim_matches('/').to_ascii_lowercase()
}

/// S3-optimized manifest listing: uses ListObjectsV2 and avoids fetching manifest bodies.
async fn collect_manifest_entries_s3(
    storage: &S3Storage,
    repository_id: Uuid,
) -> Result<Vec<DockerManifestEntry>, StorageError> {
    let objects = storage.list_docker_manifests(repository_id).await?;

    let now = chrono::Local::now().fixed_offset();

    let mut entries = objects
        .into_iter()
        .filter_map(|obj| {
            let repo_relative = obj.key.strip_prefix("v2/").unwrap_or(&obj.key);
            let (repository, reference) = repo_relative.split_once("/manifests/")?;
            if repository.is_empty() || reference.is_empty() {
                return None;
            }
            if reference.ends_with(".nr-docker-tagmeta") {
                return None;
            }
            Some(DockerManifestEntry {
                repository: repository.to_string(),
                reference: reference.to_string(),
                cache_path: obj.key,
                size: obj.size,
                modified: obj.last_modified.unwrap_or(now),
            })
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| {
        a.repository
            .cmp(&b.repository)
            .then(a.reference.cmp(&b.reference))
    });
    Ok(entries)
}

async fn read_manifest_bytes(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &StoragePath,
) -> Vec<u8> {
    let manifest_file = match storage.open_file(repository_id, path).await {
        Ok(Some(file)) => file,
        Ok(None) => return Vec::new(),
        Err(err) => {
            tracing::warn!(?err, manifest_path = %path, "Failed to open manifest file");
            return Vec::new();
        }
    };

    let StorageFile::File { meta, content } = manifest_file else {
        return Vec::new();
    };

    let size_hint = usize::try_from(meta.file_type.file_size).unwrap_or(0);
    match content.read_to_vec(size_hint).await {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!(?err, manifest_path = %path, "Failed to read manifest content");
            Vec::new()
        }
    }
}

fn calculate_manifest_size(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let media_type = value
        .get("mediaType")
        .and_then(|v| v.as_str())
        .unwrap_or(MediaType::OCI_IMAGE_MANIFEST);

    let manifest = Manifest::from_bytes(bytes, media_type).ok()?;
    Some(match manifest {
        Manifest::DockerV2(manifest) => {
            let mut total = normalize_size(manifest.config.size);
            for layer in manifest.layers {
                total += normalize_size(layer.size);
            }
            total
        }
        Manifest::OciImage(manifest) => {
            let mut total = manifest
                .config
                .as_ref()
                .map(|descriptor| normalize_size(descriptor.size))
                .unwrap_or(0);
            for layer in manifest.layers {
                total += normalize_size(layer.size);
            }
            total
        }
        Manifest::OciIndex(index) => index.manifests.into_iter().fold(0u64, |acc, descriptor| {
            acc + normalize_size(descriptor.size)
        }),
    })
}

fn normalize_size(size: i64) -> u64 {
    u64::try_from(size.max(0)).unwrap_or(0)
}
