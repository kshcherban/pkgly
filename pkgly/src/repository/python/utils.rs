use nr_core::{repository::project::ReleaseType, storage::StoragePath};
use serde::{Deserialize, Serialize};

use super::PythonRepositoryError;
use crate::repository::RepositoryHandlerError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonPackagePathInfo {
    pub package: String,
    pub version: String,
    pub file_name: String,
}

impl PythonPackagePathInfo {
    pub fn project_key(&self) -> String {
        normalize_package_name(&self.package)
    }

    pub fn release_type(&self) -> ReleaseType {
        ReleaseType::release_type_from_version(&self.version)
    }

    pub fn project_storage_path(&self) -> String {
        format!("{}/", self.project_key())
    }

    pub fn version_storage_path(&self) -> String {
        format!("{}/{}", self.project_key(), self.version)
    }
}

impl TryFrom<&StoragePath> for PythonPackagePathInfo {
    type Error = PythonRepositoryError;

    fn try_from(path: &StoragePath) -> Result<Self, Self::Error> {
        let components: Vec<String> = path
            .clone()
            .into_iter()
            .map(|component| component.to_string())
            .collect();
        if components.len() < 3 {
            return Err(PythonRepositoryError::InvalidPath(path.to_string()));
        }
        let package = components
            .first()
            .cloned()
            .ok_or(RepositoryHandlerError::NotFound)?;
        let version = components
            .get(1)
            .cloned()
            .ok_or(RepositoryHandlerError::NotFound)?;
        let file_name = components
            .last()
            .cloned()
            .ok_or(RepositoryHandlerError::NotFound)?;
        Ok(Self {
            package,
            version,
            file_name,
        })
    }
}

pub fn normalize_package_name(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .map(|char| match char {
            '-' | '_' | '.' => '-',
            _ => char,
        })
        .collect()
}

pub(crate) fn html_escape(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '\"' => "&quot;".to_string(),
            '\'' => "&#x27;".to_string(),
            _ => ch.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
