use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Metadata captured for Debian packages stored in `VersionData::extra`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq)]
pub struct DebPackageMetadata {
    /// Name of the distribution (suite) this package belongs to.
    pub distribution: String,
    /// Repository component (e.g. main, contrib).
    pub component: String,
    /// CPU architecture reported by the package.
    pub architecture: String,
    /// Relative filename within the repository.
    pub filename: String,
    /// Size of the package file in bytes.
    pub size: u64,
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub section: Option<String>,
    pub priority: Option<String>,
    pub maintainer: Option<String>,
    pub installed_size: Option<u64>,
    /// Dependency entries as provided by the package.
    #[serde(default)]
    pub depends: Vec<String>,
    pub homepage: Option<String>,
    /// Full description (summary + long form) stored verbatim.
    pub description: Option<String>,
}
