//! NPM Registry Implementation
//!
//! Documentation for NPM: https://github.com/npm/registry/blob/main/docs/REGISTRY-API.md
//!

use std::borrow::Cow;

use ahash::HashMap;
use base64::DecodeError;
use config::RepositoryConfigType;
use futures::future::BoxFuture;
use hosted::NPMHostedRegistry;
use nr_core::database::entities::repository::{DBRepository, DBRepositoryConfig};
use nr_macros::DynRepositoryHandler;
use nr_storage::DynStorage;
use tracing::debug;
use types::InvalidNPMPackageName;

pub mod hosted;
pub mod login;
pub mod proxy;
pub mod types;
pub mod utils;
pub mod r#virtual;
pub use super::prelude::*;
use crate::{
    app::authentication::AuthenticationError,
    error::OtherInternalError,
    utils::{IntoErrorResponse, bad_request::BadRequestErrors},
};
pub use r#virtual as npm_virtual;
mod configs;
pub use configs::*;

use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};
use npm_virtual::NpmVirtualRepository;
use proxy::NpmProxyRegistry;

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error=NPMRegistryError)]
pub enum NPMRegistry {
    Hosted(hosted::NPMHostedRegistry),
    Proxy(NpmProxyRegistry),
    Virtual(npm_virtual::NpmVirtualRepository),
}

#[derive(Debug, thiserror::Error)]
pub enum NPMRegistryError {
    #[error(transparent)]
    InvalidName(#[from] InvalidNPMPackageName),
    #[error(
        "Invalid tarball. The tarballs location is invalid.
        This means you used `$BASE_URL/repositories/$STORAGE/$REPO` without a trailing slash.
        tarbar Route: {tarball_route} Error: {error}"
    )]
    InvalidTarball {
        tarball_route: String,
        error: Cow<'static, str>,
    },
    #[error(
        "Invalid GET request. The requested route is invalid to the NPM Registry. This could be a bug. AS the code is very sketchy"
    )]
    InvalidGetRequest,
    #[error("Version not found")]
    VersionNotFound,
    #[error("Invalid Package Attachment. Error: {0}")]
    InvalidPackageAttachment(DecodeError),
    #[error("Only one release or attachment can be uploaded at a time")]
    OnlyOneReleaseOrAttachmentAtATime,
    #[error("Upstream proxy {url} returned status {status}")]
    ProxyUpstream { url: String, status: StatusCode },
    #[error("Failed to fetch from proxy {url}: {error}")]
    ProxyFetch { url: String, error: String },
    #[error("{0}")]
    Other(Box<dyn IntoErrorResponse>),
}
impl From<NPMRegistryError> for RepositoryHandlerError {
    fn from(err: NPMRegistryError) -> Self {
        RepositoryHandlerError::Other(Box::new(err))
    }
}

#[cfg(test)]
mod tests;

macro_rules! impl_from_error_for_other {
    ($t:ty) => {
        impl From<$t> for NPMRegistryError {
            fn from(e: $t) -> Self {
                NPMRegistryError::Other(Box::new(e))
            }
        }
    };
}
impl_from_error_for_other!(BadRequestErrors);
impl_from_error_for_other!(sqlx::Error);
impl_from_error_for_other!(serde_json::Error);
impl_from_error_for_other!(std::io::Error);
impl_from_error_for_other!(AuthenticationError);
impl_from_error_for_other!(RepositoryHandlerError);
impl_from_error_for_other!(nr_storage::StorageError);
impl From<crate::repository::proxy_indexing::ProxyIndexingError> for NPMRegistryError {
    fn from(value: crate::repository::proxy_indexing::ProxyIndexingError) -> Self {
        NPMRegistryError::Other(Box::new(OtherInternalError::new(value)))
    }
}

impl IntoErrorResponse for NPMRegistryError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        self.into_response()
    }
}

impl From<NPMRegistryError> for DynRepositoryHandlerError {
    fn from(err: NPMRegistryError) -> Self {
        DynRepositoryHandlerError(Box::new(err))
    }
}

pub(crate) fn validate_virtual_config(
    config: &npm_virtual::NpmVirtualConfig,
) -> Result<(), RepositoryFactoryError> {
    crate::repository::r#virtual::config::validate_virtual_repository_config(config).map_err(
        |err| {
            RepositoryFactoryError::InvalidConfig(
                NPMRegistryConfigType::get_type_static(),
                err.to_string(),
            )
        },
    )
}

impl IntoResponse for NPMRegistryError {
    fn into_response(self) -> Response {
        match self {
            NPMRegistryError::InvalidGetRequest => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Invalid GET request".into())
                .unwrap_or_default(),
            NPMRegistryError::ProxyUpstream { url, status } => Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(format!("Proxy upstream {url} returned status {status}").into())
                .unwrap_or_default(),
            NPMRegistryError::ProxyFetch { url, error } => Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(format!("Failed to fetch from proxy {url}: {error}").into())
                .unwrap_or_default(),
            NPMRegistryError::Other(other) => other.into_response_boxed(),
            bad_request => {
                debug!("Bad Request: {:?}", bad_request);
                Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(bad_request.to_string().into())
                    .unwrap_or_default()
            }
        }
    }
}
#[derive(Debug, Default)]
pub struct NpmRegistryType;

impl RepositoryType for NpmRegistryType {
    fn get_type(&self) -> &'static str {
        "npm"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            NPMRegistryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "npm",
            name: "NPM",
            description: "A NPM Registry",
            documentation_url: Some("https://pkgly.kingtux.dev/repositoryTypes/npm/"),
            is_stable: true,
            required_configs: vec![NPMRegistryConfigType::get_type_static()],
        }
    }

    fn create_new(
        &self,
        name: String,
        uuid: uuid::Uuid,
        configs: HashMap<String, serde_json::Value>,
        _storage: nr_storage::DynStorage,
    ) -> BoxFuture<'static, Result<NewRepository, RepositoryFactoryError>> {
        Box::pin(async move {
            let sub_type = configs
                .get(NPMRegistryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    NPMRegistryConfigType::get_type_static(),
                ))?
                .clone();
            let registry_config: NPMRegistryConfig = match serde_json::from_value(sub_type) {
                Ok(ok) => ok,
                Err(err) => {
                    return Err(RepositoryFactoryError::InvalidConfig(
                        NPMRegistryConfigType::get_type_static(),
                        err.to_string(),
                    ));
                }
            };
            if let NPMRegistryConfig::Virtual(config) = &registry_config {
                validate_virtual_config(config)?;
            }
            Ok(NewRepository {
                name,
                uuid,
                repository_type: "npm".to_string(),
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
            let config = DBRepositoryConfig::<NPMRegistryConfig>::get_config(
                repo.id,
                NPMRegistryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            match config {
                NPMRegistryConfig::Hosted => {
                    let hosted = NPMHostedRegistry::load(website, storage, repo).await?;
                    Ok(NPMRegistry::Hosted(hosted).into())
                }
                NPMRegistryConfig::Proxy(proxy_config) => {
                    let proxy =
                        NpmProxyRegistry::load(website, storage, repo, proxy_config).await?;
                    Ok(NPMRegistry::Proxy(proxy).into())
                }
                NPMRegistryConfig::Virtual(virtual_config) => {
                    let virtual_repo =
                        NpmVirtualRepository::load(website, storage, repo, virtual_config).await?;
                    Ok(NPMRegistry::Virtual(virtual_repo).into())
                }
            }
        })
    }
}
