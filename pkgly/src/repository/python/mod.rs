use crate::error::OtherInternalError;
use ahash::HashMap;
use futures::future::BoxFuture;
use nr_core::{
    database::entities::repository::{DBRepository, DBRepositoryConfig},
    repository::config::RepositoryConfigType,
};
use nr_macros::DynRepositoryHandler;
use nr_storage::DynStorage;

pub use super::prelude::*;
mod configs;
pub use configs::*;
pub mod hosted;
pub mod proxy;
pub mod utils;
pub mod r#virtual;

use proxy::PythonProxy;

use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = PythonRepositoryError)]
pub enum PythonRepository {
    Hosted(hosted::PythonHosted),
    Proxy(PythonProxy),
    Virtual(r#virtual::PythonVirtualRepository),
}

#[derive(Debug, thiserror::Error)]
pub enum PythonRepositoryError {
    #[error("Invalid package path: {0}")]
    InvalidPath(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(&'static str),
    #[error("{0}")]
    Other(Box<dyn crate::utils::IntoErrorResponse>),
}
impl From<PythonRepositoryError> for super::RepositoryHandlerError {
    fn from(value: PythonRepositoryError) -> Self {
        super::RepositoryHandlerError::Other(Box::new(value))
    }
}

#[cfg(test)]
mod tests;

impl From<PythonRepositoryError> for super::DynRepositoryHandlerError {
    fn from(value: PythonRepositoryError) -> Self {
        super::DynRepositoryHandlerError(Box::new(value))
    }
}
macro_rules! impl_from_other {
    ($from:ty) => {
        impl From<$from> for PythonRepositoryError {
            fn from(value: $from) -> Self {
                PythonRepositoryError::Other(Box::new(value))
            }
        }
    };
}
impl_from_other!(crate::repository::RepositoryHandlerError);
impl_from_other!(crate::utils::bad_request::BadRequestErrors);
impl_from_other!(nr_storage::StorageError);
impl_from_other!(sqlx::Error);
impl_from_other!(serde_json::Error);
impl_from_other!(crate::app::authentication::AuthenticationError);

impl From<crate::repository::proxy_indexing::ProxyIndexingError> for PythonRepositoryError {
    fn from(value: crate::repository::proxy_indexing::ProxyIndexingError) -> Self {
        PythonRepositoryError::Other(Box::new(OtherInternalError::new(value)))
    }
}

impl crate::utils::IntoErrorResponse for PythonRepositoryError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        use axum::response::IntoResponse;
        use http::StatusCode;
        match *self {
            PythonRepositoryError::InvalidPath(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(message)
                    .into_response()
            }
            PythonRepositoryError::Unsupported(message) => crate::utils::ResponseBuilder::default()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body(message),
            PythonRepositoryError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

#[derive(Debug, Default)]
pub struct PythonRepositoryType;

impl RepositoryType for PythonRepositoryType {
    fn get_type(&self) -> &'static str {
        "python"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            PythonRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "python",
            name: "Python",
            description: "A Python package repository compatible with Pkgly clients.",
            documentation_url: Some("https://pkgly.kingtux.dev/repositoryTypes/python/"),
            is_stable: false,
            required_configs: vec![PythonRepositoryConfigType::get_type_static()],
        }
    }

    fn create_new(
        &self,
        name: String,
        uuid: uuid::Uuid,
        configs: HashMap<String, serde_json::Value>,
        _storage: DynStorage,
    ) -> BoxFuture<'static, Result<NewRepository, RepositoryFactoryError>> {
        Box::pin(async move {
            let config = configs
                .get(PythonRepositoryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    PythonRepositoryConfigType::get_type_static(),
                ))?
                .clone();
            let config: PythonRepositoryConfig = serde_json::from_value(config)
                .map_err(|err| RepositoryFactoryError::InvalidConfig("python", err.to_string()))?;
            if let PythonRepositoryConfig::Virtual(config) = &config {
                validate_virtual_config(config)?;
            }
            Ok(NewRepository {
                name,
                uuid,
                repository_type: "python".to_string(),
                configs,
            })
        })
    }

    fn load_repo(
        &self,
        repo: DBRepository,
        storage: DynStorage,
        website: Pkgly,
    ) -> BoxFuture<'static, Result<DynRepository, RepositoryFactoryError>> {
        Box::pin(async move {
            let config = DBRepositoryConfig::<PythonRepositoryConfig>::get_config(
                repo.id,
                PythonRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            match config {
                PythonRepositoryConfig::Hosted => {
                    let hosted = hosted::PythonHosted::load(
                        website,
                        storage,
                        repo,
                        PythonRepositoryConfig::Hosted,
                    )
                    .await?;
                    Ok(DynRepository::Python(PythonRepository::Hosted(hosted)))
                }
                PythonRepositoryConfig::Proxy(proxy_config) => {
                    let proxy =
                        proxy::PythonProxy::load(website, storage, repo, proxy_config).await?;
                    Ok(DynRepository::Python(PythonRepository::Proxy(proxy)))
                }
                PythonRepositoryConfig::Virtual(virtual_config) => {
                    let virtual_repo = r#virtual::PythonVirtualRepository::load(
                        website,
                        storage,
                        repo,
                        virtual_config,
                    )
                    .await?;
                    Ok(DynRepository::Python(PythonRepository::Virtual(
                        virtual_repo,
                    )))
                }
            }
        })
    }
}

pub(crate) fn validate_virtual_config(
    config: &crate::repository::r#virtual::config::VirtualRepositoryConfig,
) -> Result<(), RepositoryFactoryError> {
    crate::repository::r#virtual::config::validate_virtual_repository_config(config).map_err(
        |err| {
            RepositoryFactoryError::InvalidConfig(
                PythonRepositoryConfigType::get_type_static(),
                err.to_string(),
            )
        },
    )
}
