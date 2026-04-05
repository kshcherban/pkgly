use axum::http::Uri;
use nr_core::storage::StoragePath;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Errors that can occur while handling Cargo helper utilities.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CargoUtilError {
    #[error("Invalid crate name: {0}")]
    InvalidCrateName(String),
    #[error("Publish payload is truncated")]
    TruncatedPayload,
    #[error("Publish metadata length mismatch")]
    MetadataLengthMismatch,
    #[error("Crate archive length mismatch")]
    ArchiveLengthMismatch,
    #[error("Invalid publish metadata: {0}")]
    InvalidPublishMetadata(String),
}

/// Metadata extracted from a publish request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishMetadata {
    pub name: String,
    pub vers: semver::Version,
    #[serde(default)]
    pub deps: Vec<PublishDependency>,
    #[serde(default)]
    pub features: ahash::HashMap<String, Vec<String>>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub license_file: Option<String>,
    #[serde(default)]
    pub readme: Option<String>,
    #[serde(default)]
    pub readme_file: Option<String>,
    #[serde(default)]
    pub badges: ahash::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub links: Option<String>,
    #[serde(default)]
    pub v: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishDependency {
    pub name: String,
    pub vers: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default_features: bool,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub registry: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
}

/// Parsed representation of a publish payload.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishPayload {
    pub metadata: PublishMetadata,
    pub crate_archive: Vec<u8>,
}

pub fn normalize_crate_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

pub fn crate_index_relative_path(crate_name: &str) -> Result<String, CargoUtilError> {
    let normalized = normalize_crate_name(crate_name);
    if normalized.is_empty()
        || !normalized
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
    {
        return Err(CargoUtilError::InvalidCrateName(crate_name.to_string()));
    }

    let bytes = normalized.as_bytes();
    let path = match bytes.len() {
        1 => format!("1/{}", normalized),
        2 => format!("2/{}", normalized),
        3 => {
            let first = &normalized[0..1];
            format!("3/{first}/{normalized}")
        }
        _ => {
            let first_two = &normalized[0..2];
            let next_two = &normalized[2..std::cmp::min(4, bytes.len())];
            format!("{first_two}/{next_two}/{normalized}")
        }
    };
    Ok(path)
}

pub fn crate_archive_storage_path(crate_name: &str, version: &semver::Version) -> StoragePath {
    let normalized = normalize_crate_name(crate_name);
    let file_name = format!("{normalized}-{}.crate", version);
    StoragePath::from(format!(
        "crates/{normalized}/{version}/{file_name}",
        version = version
    ))
}

pub fn sparse_index_storage_path(crate_name: &str) -> Result<StoragePath, CargoUtilError> {
    let relative = crate_index_relative_path(crate_name)?;
    Ok(StoragePath::from(format!("index/{relative}")))
}

pub fn parse_publish_payload(body: &[u8]) -> Result<PublishPayload, CargoUtilError> {
    if body.len() < 8 {
        return Err(CargoUtilError::TruncatedPayload);
    }

    let mut offset = 0usize;
    let metadata_len = u32::from_le_bytes(
        body[offset..offset + 4]
            .try_into()
            .map_err(|_| CargoUtilError::TruncatedPayload)?,
    ) as usize;
    offset += 4;

    if body.len() < offset + metadata_len {
        return Err(CargoUtilError::MetadataLengthMismatch);
    }
    let metadata_bytes = &body[offset..offset + metadata_len];
    offset += metadata_len;

    if body.len() < offset + 4 {
        return Err(CargoUtilError::TruncatedPayload);
    }
    let crate_len = u32::from_le_bytes(
        body[offset..offset + 4]
            .try_into()
            .map_err(|_| CargoUtilError::TruncatedPayload)?,
    ) as usize;
    offset += 4;

    if body.len() < offset + crate_len {
        return Err(CargoUtilError::ArchiveLengthMismatch);
    }

    let crate_archive = body[offset..offset + crate_len].to_vec();

    let metadata: PublishMetadata = serde_json::from_slice(metadata_bytes)
        .map_err(|err| CargoUtilError::InvalidPublishMetadata(err.to_string()))?;

    Ok(PublishPayload {
        metadata,
        crate_archive,
    })
}

pub fn build_index_entry(metadata: &PublishMetadata, checksum: &str) -> serde_json::Value {
    let deps: Vec<serde_json::Value> = metadata
        .deps
        .iter()
        .map(|dep| {
            json!({
                "name": dep.name,
                "req": dep.vers.clone().unwrap_or_else(|| "*".into()),
                "features": dep.features.clone(),
                "optional": dep.optional,
                "default_features": dep.default_features,
                "target": dep.target.clone(),
                "kind": dep.kind.clone(),
                "registry": dep.registry.clone(),
                "package": dep.package.clone(),
                "explicit_name_in_toml": dep.package.is_some(),
            })
        })
        .collect();
    json!({
        "name": metadata.name.clone(),
        "vers": metadata.vers.to_string(),
        "deps": deps,
        "cksum": checksum,
        "features": metadata.features.clone(),
        "yanked": false,
        "links": metadata.links.clone(),
        "v": metadata.v.unwrap_or(2),
    })
}

pub fn build_config_json(
    base_url: &Uri,
    storage_name: &str,
    repository_name: &str,
    auth_required: bool,
) -> serde_json::Value {
    let mut base = base_url.to_string();
    if base.ends_with('/') {
        base.truncate(base.trim_end_matches('/').len());
    }
    let repository_base = format!("{base}/repositories/{storage_name}/{repository_name}");
    let api = repository_base.clone();
    let dl = format!("{repository_base}/api/v1/crates");
    let index = format!("sparse+{repository_base}/index");
    json!({
        "dl": dl,
        "api": api,
        "index": index,
        "auth-required": auth_required,
    })
}

pub fn build_login_response(base_url: &Uri) -> serde_json::Value {
    let mut base = base_url.to_string();
    if base.ends_with('/') {
        base.truncate(base.trim_end_matches('/').len());
    }
    json!({
        "message": "Use Pkgly UI to generate an API token.",
        "token_help_url": format!("{base}/app/settings/tokens"),
        "documentation": "https://doc.rust-lang.org/cargo/reference/registries.html#logging-in",
    })
}

#[cfg(test)]
mod tests;
