use std::{borrow::Cow, str::FromStr};

use ahash::HashMap;
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, header::ToStrError};
use nr_core::{
    database::entities::project::{NewProject, versions::NewVersion},
    repository::project::VersionData,
    storage::{StoragePath, StoragePathComponent},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{Display, EnumString};
use tracing::{debug, info};
use uuid::Uuid;

use super::NPMPackageName;
use crate::repository::npm::utils::extract_version_from_file;
use crate::repository::{maven::get_release_type, npm::NPMRegistryError};

#[derive(Debug, Display, EnumString)]
pub enum NPMCommand {
    #[strum(serialize = "publish")]
    Publish,
    #[strum(serialize = "deprecate", serialize = "undeprecate")]
    Deprecate,
}
impl TryFrom<&HeaderValue> for NPMCommand {
    type Error = InvalidNPMCommand;
    fn try_from(value: &HeaderValue) -> Result<Self, Self::Error> {
        let value = value.to_str()?;
        NPMCommand::from_str(value)
            .map_err(|_| InvalidNPMCommand::InvalidCommand(value.to_string()))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum InvalidNPMCommand {
    #[error("Invalid command {0}")]
    InvalidCommand(String),
    #[error("Unparsable command {0}")]
    UnparsableCommand(#[from] ToStrError),
    #[error("No header found")]
    NoHeaderFound,
}
impl IntoResponse for InvalidNPMCommand {
    fn into_response(self) -> Response {
        Response::builder()
            .status(http::StatusCode::BAD_REQUEST)
            .body(self.to_string().into())
            .unwrap_or_default()
    }
}
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct PublishVersion {
    pub name: NPMPackageName,
    pub version: String,
    pub dist: PublishDist,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
    #[serde(rename = "_id")]
    pub hidden_id: String,
    #[serde(default)]
    pub readme: String,
    #[serde(default, rename = "readmeFilename")]
    pub readme_file_name: String,
    #[serde(rename = "_nodeVersion")]
    pub secret_node_version: String,
    #[serde(rename = "_npmVersion")]
    pub hidden_npm_version: String,
}
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct PublishDist {
    pub integrity: String,
    pub shasum: String,
    pub tarball: String,
    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}
impl PublishDist {
    #[tracing::instrument(skip(self))]
    pub fn validate_tarball(
        &self,
        storage_name: &str,
        repository_name: &str,
    ) -> Result<StoragePath, NPMRegistryError> {
        let url = url::Url::from_str(&self.tarball).map_err(|error| {
            info!(?error, "Invalid tarball");
            NPMRegistryError::InvalidTarball {
                tarball_route: self.tarball.clone(),
                error: Cow::Owned(format!("Invalid URL: {}", error)),
            }
        })?;
        let mut path = url
            .path_segments()
            .ok_or(NPMRegistryError::InvalidTarball {
                tarball_route: self.tarball.clone(),
                error: Cow::Borrowed("No Path"),
            })?;
        if path.next() != Some("repositories") {
            info!(?url, "Invalid tarball (Missing Base Path for tarball)");
            return Err(NPMRegistryError::InvalidTarball {
                tarball_route: self.tarball.clone(),
                error: Cow::Borrowed("Missing base path"),
            });
        }
        if !path
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(storage_name))
        {
            info!(?url, "Invalid tarball (Missing storage name)");
            return Err(NPMRegistryError::InvalidTarball {
                tarball_route: self.tarball.clone(),
                error: Cow::Borrowed("Missing storage name"),
            });
        }
        if !path
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(repository_name))
        {
            info!(?url, "Invalid tarball (Missing repository name)");
            return Err(NPMRegistryError::InvalidTarball {
                tarball_route: self.tarball.clone(),
                error: Cow::Borrowed("Missing repository name"),
            });
        }
        let suffix = path.collect::<Vec<_>>().join("/");
        let decoded = percent_encoding::percent_decode_str(&suffix)
            .decode_utf8()
            .map_err(|_| NPMRegistryError::InvalidTarball {
                tarball_route: self.tarball.clone(),
                error: Cow::Borrowed("Invalid UTF-8 in tarball path"),
            })?;
        Ok(StoragePath::from(decoded.as_ref()))
    }
}
impl PublishVersion {
    pub fn new_project(
        &self,
        save_path: String,
        repository_id: Uuid,
    ) -> Result<NewProject, NPMRegistryError> {
        let project_key = self.name.to_string();
        let NPMPackageName { name, scope } = self.name.clone();
        Ok(NewProject {
            scope,
            project_key,
            name,
            storage_path: save_path,
            repository: repository_id,
            description: None,
        })
    }
    pub fn new_version(
        &self,
        project_id: Uuid,
        repository_id: Uuid,
        save_path: String,
        publisher: i32,
    ) -> Result<NewVersion, NPMRegistryError> {
        let release_type = get_release_type(&self.version);
        let extra = VersionData {
            extra: Some(serde_json::to_value(self)?),
            ..Default::default()
        };
        Ok(NewVersion {
            project_id,
            repository_id,
            version: self.version.clone(),
            release_type,
            version_path: save_path,
            publisher: Some(publisher),
            version_page: None,
            extra,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetPath {
    RegistryBase,
    Search,
    GetPackageInfo {
        name: String,
    },
    VersionInfo {
        name: String,
        version: String,
    },
    GetTar {
        name: String,
        version: String,
        file: String,
    },
}
impl GetPath {
    /// Path Types
    ///
    /// - `@{scope}/{package}` - Get package info
    /// - `@{scope}/{package}/{version}` - Get version info
    /// - `@{scope}/{package}/-/{scope}/{package}-{version}.tgz` - Get file
    pub fn scoped_package_call(
        components: Vec<StoragePathComponent>,
    ) -> Result<Self, NPMRegistryError> {
        let length = components.len();
        if length < 2 {
            return Err(NPMRegistryError::InvalidGetRequest);
        }
        let name = format!("{}/{}", components[0], components[1]);
        if length == 2 {
            return Ok(GetPath::GetPackageInfo { name });
        }
        if length == 3 {
            let version = components[2].to_string();
            debug!(?name, ?version, "Version info");
            return Ok(GetPath::VersionInfo { name, version });
        }
        if (length == 4 || length == 5)
            && components[2].as_ref() == "-"
            && (length == 4 || components[3] == components[0])
        {
            let file = components[length - 1].to_string();
            let version = extract_version_from_file(&file, &name)
                .ok_or(NPMRegistryError::InvalidGetRequest)?;
            return Ok(GetPath::GetTar {
                name,
                version,
                file,
            });
        }
        info!(?components, "Invalid path");
        Err(NPMRegistryError::InvalidGetRequest)
    }
    /// Path Types
    ///
    /// - `{package}` - Get package info
    /// - `{package}/{version}` - Get version info
    /// - `{package}/-/{package}-{version}.tgz` - Get file
    pub fn unscoped_package_call(
        components: Vec<StoragePathComponent>,
    ) -> Result<Self, NPMRegistryError> {
        let length = components.len();

        let name = components[0].to_string();
        if length == 1 {
            return Ok(GetPath::GetPackageInfo { name });
        }
        if length == 2 {
            let version = components[1].to_string();
            debug!(?name, ?version, "Version info");
            return Ok(GetPath::VersionInfo { name, version });
        }
        if length == 3 && components[1].as_ref() == "-" {
            let file = components[2].to_string();
            let version = extract_version_from_file(&file, &name)
                .ok_or(NPMRegistryError::InvalidGetRequest)?;
            return Ok(GetPath::GetTar {
                name,
                version,
                file,
            });
        }
        info!(?components, "Invalid path");
        Err(NPMRegistryError::InvalidGetRequest)
    }
}
impl TryFrom<StoragePath> for GetPath {
    type Error = NPMRegistryError;

    fn try_from(value: StoragePath) -> Result<Self, Self::Error> {
        let as_string = value.to_string();
        let components: Vec<_> = value.into();
        if components.is_empty() {
            return Ok(GetPath::RegistryBase);
        }
        if as_string.starts_with('@') {
            GetPath::scoped_package_call(components)
        } else {
            GetPath::unscoped_package_call(components)
        }
    }
}

#[cfg(test)]
pub mod tests;
