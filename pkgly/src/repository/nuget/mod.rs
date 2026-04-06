use ahash::HashMap;
use futures::future::BoxFuture;
use nr_core::{
    database::entities::repository::{DBRepository, DBRepositoryConfig},
    repository::{Visibility, config::RepositoryConfigType, project::ProjectResolution},
    storage::StoragePath,
};
use nr_macros::DynRepositoryHandler;
use nr_storage::DynStorage;
use thiserror::Error;
use uuid::Uuid;

pub mod configs;
mod hosted;
mod proxy;
mod utils;
mod r#virtual;

pub use configs::*;
pub use hosted::NugetHosted;
pub use proxy::NugetProxy;
pub use r#virtual::NugetVirtualRepository;
pub use utils::REPOSITORY_TYPE_ID;

use crate::error::OtherInternalError;
use crate::{
    app::Pkgly,
    repository::{
        DynRepository, DynRepositoryHandlerError, NewRepository, RepoResponse, Repository,
        RepositoryAuthConfigType, RepositoryFactoryError, RepositoryHandlerError,
        RepositoryRequest, RepositoryType, RepositoryTypeDescription,
    },
};

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = NugetError)]
pub enum NugetRepository {
    Hosted(NugetHosted),
    Proxy(NugetProxy),
    Virtual(NugetVirtualRepository),
}

#[derive(Debug, Error)]
pub enum NugetError {
    #[error("Invalid NuGet package: {0}")]
    InvalidPackage(String),
    #[error("Unsupported NuGet path: {0}")]
    UnsupportedPath(String),
    #[error("{0}")]
    Other(Box<dyn crate::utils::IntoErrorResponse>),
}

impl crate::utils::IntoErrorResponse for NugetError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        use axum::response::IntoResponse;
        match *self {
            NugetError::InvalidPackage(message) | NugetError::UnsupportedPath(message) => {
                crate::utils::ResponseBuilder::bad_request()
                    .body(message)
                    .into_response()
            }
            NugetError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

impl From<NugetError> for RepositoryHandlerError {
    fn from(value: NugetError) -> Self {
        RepositoryHandlerError::Other(Box::new(value))
    }
}

impl From<NugetError> for DynRepositoryHandlerError {
    fn from(value: NugetError) -> Self {
        DynRepositoryHandlerError(Box::new(value))
    }
}

macro_rules! impl_from_other {
    ($from:ty) => {
        impl From<$from> for NugetError {
            fn from(value: $from) -> Self {
                NugetError::Other(Box::new(value))
            }
        }
    };
}

impl_from_other!(crate::repository::RepositoryHandlerError);
impl_from_other!(crate::utils::bad_request::BadRequestErrors);
impl_from_other!(nr_storage::StorageError);
impl_from_other!(sqlx::Error);
impl_from_other!(serde_json::Error);
impl_from_other!(std::io::Error);
impl_from_other!(crate::app::authentication::AuthenticationError);
impl From<maven_rs::quick_xml::DeError> for NugetError {
    fn from(value: maven_rs::quick_xml::DeError) -> Self {
        NugetError::InvalidPackage(value.to_string())
    }
}
impl From<multer::Error> for NugetError {
    fn from(value: multer::Error) -> Self {
        NugetError::InvalidPackage(value.to_string())
    }
}
impl From<zip::result::ZipError> for NugetError {
    fn from(value: zip::result::ZipError) -> Self {
        NugetError::InvalidPackage(value.to_string())
    }
}
impl From<reqwest::Error> for NugetError {
    fn from(value: reqwest::Error) -> Self {
        NugetError::Other(Box::new(OtherInternalError::new(value)))
    }
}
impl From<std::string::FromUtf8Error> for NugetError {
    fn from(value: std::string::FromUtf8Error) -> Self {
        NugetError::Other(Box::new(OtherInternalError::new(value)))
    }
}
impl From<url::ParseError> for NugetError {
    fn from(value: url::ParseError) -> Self {
        NugetError::Other(Box::new(OtherInternalError::new(value)))
    }
}
impl From<axum::Error> for NugetError {
    fn from(value: axum::Error) -> Self {
        NugetError::Other(Box::new(OtherInternalError::new(value)))
    }
}

#[derive(Debug, Default)]
pub struct NugetRepositoryType;

impl RepositoryType for NugetRepositoryType {
    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            NugetRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: REPOSITORY_TYPE_ID,
            name: "NuGet",
            description: "A NuGet repository compatible with .NET and NuGet V3 clients.",
            documentation_url: Some("https://pkgly.kingtux.dev/repositoryTypes/nuget/"),
            is_stable: false,
            required_configs: vec![NugetRepositoryConfigType::get_type_static()],
        }
    }

    fn create_new(
        &self,
        name: String,
        uuid: Uuid,
        configs: HashMap<String, serde_json::Value>,
        _storage: DynStorage,
    ) -> BoxFuture<'static, Result<NewRepository, RepositoryFactoryError>> {
        Box::pin(async move {
            let config = configs
                .get(NugetRepositoryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    NugetRepositoryConfigType::get_type_static(),
                ))?
                .clone();
            let parsed: NugetRepositoryConfig = serde_json::from_value(config).map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    NugetRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;
            if let NugetRepositoryConfig::Virtual(config) = &parsed {
                crate::repository::r#virtual::config::validate_virtual_repository_config(config)
                    .map_err(|err| {
                        RepositoryFactoryError::InvalidConfig(
                            NugetRepositoryConfigType::get_type_static(),
                            err.to_string(),
                        )
                    })?;
            }
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
        website: Pkgly,
    ) -> BoxFuture<'static, Result<DynRepository, RepositoryFactoryError>> {
        Box::pin(async move {
            let config = DBRepositoryConfig::<NugetRepositoryConfig>::get_config(
                repo.id,
                NugetRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            match config {
                NugetRepositoryConfig::Hosted => {
                    let hosted = hosted::NugetHosted::load(website, storage, repo).await?;
                    Ok(DynRepository::Nuget(NugetRepository::Hosted(hosted)))
                }
                NugetRepositoryConfig::Proxy(proxy_config) => {
                    let proxy = proxy::NugetProxy::load(website, storage, repo, proxy_config).await?;
                    Ok(DynRepository::Nuget(NugetRepository::Proxy(proxy)))
                }
                NugetRepositoryConfig::Virtual(virtual_config) => {
                    let virtual_repo =
                        r#virtual::NugetVirtualRepository::load(website, storage, repo, virtual_config).await?;
                    Ok(DynRepository::Nuget(NugetRepository::Virtual(virtual_repo)))
                }
            }
        })
    }
}

impl NugetRepository {
    pub async fn resolve_project_and_version(
        &self,
        path: &StoragePath,
    ) -> Result<ProjectResolution, NugetError> {
        match self {
            NugetRepository::Hosted(repo) => repo.resolve_project(path).await,
            NugetRepository::Proxy(_) | NugetRepository::Virtual(_) => {
                Ok(ProjectResolution::default())
            }
        }
    }
}

#[cfg(test)]
mod tests;
