use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::VersionData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMetadataKind {
    ProxyArtifact,
}

impl ProxyMetadataKind {
    pub const fn proxy_artifact() -> Self {
        ProxyMetadataKind::ProxyArtifact
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyArtifactMeta {
    #[serde(rename = "type", default = "ProxyMetadataKind::proxy_artifact")]
    kind: ProxyMetadataKind,
    pub package_name: String,
    pub package_key: String,
    pub version: Option<String>,
    pub cache_path: String,
    pub upstream_digest: Option<String>,
    pub upstream_url: Option<String>,
    pub size: Option<u64>,
    pub fetched_at: DateTime<Utc>,
}

impl ProxyArtifactMeta {
    pub fn builder(
        package_name: impl Into<String>,
        package_key: impl Into<String>,
        cache_path: impl Into<String>,
    ) -> ProxyArtifactMetaBuilder {
        ProxyArtifactMetaBuilder {
            package_name: package_name.into(),
            package_key: package_key.into(),
            cache_path: cache_path.into(),
            version: None,
            upstream_digest: None,
            upstream_url: None,
            size: None,
            fetched_at: None,
        }
    }

    pub fn kind(&self) -> ProxyMetadataKind {
        self.kind
    }
}

#[derive(Debug, Clone)]
pub struct ProxyArtifactMetaBuilder {
    package_name: String,
    package_key: String,
    cache_path: String,
    version: Option<String>,
    upstream_digest: Option<String>,
    upstream_url: Option<String>,
    size: Option<u64>,
    fetched_at: Option<DateTime<Utc>>,
}

impl ProxyArtifactMetaBuilder {
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn upstream_digest(mut self, digest: impl Into<String>) -> Self {
        self.upstream_digest = Some(digest.into());
        self
    }

    pub fn upstream_url(mut self, url: impl Into<String>) -> Self {
        self.upstream_url = Some(url.into());
        self
    }

    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn fetched_at(mut self, fetched_at: DateTime<Utc>) -> Self {
        self.fetched_at = Some(fetched_at);
        self
    }

    pub fn build(self) -> ProxyArtifactMeta {
        ProxyArtifactMeta {
            kind: ProxyMetadataKind::proxy_artifact(),
            package_name: self.package_name,
            package_key: self.package_key,
            version: self.version,
            cache_path: self.cache_path,
            upstream_digest: self.upstream_digest,
            upstream_url: self.upstream_url,
            size: self.size,
            fetched_at: self.fetched_at.unwrap_or_else(Utc::now),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyArtifactKey {
    pub package_key: String,
    pub version: Option<String>,
    pub cache_path: Option<String>,
}

impl ProxyArtifactKey {
    pub fn from_meta(meta: &ProxyArtifactMeta) -> Self {
        Self {
            package_key: meta.package_key.clone(),
            version: meta.version.clone(),
            cache_path: Some(meta.cache_path.clone()),
        }
    }
}

impl VersionData {
    pub fn set_proxy_artifact(
        &mut self,
        meta: &ProxyArtifactMeta,
    ) -> Result<(), serde_json::Error> {
        self.extra = Some(serde_json::to_value(meta)?);
        Ok(())
    }

    pub fn proxy_artifact(&self) -> Option<ProxyArtifactMeta> {
        self.extra.as_ref().and_then(|value| {
            serde_json::from_value(value.clone())
                .ok()
                .filter(|meta: &ProxyArtifactMeta| meta.kind == ProxyMetadataKind::proxy_artifact())
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
    use chrono::{TimeZone, Utc};

    use super::*;

    fn sample_meta() -> ProxyArtifactMeta {
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 1, 0, 0, 0).single().unwrap();
        ProxyArtifactMeta::builder("numpy", "numpy", "packages/numpy/numpy-2.1.0.whl")
            .version("2.1.0")
            .size(4_096)
            .upstream_digest("sha256:deadbeef")
            .fetched_at(timestamp)
            .build()
    }

    #[test]
    fn proxy_artifact_meta_round_trip() {
        let meta = sample_meta();
        let json = serde_json::to_value(&meta).expect("serialize metadata");
        assert_eq!(json.get("type").unwrap(), "proxy_artifact");

        let parsed: ProxyArtifactMeta = serde_json::from_value(json).expect("deserialize metadata");
        assert_eq!(parsed, meta);
    }

    #[test]
    fn version_data_helpers_store_proxy_meta() {
        let mut data = VersionData::default();
        let meta = sample_meta();
        data.set_proxy_artifact(&meta).expect("set proxy metadata");
        let recovered = data.proxy_artifact().expect("proxy metadata present");
        assert_eq!(recovered, meta);
    }

    #[test]
    fn proxy_artifact_key_extracts_identifiers() {
        let meta = sample_meta();
        let key = ProxyArtifactKey::from_meta(&meta);
        assert_eq!(key.package_key, "numpy");
        assert_eq!(key.version.as_deref(), Some("2.1.0"));
        assert_eq!(
            key.cache_path.as_deref(),
            Some("packages/numpy/numpy-2.1.0.whl")
        );
    }
}
