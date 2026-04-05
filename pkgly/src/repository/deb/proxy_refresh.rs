use bytes::Bytes;
use http::StatusCode;
use nr_core::{repository::proxy_url::ProxyURL, storage::StoragePath};
use nr_storage::{DynStorage, FileContent, Storage};
use serde::Serialize;
use sha2::Digest;
use thiserror::Error;
use tracing::{debug, warn};
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    configs::{DebProxyConfig, DebProxyLayout},
    proxy_indexing::{DebProxyIndexing, DebProxyIndexingError},
};

#[derive(Debug, Error)]
pub enum DebProxyRefreshError {
    #[error(transparent)]
    Indexing(#[from] DebProxyIndexingError),
    #[error(transparent)]
    Storage(#[from] nr_storage::StorageError),
    #[error(transparent)]
    Upstream(#[from] reqwest::Error),
    #[error("invalid upstream url")]
    InvalidUpstreamUrl,
    #[error("upstream returned status {0}")]
    UpstreamStatus(StatusCode),
    #[error("failed to decode Packages index: {0}")]
    InvalidPackagesIndex(String),
    #[error("package size mismatch for {path}: expected {expected} got {actual}")]
    PackageSizeMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },
    #[error("sha256 mismatch for {path}: expected {expected} got {actual}")]
    PackageSha256Mismatch {
        path: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct DebProxyRefreshSummary {
    pub downloaded_packages: usize,
    pub downloaded_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackagesIndexEntry {
    filename: String,
    size: Option<u64>,
    sha256: Option<String>,
}

fn build_upstream_url(base: &ProxyURL, path: &StoragePath) -> Option<Url> {
    let mut upstream = Url::parse(base.as_ref()).ok()?;
    let base_path = upstream.path().trim_end_matches('/').to_string();
    let request_path = path.to_string();
    let request_path = request_path.trim_start_matches('/').to_string();

    let combined = if base_path.is_empty() || base_path == "/" {
        format!("/{}", request_path)
    } else if request_path.is_empty() {
        base_path
    } else {
        format!("{}/{}", base_path, request_path)
    };

    upstream.set_path(&combined);
    Some(upstream)
}

async fn fetch_upstream_bytes(
    client: &reqwest::Client,
    upstream: &ProxyURL,
    path: &StoragePath,
) -> Result<Option<Bytes>, DebProxyRefreshError> {
    let Some(url) = build_upstream_url(upstream, path) else {
        return Err(DebProxyRefreshError::InvalidUpstreamUrl);
    };
    let response = crate::utils::upstream::send(client, client.get(url.clone())).await?;
    let status = response.status();
    if status == StatusCode::NOT_FOUND {
        debug!(
            url.full = %crate::utils::upstream::sanitize_url_for_logging(&url),
            path = %path.to_string(),
            "Upstream returned 404 for refresh target"
        );
        return Ok(None);
    }
    if !status.is_success() {
        warn!(
            url.full = %crate::utils::upstream::sanitize_url_for_logging(&url),
            status = ?status,
            "Upstream returned non-success during refresh"
        );
        return Err(DebProxyRefreshError::UpstreamStatus(status));
    }
    Ok(Some(response.bytes().await?))
}

async fn save_bytes(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &StoragePath,
    bytes: Bytes,
) -> Result<(), nr_storage::StorageError> {
    storage
        .save_file(repository_id, FileContent::Bytes(bytes), path)
        .await?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn by_hash_path_for_index(index_path: &StoragePath, sha256: &str) -> Option<StoragePath> {
    let mut segments: Vec<String> = index_path.clone().into_iter().map(String::from).collect();
    if segments.len() < 2 {
        return None;
    }
    let _file = segments.pop()?;
    segments.push("by-hash".to_string());
    segments.push("SHA256".to_string());
    segments.push(sha256.to_string());
    Some(StoragePath::from(segments.join("/")))
}

fn parse_packages_index(bytes: &Bytes) -> Result<Vec<PackagesIndexEntry>, DebProxyRefreshError> {
    let contents = std::str::from_utf8(bytes).map_err(|err| {
        DebProxyRefreshError::InvalidPackagesIndex(format!("packages index is not utf8: {err}"))
    })?;
    let mut entries = Vec::new();
    for stanza in contents.split("\n\n") {
        let stanza = stanza.trim();
        if stanza.is_empty() {
            continue;
        }
        let control = super::metadata::ControlFile::parse(stanza).map_err(|err| {
            DebProxyRefreshError::InvalidPackagesIndex(format!("failed to parse stanza: {err}"))
        })?;
        let Some(filename) = control.get("Filename") else {
            continue;
        };
        let size = control
            .get("Size")
            .and_then(|value| value.parse::<u64>().ok());
        let sha256 = control.get("SHA256").map(str::to_owned);
        entries.push(PackagesIndexEntry {
            filename: filename.to_string(),
            size,
            sha256,
        });
    }
    Ok(entries)
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, DebProxyRefreshError> {
    use std::io::Read;

    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|err| {
        DebProxyRefreshError::InvalidPackagesIndex(format!("failed to decode Packages.gz: {err}"))
    })?;
    Ok(out)
}

async fn download_and_store_package(
    client: &reqwest::Client,
    storage: &DynStorage,
    repository_id: Uuid,
    upstream: &ProxyURL,
    entry: &PackagesIndexEntry,
) -> Result<Option<Bytes>, DebProxyRefreshError> {
    let path = StoragePath::from(entry.filename.as_str());
    if storage
        .get_file_information(repository_id, &path)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let Some(bytes) = fetch_upstream_bytes(client, upstream, &path).await? else {
        return Ok(None);
    };
    if let Some(expected) = entry.size {
        let actual = bytes.len() as u64;
        if expected != actual {
            return Err(DebProxyRefreshError::PackageSizeMismatch {
                path: entry.filename.clone(),
                expected,
                actual,
            });
        }
    }
    if let Some(expected) = entry.sha256.as_deref() {
        let actual = sha256_hex(&bytes);
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(DebProxyRefreshError::PackageSha256Mismatch {
                path: entry.filename.clone(),
                expected: expected.to_string(),
                actual,
            });
        }
    }
    save_bytes(storage, repository_id, &path, bytes.clone()).await?;
    Ok(Some(bytes))
}

pub async fn refresh_deb_proxy_offline_mirror(
    client: &reqwest::Client,
    storage: &DynStorage,
    repository_id: Uuid,
    config: &DebProxyConfig,
    indexer: &dyn DebProxyIndexing,
) -> Result<DebProxyRefreshSummary, DebProxyRefreshError> {
    let mut downloaded_files = 0usize;
    let mut downloaded_packages = 0usize;

    match &config.layout {
        DebProxyLayout::Dists(layout) => {
            for distribution in &layout.distributions {
                for path in [
                    format!("dists/{distribution}/Release"),
                    format!("dists/{distribution}/InRelease"),
                    format!("dists/{distribution}/Release.gpg"),
                ] {
                    let path = StoragePath::from(path);
                    if let Some(bytes) =
                        fetch_upstream_bytes(client, &config.upstream_url, &path).await?
                    {
                        save_bytes(storage, repository_id, &path, bytes).await?;
                        downloaded_files += 1;
                    }
                }

                for component in &layout.components {
                    for architecture in &layout.architectures {
                        let packages_path = StoragePath::from(format!(
                            "dists/{distribution}/{component}/binary-{architecture}/Packages"
                        ));
                        let Some(packages_bytes) =
                            fetch_upstream_bytes(client, &config.upstream_url, &packages_path)
                                .await?
                        else {
                            continue;
                        };
                        save_bytes(
                            storage,
                            repository_id,
                            &packages_path,
                            packages_bytes.clone(),
                        )
                        .await?;
                        downloaded_files += 1;

                        let hash = sha256_hex(&packages_bytes);
                        if let Some(by_hash) = by_hash_path_for_index(&packages_path, &hash) {
                            save_bytes(storage, repository_id, &by_hash, packages_bytes.clone())
                                .await?;
                            downloaded_files += 1;
                        }

                        let entries = parse_packages_index(&packages_bytes)?;
                        for entry in entries {
                            if !entry.filename.to_ascii_lowercase().ends_with(".deb") {
                                continue;
                            }
                            let Some(bytes) = download_and_store_package(
                                client,
                                storage,
                                repository_id,
                                &config.upstream_url,
                                &entry,
                            )
                            .await?
                            else {
                                continue;
                            };
                            downloaded_files += 1;
                            downloaded_packages += 1;
                            super::proxy_indexing::record_deb_proxy_cache_hit(
                                indexer,
                                &StoragePath::from(entry.filename.as_str()),
                                bytes,
                                None,
                            )
                            .await?;
                        }
                    }
                }
            }
        }
        DebProxyLayout::Flat(layout) => {
            let prefix = layout.distribution.trim();
            let normalized = prefix
                .trim_start_matches("./")
                .trim_start_matches('.')
                .trim_matches('/');
            let packages_path = if normalized.is_empty() {
                StoragePath::from("Packages")
            } else {
                StoragePath::from(format!("{normalized}/Packages"))
            };
            let packages_gz_path = StoragePath::from(format!("{}.gz", packages_path.to_string()));

            let mut parse_bytes: Option<Bytes> = None;

            if let Some(bytes) =
                fetch_upstream_bytes(client, &config.upstream_url, &packages_path).await?
            {
                save_bytes(storage, repository_id, &packages_path, bytes.clone()).await?;
                downloaded_files += 1;
                parse_bytes = Some(bytes);
            }

            if let Some(gz_bytes) =
                fetch_upstream_bytes(client, &config.upstream_url, &packages_gz_path).await?
            {
                save_bytes(storage, repository_id, &packages_gz_path, gz_bytes.clone()).await?;
                downloaded_files += 1;

                if parse_bytes.is_none() {
                    let decoded = gunzip(&gz_bytes)?;
                    let decoded_bytes = Bytes::from(decoded);
                    save_bytes(
                        storage,
                        repository_id,
                        &packages_path,
                        decoded_bytes.clone(),
                    )
                    .await?;
                    downloaded_files += 1;
                    parse_bytes = Some(decoded_bytes);
                }
            }

            let Some(packages_bytes) = parse_bytes else {
                return Ok(DebProxyRefreshSummary {
                    downloaded_packages: 0,
                    downloaded_files,
                });
            };

            let entries = parse_packages_index(&packages_bytes)?;
            for entry in entries {
                if !entry.filename.to_ascii_lowercase().ends_with(".deb") {
                    continue;
                }
                let Some(bytes) = download_and_store_package(
                    client,
                    storage,
                    repository_id,
                    &config.upstream_url,
                    &entry,
                )
                .await?
                else {
                    continue;
                };
                downloaded_files += 1;
                downloaded_packages += 1;
                super::proxy_indexing::record_deb_proxy_cache_hit(
                    indexer,
                    &StoragePath::from(entry.filename.as_str()),
                    bytes,
                    None,
                )
                .await?;
            }
        }
    }

    Ok(DebProxyRefreshSummary {
        downloaded_packages,
        downloaded_files,
    })
}

#[cfg(test)]
mod tests;
