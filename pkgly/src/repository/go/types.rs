use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Represents a Go module path following Go module naming conventions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GoModulePath(String);

impl GoModulePath {
    /// Create a new Go module path, validating it follows Go conventions
    pub fn new(path: impl Into<String>) -> Result<Self, GoModuleError> {
        let path = path.into();
        Self::validate(&path)?;
        Ok(GoModulePath(path))
    }

    /// Get the module path as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the module path as a string slice
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Get the module name (last component of the path)
    pub fn module_name(&self) -> &str {
        self.0.split('/').last().unwrap_or(&self.0)
    }

    /// Get the domain prefix (first component of the path)
    pub fn domain(&self) -> &str {
        self.0.split('/').next().unwrap_or(&self.0)
    }

    /// Check if this is a standard library module
    pub fn is_stdlib(&self) -> bool {
        self.0.starts_with("std") || self.0.starts_with("cmd")
    }

    /// Validate a Go module path according to Go naming conventions
    fn validate(path: &str) -> Result<(), GoModuleError> {
        if path.is_empty() {
            return Err(GoModuleError::EmptyPath);
        }

        // Basic validation according to Go module path rules
        // https://golang.org/ref/mod#module-path

        // Check for invalid characters
        if path.chars().any(|c| !Self::is_valid_path_char(c)) {
            return Err(GoModuleError::InvalidCharacters(path.to_string()));
        }

        // Check that it doesn't start or end with '/'
        if path.starts_with('/') || path.ends_with('/') {
            return Err(GoModuleError::InvalidPathFormat(path.to_string()));
        }

        // Check for consecutive slashes
        if path.contains("//") {
            return Err(GoModuleError::InvalidPathFormat(path.to_string()));
        }

        // Check that the domain is valid if it looks like a domain
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() > 1 {
            let domain = parts[0];
            if domain.contains('.') && !Self::is_valid_domain(domain) {
                return Err(GoModuleError::InvalidDomain(domain.to_string()));
            }
        }

        Ok(())
    }

    /// Check if a character is valid in a Go module path
    fn is_valid_path_char(c: char) -> bool {
        matches!(c,
            'a'..='z' | 'A'..='Z' | '0'..='9' |
            '-' | '_' | '.' | '/' | '~'
        )
    }

    /// Check if a domain is valid according to DNS rules
    fn is_valid_domain(domain: &str) -> bool {
        // Basic domain validation
        if domain.is_empty() || domain.len() > 253 {
            return false;
        }

        // Check for valid characters and structure
        for label in domain.split('.') {
            if label.is_empty() || label.len() > 63 {
                return false;
            }

            // Check if label starts and ends with alphanumeric
            if !label.chars().next().unwrap_or(' ').is_alphanumeric()
                || !label.chars().last().unwrap_or(' ').is_alphanumeric()
            {
                return false;
            }

            // Check for valid characters in label
            if label
                .chars()
                .any(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-'))
            {
                return false;
            }
        }

        true
    }
}

impl FromStr for GoModulePath {
    type Err = GoModuleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl std::fmt::Display for GoModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Represents a Go module version following semantic versioning
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GoVersion(String);

impl GoVersion {
    /// Create a new Go version, validating it follows semantic versioning
    pub fn new(version: impl Into<String>) -> Result<Self, GoModuleError> {
        let version = version.into();
        Self::validate(&version)?;
        Ok(GoVersion(version))
    }

    /// Get the version as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a pre-release version
    pub fn is_prerelease(&self) -> bool {
        self.0.contains('-')
    }

    /// Check if this is a pseudo-version (like v1.0.0-20210101123456-abcdefabcdef)
    pub fn is_pseudo_version(&self) -> bool {
        self.0.contains('-') && self.0.matches('-').count() >= 2
    }

    /// Get the major version
    pub fn major(&self) -> Result<u64, GoModuleError> {
        self.parse_semver().map(|(major, _, _)| major)
    }

    /// Get the minor version
    pub fn minor(&self) -> Result<u64, GoModuleError> {
        self.parse_semver().map(|(_, minor, _)| minor)
    }

