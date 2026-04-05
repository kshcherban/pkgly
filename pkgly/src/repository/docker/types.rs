//! Docker and OCI image format type definitions

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Docker Image Manifest V2, Schema 2
/// https://docs.docker.com/registry/spec/manifest-v2-2/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageManifestV2 {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,

    #[serde(rename = "mediaType")]
    pub media_type: String,

    pub config: Descriptor,

    pub layers: Vec<Descriptor>,
}

/// OCI Image Manifest
/// https://github.com/opencontainers/image-spec/blob/main/manifest.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciImageManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,

    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Descriptor>,

    #[serde(default)]
    pub layers: Vec<Descriptor>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ahash::HashMap<String, String>>,
}

/// OCI Image Index (Manifest List)
/// Used for multi-platform images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciImageIndex {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,

    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    pub manifests: Vec<ManifestDescriptor>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ahash::HashMap<String, String>>,
}

/// Content descriptor for manifests, configs, and layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,

    pub digest: String,

    pub size: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ahash::HashMap<String, String>>,
}

/// Manifest descriptor with platform information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,

    pub digest: String,

    pub size: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ahash::HashMap<String, String>>,
}

/// Platform information for multi-arch images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub architecture: String,
    pub os: String,

    #[serde(rename = "os.version", skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,

    #[serde(rename = "os.features", skip_serializing_if = "Option::is_none")]
    pub os_features: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// Generic manifest that can be any type
#[derive(Debug, Clone)]
pub enum Manifest {
    DockerV2(ImageManifestV2),
    OciImage(OciImageManifest),
    OciIndex(OciImageIndex),
}

impl Manifest {
    /// Parse a manifest from JSON
    pub fn from_bytes(bytes: &[u8], content_type: &str) -> Result<Self, serde_json::Error> {
        match content_type {
            MediaType::DOCKER_MANIFEST_V2 | MediaType::DOCKER_MANIFEST_V2_ALT => {
                let manifest: ImageManifestV2 = serde_json::from_slice(bytes)?;
                Ok(Manifest::DockerV2(manifest))
            }
            MediaType::OCI_IMAGE_MANIFEST => {
                let manifest: OciImageManifest = serde_json::from_slice(bytes)?;
                Ok(Manifest::OciImage(manifest))
            }
            MediaType::OCI_IMAGE_INDEX | MediaType::DOCKER_MANIFEST_LIST => {
                let index: OciImageIndex = serde_json::from_slice(bytes)?;
                Ok(Manifest::OciIndex(index))
            }
            _ => {
                // Try to detect from content
                if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                    if let Some(media_type) = value.get("mediaType").and_then(|v| v.as_str()) {
                        return Self::from_bytes(bytes, media_type);
                    }
                }
                // Default to OCI manifest
                let manifest: OciImageManifest = serde_json::from_slice(bytes)?;
                Ok(Manifest::OciImage(manifest))
            }
        }
    }

    /// Get the media type for this manifest
    pub fn media_type(&self) -> &str {
        match self {
            Manifest::DockerV2(_) => MediaType::DOCKER_MANIFEST_V2,
            Manifest::OciImage(_) => MediaType::OCI_IMAGE_MANIFEST,
            Manifest::OciIndex(_) => MediaType::OCI_IMAGE_INDEX,
        }
    }

    /// Serialize to JSON bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Manifest::DockerV2(m) => serde_json::to_vec(m),
            Manifest::OciImage(m) => serde_json::to_vec(m),
            Manifest::OciIndex(m) => serde_json::to_vec(m),
        }
    }

    /// Get all blob digests referenced by this manifest
    pub fn get_blob_digests(&self) -> Vec<String> {
        let mut digests = Vec::new();

        match self {
            Manifest::DockerV2(m) => {
                digests.push(m.config.digest.clone());
                for layer in &m.layers {
                    digests.push(layer.digest.clone());
                }
            }
            Manifest::OciImage(m) => {
                if let Some(config) = &m.config {
                    digests.push(config.digest.clone());
                }
                for layer in &m.layers {
                    digests.push(layer.digest.clone());
                }
            }
            Manifest::OciIndex(index) => {
                for manifest in &index.manifests {
                    digests.push(manifest.digest.clone());
                }
            }
        }

        digests
    }
}

/// Standard media types for Docker and OCI
pub struct MediaType;

impl MediaType {
    // Docker manifest types
    pub const DOCKER_MANIFEST_V2: &'static str =
        "application/vnd.docker.distribution.manifest.v2+json";
    pub const DOCKER_MANIFEST_V2_ALT: &'static str =
        "application/vnd.docker.distribution.manifest.v2+prettyjws";
    pub const DOCKER_MANIFEST_LIST: &'static str =
        "application/vnd.docker.distribution.manifest.list.v2+json";
    pub const DOCKER_CONTAINER_IMAGE_V1: &'static str =
        "application/vnd.docker.container.image.v1+json";
    pub const DOCKER_IMAGE_LAYER: &'static str =
        "application/vnd.docker.image.rootfs.diff.tar.gzip";
    pub const DOCKER_IMAGE_LAYER_FOREIGN: &'static str =
        "application/vnd.docker.image.rootfs.foreign.diff.tar.gzip";

    // OCI manifest types
    pub const OCI_IMAGE_MANIFEST: &'static str = "application/vnd.oci.image.manifest.v1+json";
    pub const OCI_IMAGE_INDEX: &'static str = "application/vnd.oci.image.index.v1+json";
    pub const OCI_IMAGE_CONFIG: &'static str = "application/vnd.oci.image.config.v1+json";
    pub const OCI_IMAGE_LAYER: &'static str = "application/vnd.oci.image.layer.v1.tar+gzip";
    pub const OCI_IMAGE_LAYER_NONDIST: &'static str =
        "application/vnd.oci.image.layer.nondistributable.v1.tar+gzip";
}

/// Blob upload session information
#[derive(Debug, Clone)]
pub struct BlobUploadSession {
    pub upload_id: String,
    pub repository: String,
    pub digest: Option<String>,
    pub uploaded_size: i64,
}
