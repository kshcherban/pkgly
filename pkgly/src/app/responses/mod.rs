use std::fmt::Debug;

use axum::response::{IntoResponse, Response};
use derive_more::derive::From;
use nr_core::repository::config::RepositoryConfigError;
use nr_storage::StorageError;
use tracing::instrument;

use super::RepositoryStorageName;
use crate::utils::ResponseBuilder;
#[derive(Debug, From)]
pub enum RepositoryNotFound {
    RepositoryAndNameLookup(RepositoryStorageName),
    Uuid(uuid::Uuid),
}
impl IntoResponse for RepositoryNotFound {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::RepositoryAndNameLookup(lookup) => ResponseBuilder::not_found().body(format!(
                "Repository {}/{} not found",
                lookup.storage_name, lookup.repository_name
            )),
            Self::Uuid(uuid) => {
                ResponseBuilder::not_found().body(format!("Repository not found: {:?}", uuid))
            }
        }
    }
}

#[derive(Debug)]
pub enum MissingPermission {
    UserManager,
    RepositoryManager,
    EditRepository(uuid::Uuid),
    ReadRepository(uuid::Uuid),
    StorageManager,
}
impl IntoResponse for MissingPermission {
    #[inline(always)]
    #[instrument(name = "MissingPermission::into_response", skip(self))]
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::UserManager => {
                ResponseBuilder::forbidden().body("You are not a user manager or admin")
            }
            Self::RepositoryManager => {
                ResponseBuilder::forbidden().body("You are not a repository manager or admin")
            }
            Self::EditRepository(id) => ResponseBuilder::forbidden().body(format!(
                "You do not have permission to edit repository: {}",
                id
            )),
            Self::ReadRepository(id) => ResponseBuilder::forbidden().body(format!(
                "You do not have permission to read repository: {}",
                id
            )),
            Self::StorageManager => {
                ResponseBuilder::forbidden().body("You are not a storage manager or admin")
            }
        }
    }
}
#[derive(Debug, From)]
pub struct InvalidStorageType(pub String);
impl IntoResponse for InvalidStorageType {
    fn into_response(self) -> Response {
        ResponseBuilder::bad_request().body(format!("Invalid Storage Type: {}", self.0))
    }
}
#[derive(Debug, From)]
pub struct InvalidStorageConfig(pub StorageError);

impl IntoResponse for InvalidStorageConfig {
    fn into_response(self) -> Response {
        ResponseBuilder::bad_request().body(format!("Invalid Storage Config: {}", self.0))
    }
}

#[derive(Debug, From)]
pub enum InvalidRepositoryConfig {
    InvalidConfigType(String),
    RepositoryTypeDoesntSupportConfig {
        repository_type: String,
        config_key: String,
    },
    InvalidConfig {
        config_key: String,
        error: RepositoryConfigError,
    },
}
impl IntoResponse for InvalidRepositoryConfig {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidConfigType(t) => ResponseBuilder::bad_request()
                .body(format!("Invalid Repository Config Type: {}", t)),
            Self::RepositoryTypeDoesntSupportConfig {
                repository_type,
                config_key,
            } => ResponseBuilder::bad_request().body(format!(
                "Repository Type {} does not support config key {}",
                repository_type, config_key
            )),
            Self::InvalidConfig { config_key, error } => ResponseBuilder::bad_request()
                .body(format!("Invalid Config for key {}: {}", config_key, error)),
        }
    }
}