    /// Get the patch version
    pub fn patch(&self) -> Result<u64, GoModuleError> {
        self.parse_semver().map(|(_, _, patch)| patch)
    }

    /// Parse semantic version components
    fn parse_semver(&self) -> Result<(u64, u64, u64), GoModuleError> {
        // Handle pseudo-versions by extracting the base version
        let base_version = if self.is_pseudo_version() {
            self.0
                .split('-')
                .next()
                .unwrap_or(&self.0)
                .trim_start_matches('v')
        } else {
            self.0.trim_start_matches('v')
        };

        let parts: Vec<&str> = base_version.split('.').collect();
        if parts.len() != 3 {
            return Err(GoModuleError::InvalidVersion(self.0.clone()));
        }

        let major = parts[0]
            .parse::<u64>()
            .map_err(|_| GoModuleError::InvalidVersion(self.0.clone()))?;
        let minor = parts[1]
            .parse::<u64>()
            .map_err(|_| GoModuleError::InvalidVersion(self.0.clone()))?;
        let patch = parts[2]
            .parse::<u64>()
            .map_err(|_| GoModuleError::InvalidVersion(self.0.clone()))?;

        Ok((major, minor, patch))
    }

    /// Validate a Go version string
    fn validate(version: &str) -> Result<(), GoModuleError> {
        if version.is_empty() {
            return Err(GoModuleError::EmptyVersion);
        }

        // Check for 'v' prefix (optional but common)
        let version = version.trim_start_matches('v');

        // Basic semantic version validation
        if version.contains(' ') {
            return Err(GoModuleError::InvalidVersion(version.to_string()));
        }

        // Try to parse as semantic version
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() < 2 {
            return Err(GoModuleError::InvalidVersion(version.to_string()));
        }

        // Check if major version is numeric
        if parts[0].parse::<u64>().is_err() {
            return Err(GoModuleError::InvalidVersion(version.to_string()));
        }

        Ok(())
    }
}

impl FromStr for GoVersion {
    type Err = GoModuleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl std::fmt::Display for GoVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Go module-related errors
#[derive(Debug, Error, Clone)]
pub enum GoModuleError {
    #[error("Empty module path")]
    EmptyPath,
    #[error("Empty version")]
    EmptyVersion,
    #[error("Invalid characters in module path: {0}")]
    InvalidCharacters(String),
    #[error("Invalid path format: {0}")]
    InvalidPathFormat(String),
    #[error("Invalid domain: {0}")]
    InvalidDomain(String),
    #[error("Invalid version: {0}")]
    InvalidVersion(String),
    #[error("Invalid module path: {0}")]
    InvalidModulePath(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

impl crate::utils::IntoErrorResponse for GoModuleError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        use axum::response::IntoResponse;
        match *self {
            GoModuleError::EmptyPath | GoModuleError::EmptyVersion => {
                crate::utils::ResponseBuilder::bad_request()
                    .body("Empty module path or version")
                    .into_response()
            }
            GoModuleError::InvalidCharacters(message)
            | GoModuleError::InvalidPathFormat(message)
            | GoModuleError::InvalidDomain(message)
            | GoModuleError::InvalidVersion(message)
            | GoModuleError::InvalidModulePath(message)
            | GoModuleError::InvalidRequest(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(message)
                    .into_response()
            }
        }
    }
}

/// Represents information about a Go module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoModuleInfo {
    pub module_path: GoModulePath,
    pub version: GoVersion,
    pub time: chrono::DateTime<chrono::Utc>,
}

/// Represents a go.mod file content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoModFile {
    pub module: String,
    pub go_version: Option<String>,
    pub require: Vec<GoDependency>,
    pub replace: Vec<GoReplace>,
    pub exclude: Vec<GoExclude>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoDependency {
    pub path: String,
    pub version: String,
    pub indirect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoReplace {
    pub old: GoDependencySpec,
    pub new: GoDependencySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoDependencySpec {
    pub path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoExclude {
    pub path: String,
    pub version: String,
}

#[cfg(test)]
mod tests;
