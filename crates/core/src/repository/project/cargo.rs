use ahash::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Metadata captured for Cargo crates stored in `VersionData::extra`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq)]
pub struct CargoPackageMetadata {
    /// SHA256 checksum published to the index.
    pub checksum: String,
    /// Size in bytes of the `.crate` archive.
    pub crate_size: u64,
    /// Whether the crate version is yanked.
    #[serde(default)]
    pub yanked: bool,
    /// Map of cargo features.
    #[serde(default)]
    pub features: HashMap<String, Vec<String>>,
    /// Dependencies included in the publish metadata.
    #[serde(default)]
    pub dependencies: Vec<CargoDependencyMetadata>,
    /// Additional metadata captured during publish.
    #[serde(default)]
    pub extra: Option<Value>,
}

/// Simplified dependency metadata stored alongside Cargo packages.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq)]
pub struct CargoDependencyMetadata {
    pub name: String,
    #[serde(default)]
    pub req: Option<String>,
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
