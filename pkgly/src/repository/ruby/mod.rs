//! RubyGems repository support (Hosted + Proxy).

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
pub mod compact_index;
pub mod full_index;
pub mod gem;
pub mod hosted;
pub mod proxy;
pub mod utils;

use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};

pub static REPOSITORY_TYPE_ID: &str = "ruby";

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = RubyRepositoryError)]
pub enum RubyRepository {
    Hosted(hosted::RubyHosted),
    Proxy(proxy::RubyProxy),
}

#[derive(Debug, thiserror::Error)]
pub enum RubyRepositoryError {
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(&'static str),
    #[error("{0}")]
    Other(Box<dyn crate::utils::IntoErrorResponse>),
}

impl From<RubyRepositoryError> for super::RepositoryHandlerError {
    fn from(value: RubyRepositoryError) -> Self {
        super::RepositoryHandlerError::Other(Box::new(value))
    }
}

impl From<RubyRepositoryError> for super::DynRepositoryHandlerError {
    fn from(value: RubyRepositoryError) -> Self {
        super::DynRepositoryHandlerError(Box::new(value))
    }
}

macro_rules! impl_from_other {
    ($from:ty) => {
        impl From<$from> for RubyRepositoryError {
            fn from(value: $from) -> Self {
                RubyRepositoryError::Other(Box::new(value))
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

impl From<crate::repository::proxy_indexing::ProxyIndexingError> for RubyRepositoryError {
    fn from(value: crate::repository::proxy_indexing::ProxyIndexingError) -> Self {
        RubyRepositoryError::Other(Box::new(OtherInternalError::new(value)))
    }
}

impl crate::utils::IntoErrorResponse for RubyRepositoryError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        use axum::response::IntoResponse;
        use http::StatusCode;

        match *self {
            RubyRepositoryError::InvalidPath(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(message)
                    .into_response()
            }
            RubyRepositoryError::InvalidRequest(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(message)
                    .into_response()
            }
            RubyRepositoryError::Unsupported(message) => crate::utils::ResponseBuilder::default()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body(message),
            RubyRepositoryError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

#[derive(Debug, Default)]
pub struct RubyRepositoryType;

impl RepositoryType for RubyRepositoryType {
    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            RubyRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: REPOSITORY_TYPE_ID,
            name: "RubyGems",
            description: "A RubyGems repository compatible with Bundler and the `gem` CLI.",
            documentation_url: None,
            is_stable: false,
            required_configs: vec![RubyRepositoryConfigType::get_type_static()],
        }
    }

    fn create_new(
        &self,
        name: String,
        uuid: uuid::Uuid,
        configs: HashMap<String, serde_json::Value>,
        _storage: DynStorage,
    ) -> BoxFuture<'static, Result<NewRepository, super::RepositoryFactoryError>> {
        Box::pin(async move {
            let config_key = RubyRepositoryConfigType::get_type_static();
            let config_value = configs
                .get(config_key)
                .ok_or(super::RepositoryFactoryError::MissingConfig(config_key))?
                .clone();

            serde_json::from_value::<RubyRepositoryConfig>(config_value).map_err(|err| {
                super::RepositoryFactoryError::InvalidConfig(config_key, err.to_string())
            })?;

            Ok(NewRepository {
                name,
                uuid,
                repository_type: REPOSITORY_TYPE_ID.to_string(),
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
            let Some(config) = DBRepositoryConfig::<RubyRepositoryConfig>::get_config(
                repo.id,
                RubyRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            else {
                return Err(super::RepositoryFactoryError::MissingConfig(
                    RubyRepositoryConfigType::get_type_static(),
                ));
            };

            match config.value.0 {
                RubyRepositoryConfig::Hosted => {
                    let hosted = hosted::RubyHosted::load(website, storage, repo).await?;
                    Ok(DynRepository::Ruby(RubyRepository::Hosted(hosted)))
                }
                RubyRepositoryConfig::Proxy(proxy_config) => {
                    let proxy =
                        proxy::RubyProxy::load(website, storage, repo, proxy_config).await?;
                    Ok(DynRepository::Ruby(RubyRepository::Proxy(proxy)))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests;
