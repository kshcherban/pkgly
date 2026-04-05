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
mod hosted;
pub mod proxy;
mod types;
pub mod utils;
pub use types::*;
pub mod ext;

use crate::error::OtherInternalError;
use hosted::GoHosted;
use proxy::GoProxy;

use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = GoRepositoryError)]
pub enum GoRepository {
    Hosted(GoHosted),
    Proxy(GoProxy),
}

#[derive(Debug, thiserror::Error)]
pub enum GoRepositoryError {
    #[error("Invalid Go module path: {0}")]
    InvalidModulePath(String),
    #[error("Invalid Go version: {0}")]
    InvalidVersion(String),
    #[error("Module not found: {0}")]
    ModuleNotFound(String),
    #[error("Version not found: {0}")]
    VersionNotFound(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(&'static str),
    #[error("Invalid request format: {0}")]
    InvalidRequest(String),
    #[error("Go module error: {0}")]
    GoModuleError(#[from] GoModuleError),
    #[error("{0}")]
    Other(Box<dyn crate::utils::IntoErrorResponse>),
}

impl From<GoRepositoryError> for super::RepositoryHandlerError {
    fn from(value: GoRepositoryError) -> Self {
        super::RepositoryHandlerError::Other(Box::new(value))
    }
}

#[cfg(test)]
mod tests;

impl From<GoRepositoryError> for super::DynRepositoryHandlerError {
    fn from(value: GoRepositoryError) -> Self {
        super::DynRepositoryHandlerError(Box::new(value))
    }
}

macro_rules! impl_from_other {
    ($from:ty) => {
        impl From<$from> for GoRepositoryError {
            fn from(value: $from) -> Self {
                GoRepositoryError::Other(Box::new(value))
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
impl From<crate::repository::proxy_indexing::ProxyIndexingError> for GoRepositoryError {
    fn from(value: crate::repository::proxy_indexing::ProxyIndexingError) -> Self {
        GoRepositoryError::Other(Box::new(OtherInternalError::new(value)))
    }
}

// Also add direct conversion from GoModuleError to RepositoryHandlerError
impl From<types::GoModuleError> for crate::repository::RepositoryHandlerError {
    fn from(value: types::GoModuleError) -> Self {
        crate::repository::RepositoryHandlerError::Other(Box::new(
            crate::utils::bad_request::BadRequestErrors::Other(format!(
                "Go module error: {}",
                value
            )),
        ))
    }
}

impl crate::utils::IntoErrorResponse for GoRepositoryError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        use axum::response::IntoResponse;
        use http::StatusCode;
        match *self {
            GoRepositoryError::InvalidModulePath(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(format!("Invalid module path: {}", message))
                    .into_response()
            }
            GoRepositoryError::InvalidVersion(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(format!("Invalid version: {}", message))
                    .into_response()
            }
            GoRepositoryError::ModuleNotFound(message) => crate::utils::ResponseBuilder::default()
                .status(StatusCode::NOT_FOUND)
                .body(format!("Module not found: {}", message)),
            GoRepositoryError::VersionNotFound(message) => crate::utils::ResponseBuilder::default()
                .status(StatusCode::NOT_FOUND)
                .body(format!("Version not found: {}", message)),
            GoRepositoryError::Unsupported(message) => crate::utils::ResponseBuilder::default()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body(message),
            GoRepositoryError::InvalidRequest(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(format!("Invalid request: {}", message))
                    .into_response()
            }
            GoRepositoryError::GoModuleError(err) => crate::utils::ResponseBuilder::bad_request()
                .body(format!("Go module error: {}", err))
                .into_response(),
            GoRepositoryError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

#[derive(Debug, Default)]
pub struct GoRepositoryType;

impl RepositoryType for GoRepositoryType {
    fn get_type(&self) -> &'static str {
        "go"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            GoRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "go",
            name: "Go",
            description: "A Go module repository compatible with Go proxy protocol and GOPROXY.",
            documentation_url: Some("https://pkgly.kingtux.dev/repositoryTypes/go/"),
            is_stable: false,
            required_configs: vec![GoRepositoryConfigType::get_type_static()],
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
                .get(GoRepositoryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    GoRepositoryConfigType::get_type_static(),
                ))?
                .clone();
            let _: GoRepositoryConfig = serde_json::from_value(config)
                .map_err(|err| RepositoryFactoryError::InvalidConfig("go", err.to_string()))?;
            Ok(NewRepository {
                name,
                uuid,
                repository_type: "go".to_string(),
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
            let config = DBRepositoryConfig::<GoRepositoryConfig>::get_config(
                repo.id,
                GoRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            match config {
                GoRepositoryConfig::Hosted => {
                    let hosted =
                        hosted::GoHosted::load(website, storage, repo, GoRepositoryConfig::Hosted)
                            .await?;
                    Ok(DynRepository::Go(GoRepository::Hosted(hosted)))
                }
                GoRepositoryConfig::Proxy(proxy_config) => {
                    let proxy = proxy::GoProxy::load(website, storage, repo, proxy_config).await?;
                    Ok(DynRepository::Go(GoRepository::Proxy(proxy)))
                }
            }
        })
    }
}
