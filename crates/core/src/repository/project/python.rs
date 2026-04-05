use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Additional metadata captured for Python releases.
///
/// Stored under [`VersionData::extra`] when Python packages are published to Pkgly.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq)]
pub struct PythonPackageMetadata {
    /// Original filename that was uploaded (wheel, sdist, etc.).
    pub filename: String,
    /// Normalized project name following PEP 503.
    pub normalized_name: Option<String>,
    /// Optional `Requires-Python` declaration extracted from metadata.
    pub requires_python: Option<String>,
    /// Optional SHA256 checksum for convenience.
    pub sha256: Option<String>,
    /// Free-form field for additional metadata captured during publish.
    pub extra: Option<Value>,
}
