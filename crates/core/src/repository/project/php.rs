use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Metadata captured for Composer packages stored in `VersionData::extra`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq)]
pub struct PhpPackageMetadata {
    /// Original filename for the distribution archive.
    pub filename: String,
    /// Optional dist type reported by the client.
    pub dist_type: Option<String>,
    /// Optional checksum value.
    pub sha256: Option<String>,
    /// Optional homepage or repository URL.
    pub homepage: Option<String>,
    /// Extra composer.json content for downstream consumers.
    pub extra: Option<Value>,
}
