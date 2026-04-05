use std::fmt;

use ahash::HashMap;
use futures::future::BoxFuture;
use nr_core::{
    database::entities::repository::{DBRepository, DBRepositoryConfig},
    repository::config::RepositoryConfigType,
};
use nr_storage::DynStorage;
use tokio::task::JoinError;

pub use super::prelude::*;

use self::package::DebPackageError;
use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};
use crate::utils::{IntoErrorResponse, ResponseBuilder};

pub mod configs;
pub mod hosted;
mod metadata;
mod package;
pub mod proxy;
pub mod proxy_indexing;
pub mod proxy_refresh;
pub mod refresh_status;
pub mod scheduler;

pub use configs::*;
pub use hosted::DebHostedRepository;
pub use proxy::DebProxyRepository;
pub use proxy_indexing::{DatabaseDebProxyIndexer, DebProxyIndexing, DebProxyIndexingError};

#[derive(Debug, Clone, nr_macros::DynRepositoryHandler)]
#[repository_handler(error = DebRepositoryError)]
pub enum DebRepository {
    Hosted(DebHostedRepository),
    Proxy(DebProxyRepository),
}

#[derive(Debug, thiserror::Error)]
pub enum DebRepositoryError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    Other(Box<dyn IntoErrorResponse>),
}

impl IntoErrorResponse for DebRepositoryError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        match *self {
            DebRepositoryError::InvalidRequest(message) => {
                ResponseBuilder::bad_request().body(message)
            }
            DebRepositoryError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

impl From<sqlx::Error> for DebRepositoryError {
    fn from(value: sqlx::Error) -> Self {
        DebRepositoryError::Other(Box::new(value))
    }
}

impl From<nr_storage::StorageError> for DebRepositoryError {
    fn from(value: nr_storage::StorageError) -> Self {
        DebRepositoryError::Other(Box::new(value))
    }
}

impl From<crate::utils::bad_request::BadRequestErrors> for DebRepositoryError {
    fn from(value: crate::utils::bad_request::BadRequestErrors) -> Self {
        DebRepositoryError::Other(Box::new(value))
    }
}

impl From<crate::app::authentication::AuthenticationError> for DebRepositoryError {
    fn from(value: crate::app::authentication::AuthenticationError) -> Self {
        DebRepositoryError::Other(Box::new(value))
    }
}

impl From<serde_json::Error> for DebRepositoryError {
    fn from(value: serde_json::Error) -> Self {
        DebRepositoryError::Other(Box::new(value))
    }
}

impl From<super::RepositoryHandlerError> for DebRepositoryError {
    fn from(value: super::RepositoryHandlerError) -> Self {
        DebRepositoryError::Other(Box::new(value))
    }
}

impl From<multer::Error> for DebRepositoryError {
    fn from(value: multer::Error) -> Self {
        DebRepositoryError::InvalidRequest(value.to_string())
    }
}

impl From<DebPackageError> for DebRepositoryError {
    fn from(value: DebPackageError) -> Self {
        DebRepositoryError::InvalidRequest(value.to_string())
    }
}

impl From<std::io::Error> for DebRepositoryError {
    fn from(value: std::io::Error) -> Self {
        DebRepositoryError::Other(Box::new(value))
    }
}

impl From<JoinError> for DebRepositoryError {
    fn from(value: JoinError) -> Self {
        DebRepositoryError::Other(Box::new(BlockingTaskError(value)))
    }
}

#[derive(Debug)]
struct BlockingTaskError(JoinError);

impl fmt::Display for BlockingTaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Blocking task failed: {}", self.0)
    }
}

impl IntoErrorResponse for BlockingTaskError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        ResponseBuilder::internal_server_error().body(self.to_string())
    }
}

impl std::error::Error for BlockingTaskError {}

impl From<DebRepositoryError> for super::DynRepositoryHandlerError {
    fn from(value: DebRepositoryError) -> Self {
        super::DynRepositoryHandlerError(Box::new(value))
    }
}

#[derive(Debug, Default)]
pub struct DebRepositoryType;

impl RepositoryType for DebRepositoryType {
    fn get_type(&self) -> &'static str {
        "deb"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            DebRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "deb",
            name: "Debian",
            description: "Hosted Debian repository",
            documentation_url: Some("https://pkgly.kingtux.dev/repositoryTypes/deb/"),
            is_stable: false,
            required_configs: vec![DebRepositoryConfigType::get_type_static()],
        }
    }

    fn create_new(
        &self,
        name: String,
        uuid: uuid::Uuid,
        configs: HashMap<String, serde_json::Value>,
        storage: DynStorage,
    ) -> BoxFuture<'static, Result<NewRepository, super::RepositoryFactoryError>> {
        Box::pin(async move {
            let _ = storage;
            let config_value = configs
                .get(DebRepositoryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    DebRepositoryConfigType::get_type_static(),
                ))?
                .clone();
            serde_json::from_value::<DebRepositoryConfig>(config_value).map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    DebRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;
            Ok(NewRepository {
                name,
                uuid,
                repository_type: "deb".to_string(),
                configs,
            })
        })
    }

    fn load_repo(
        &self,
        repo: DBRepository,
        storage: DynStorage,
        website: crate::app::Pkgly,
    ) -> BoxFuture<'static, Result<DynRepository, super::RepositoryFactoryError>> {
        Box::pin(async move {
            let config = DBRepositoryConfig::<DebRepositoryConfig>::get_config(
                repo.id,
                DebRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .ok_or(RepositoryFactoryError::MissingConfig(
                DebRepositoryConfigType::get_type_static(),
            ))?;
            match config.value.0 {
                DebRepositoryConfig::Hosted(hosted_config) => {
                    let hosted =
                        DebHostedRepository::load(website, storage, repo, hosted_config).await?;
                    Ok(DynRepository::Deb(DebRepository::Hosted(hosted)))
                }
                DebRepositoryConfig::Proxy(proxy_config) => {
                    let proxy =
                        DebProxyRepository::load(website, storage, repo, proxy_config).await?;
                    Ok(DynRepository::Deb(DebRepository::Proxy(proxy)))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests;
