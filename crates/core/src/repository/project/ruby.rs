use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// RubyGems metadata captured for a published gem.
///
/// Stored under [`VersionData::extra`] when Ruby gems are published to Pkgly.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq)]
pub struct RubyPackageMetadata {
    /// Canonical `.gem` filename stored by Pkgly.
    pub filename: String,
    /// Optional platform for the gem (omitted for the default `ruby` platform).
    pub platform: Option<String>,
    /// Optional SHA256 checksum (hex) of the `.gem` file.
    pub sha256: Option<String>,
    /// Runtime dependencies for the gem.
    #[serde(default)]
    pub dependencies: Vec<RubyDependencyMetadata>,
    /// Optional Ruby version requirement string (Compact Index format).
    pub required_ruby: Option<String>,
    /// Optional RubyGems version requirement string (Compact Index format).
    pub required_rubygems: Option<String>,
}

/// Dependency metadata stored in [`RubyPackageMetadata`].
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq)]
pub struct RubyDependencyMetadata {
    /// Dependency gem name.
    pub name: String,
    /// Version constraints for the dependency.
    ///
    /// Each string should be formatted like `>= 1.2.3`, `~> 2.0`, etc.
    #[serde(default)]
    pub requirements: Vec<String>,
}
