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
mod composer;
pub(crate) use composer::*;
pub mod hosted;
pub mod proxy;
pub mod utils;

use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = PhpRepositoryError)]
pub enum PhpRepository {
    Hosted(hosted::PhpHosted),
    Proxy(proxy::PhpProxy),
}

#[derive(Debug, thiserror::Error)]
pub enum PhpRepositoryError {
    #[error("Invalid package path: {0}")]
    InvalidPath(String),
    #[error("Invalid composer package: {0}")]
    InvalidComposer(String),
    #[error("{0}")]
    Other(Box<dyn crate::utils::IntoErrorResponse>),
}

impl crate::utils::IntoErrorResponse for PhpRepositoryError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        match *self {
            PhpRepositoryError::InvalidPath(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .json(&serde_json::json!({ "error": message }))
            }
            PhpRepositoryError::InvalidComposer(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .json(&serde_json::json!({ "error": message }))
            }
            PhpRepositoryError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

macro_rules! impl_from_error {
    ($err:ty) => {
        impl From<$err> for PhpRepositoryError {
            fn from(value: $err) -> Self {
                PhpRepositoryError::Other(Box::new(value))
            }
        }
    };
}
impl_from_error!(crate::repository::RepositoryHandlerError);
impl_from_error!(crate::utils::bad_request::BadRequestErrors);
impl_from_error!(nr_storage::StorageError);
impl_from_error!(sqlx::Error);
impl_from_error!(serde_json::Error);
impl_from_error!(crate::app::authentication::AuthenticationError);
impl_from_error!(crate::repository::proxy_indexing::ProxyIndexingError);
impl_from_error!(reqwest::Error);

impl From<PhpRepositoryError> for super::RepositoryHandlerError {
    fn from(value: PhpRepositoryError) -> Self {
        super::RepositoryHandlerError::Other(Box::new(value))
    }
}

impl From<PhpRepositoryError> for super::DynRepositoryHandlerError {
    fn from(value: PhpRepositoryError) -> Self {
        super::DynRepositoryHandlerError(Box::new(value))
    }
}

#[derive(Debug, Default)]
pub struct PhpRepositoryType;

impl RepositoryType for PhpRepositoryType {
    fn get_type(&self) -> &'static str {
        "php"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            PhpRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "php",
            name: "PHP Composer",
            description: "A Composer/Packagist compatible repository for Pkgly.",
            documentation_url: Some("https://pkgly.kingtux.dev/repositoryTypes/php/"),
            is_stable: false,
            required_configs: vec![PhpRepositoryConfigType::get_type_static()],
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
                .get(PhpRepositoryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    PhpRepositoryConfigType::get_type_static(),
                ))?
                .clone();
            serde_json::from_value::<PhpRepositoryConfig>(config).map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    PhpRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;
            Ok(NewRepository {
                name,
                uuid,
                repository_type: "php".to_string(),
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
            let Some(config) = DBRepositoryConfig::<PhpRepositoryConfig>::get_config(
                repo.id,
                PhpRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            else {
                return Err(RepositoryFactoryError::MissingConfig(
                    PhpRepositoryConfigType::get_type_static(),
                ));
            };
            match config.value.0 {
                PhpRepositoryConfig::Hosted => {
                    let hosted = hosted::PhpHosted::load(website, storage, repo).await?;
                    Ok(DynRepository::Php(PhpRepository::Hosted(hosted)))
                }
                PhpRepositoryConfig::Proxy(proxy_config) => {
                    let proxy = proxy::PhpProxy::load(website, storage, repo, proxy_config).await?;
                    Ok(DynRepository::Php(PhpRepository::Proxy(proxy)))
                }
            }
        })
    }
}

#[cfg(test)]
mod composer_tests;
#[cfg(test)]
mod proxy_tests;
#[cfg(test)]
mod tests;
