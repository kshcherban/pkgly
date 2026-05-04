// ABOUTME: Collects Docker registry metadata for listings, browse views, and size reporting.
// ABOUTME: Resolves manifest paths and computes referenced Docker content sizes.
use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, FixedOffset};
use nr_core::storage::StoragePath;
use nr_storage::{DynStorage, FileType, Storage, StorageError, StorageFile, s3::S3Storage};
use uuid::Uuid;

use super::types::{Descriptor, Manifest, ManifestDescriptor, MediaType};

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

                                let calculated_size = calculate_referenced_manifest_size(
                                    storage,
                                    repository_id,
                                    &manifest_path,
                                )
                                .await?
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

async fn read_manifest_file(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &StoragePath,
) -> Result<Option<(Vec<u8>, u64)>, StorageError> {
    let manifest_file = match storage.open_file(repository_id, path).await {
        Ok(Some(file)) => file,
        Ok(None) => return Ok(None),
        Err(err) => return Err(err),
    };

    let StorageFile::File { meta, content } = manifest_file else {
        return Ok(None);
    };

    let size_hint = usize::try_from(meta.file_type.file_size).unwrap_or(0);
    match content.read_to_vec(size_hint).await {
        Ok(bytes) => Ok(Some((bytes, meta.file_type.file_size))),
        Err(err) => {
            tracing::warn!(?err, manifest_path = %path, "Failed to read manifest content");
            Ok(None)
        }
    }
}

pub(crate) async fn calculate_referenced_manifest_size(
    storage: &DynStorage,
    repository_id: Uuid,
    manifest_path: &StoragePath,
) -> Result<Option<u64>, StorageError> {
    let manifest_path_string = manifest_path.to_string();
    let Some((repository_name, _)) = split_manifest_cache_path(&manifest_path_string) else {
        return Ok(None);
    };
    let Some((bytes, manifest_size)) =
        read_manifest_file(storage, repository_id, manifest_path).await?
    else {
        return Ok(None);
    };
    let Some(manifest) = parse_manifest(&bytes) else {
        return Ok(None);
    };

    let mut total = manifest_size;
    let mut seen_blobs = HashSet::new();
    let mut seen_manifests = HashSet::new();
    let mut pending_manifests = Vec::new();

    add_manifest_payload_sizes(
        storage,
        repository_id,
        &repository_name,
        manifest,
        &mut total,
        &mut seen_blobs,
        &mut pending_manifests,
    )
    .await?;

    while let Some(descriptor) = pending_manifests.pop() {
        if !seen_manifests.insert(descriptor.digest.clone()) {
            continue;
        }

        let child_path = StoragePath::from(format!(
            "v2/{}/manifests/{}",
            repository_name, descriptor.digest
        ));
        let Some((child_bytes, child_size)) =
            read_manifest_file(storage, repository_id, &child_path).await?
        else {
            continue;
        };

        total += child_size;
        if let Some(child_manifest) = parse_manifest(&child_bytes) {
            add_manifest_payload_sizes(
                storage,
                repository_id,
                &repository_name,
                child_manifest,
                &mut total,
                &mut seen_blobs,
                &mut pending_manifests,
            )
            .await?;
        }
    }

    Ok(Some(total))
}

fn parse_manifest(bytes: &[u8]) -> Option<Manifest> {
    if bytes.is_empty() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let media_type = value
        .get("mediaType")
        .and_then(|v| v.as_str())
        .unwrap_or(MediaType::OCI_IMAGE_MANIFEST);

    Manifest::from_bytes(bytes, media_type).ok()
}

async fn add_manifest_payload_sizes(
    storage: &DynStorage,
    repository_id: Uuid,
    repository_name: &str,
    manifest: Manifest,
    total: &mut u64,
    seen_blobs: &mut HashSet<String>,
    pending_manifests: &mut Vec<ManifestDescriptor>,
) -> Result<(), StorageError> {
    match manifest {
        Manifest::DockerV2(manifest) => {
            add_blob_descriptor_size(
                storage,
                repository_id,
                repository_name,
                &manifest.config,
                total,
                seen_blobs,
            )
            .await?;
            for layer in manifest.layers.iter() {
                add_blob_descriptor_size(
                    storage,
                    repository_id,
                    repository_name,
                    layer,
                    total,
                    seen_blobs,
                )
                .await?;
            }
        }
        Manifest::OciImage(manifest) => {
            if let Some(config) = manifest.config.as_ref() {
                add_blob_descriptor_size(
                    storage,
                    repository_id,
                    repository_name,
                    config,
                    total,
                    seen_blobs,
                )
                .await?;
            }
            for layer in manifest.layers.iter() {
                add_blob_descriptor_size(
                    storage,
                    repository_id,
                    repository_name,
                    layer,
                    total,
                    seen_blobs,
                )
                .await?;
            }
        }
        Manifest::OciIndex(index) => {
            pending_manifests.extend(index.manifests);
        }
    }
    Ok(())
}

async fn add_blob_descriptor_size(
    storage: &DynStorage,
    repository_id: Uuid,
    repository_name: &str,
    descriptor: &Descriptor,
    total: &mut u64,
    seen_blobs: &mut HashSet<String>,
) -> Result<(), StorageError> {
    if !seen_blobs.insert(descriptor.digest.clone()) {
        return Ok(());
    }

    let blob_path = StoragePath::from(format!(
        "v2/{}/blobs/{}",
        repository_name, descriptor.digest
    ));
    if let Some(StorageFile::File { meta, .. }) =
        storage.open_file(repository_id, &blob_path).await?
    {
        *total += meta.file_type.file_size;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
