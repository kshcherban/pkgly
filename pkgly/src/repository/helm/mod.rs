use super::prelude::*;
use ahash::HashMap;
use futures::future::BoxFuture;
use nr_core::{
    database::entities::repository::{DBRepository, DBRepositoryConfig},
    repository::config::RepositoryConfigType,
};
use nr_macros::DynRepositoryHandler;
use nr_storage::DynStorage;

pub mod chart;
pub mod configs;
pub mod hosted;
pub mod index;
pub mod oci;
pub mod types;

pub use configs::*;
pub use hosted::DeletePackageEntry;
pub use types::HelmChartVersionExtra;

use chart::ChartParseError;
use hosted::HelmHosted;

use super::{
    DynRepository, NewRepository, RepositoryAuthConfig, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};
use crate::{
    app::Pkgly,
    repository::{DynRepositoryHandlerError, RepositoryFactoryError, RepositoryHandlerError},
    utils::{IntoErrorResponse, ResponseBuilder},
};

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = HelmRepositoryError)]
pub enum HelmRepository {
    Hosted(HelmHosted),
}

#[derive(Debug, thiserror::Error)]
pub enum HelmRepositoryError {
    #[error("chart parsing failed: {0}")]
    ChartParse(String),
    #[error("chart '{name}' version '{version}' already exists")]
    ChartAlreadyExists { name: String, version: String },
    #[error("chart not found: {0}")]
    ChartNotFound(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("forbidden")]
    Forbidden,
    #[error("{0}")]
    Other(Box<dyn IntoErrorResponse>),
}

impl From<HelmRepositoryError> for RepositoryHandlerError {
    fn from(value: HelmRepositoryError) -> Self {
        RepositoryHandlerError::Other(Box::new(value))
    }
}

impl From<HelmRepositoryError> for DynRepositoryHandlerError {
    fn from(value: HelmRepositoryError) -> Self {
        DynRepositoryHandlerError(Box::new(value))
    }
}

impl From<ChartParseError> for HelmRepositoryError {
    fn from(value: ChartParseError) -> Self {
        HelmRepositoryError::ChartParse(value.to_string())
    }
}

impl From<crate::utils::bad_request::BadRequestErrors> for HelmRepositoryError {
    fn from(value: crate::utils::bad_request::BadRequestErrors) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<nr_storage::StorageError> for HelmRepositoryError {
    fn from(value: nr_storage::StorageError) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<sqlx::Error> for HelmRepositoryError {
    fn from(value: sqlx::Error) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<serde_json::Error> for HelmRepositoryError {
    fn from(value: serde_json::Error) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<index::IndexRenderError> for HelmRepositoryError {
    fn from(value: index::IndexRenderError) -> Self {
        HelmRepositoryError::InvalidRequest(value.to_string())
    }
}

impl From<RepositoryHandlerError> for HelmRepositoryError {
    fn from(value: RepositoryHandlerError) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<nr_core::database::DBError> for HelmRepositoryError {
    fn from(value: nr_core::database::DBError) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<RepositoryFactoryError> for HelmRepositoryError {
    fn from(value: RepositoryFactoryError) -> Self {
        HelmRepositoryError::InvalidRequest(value.to_string())
    }
}

impl From<crate::repository::docker::DockerError> for HelmRepositoryError {
    fn from(value: crate::repository::docker::DockerError) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<crate::app::authentication::AuthenticationError> for HelmRepositoryError {
    fn from(value: crate::app::authentication::AuthenticationError) -> Self {
        HelmRepositoryError::Other(Box::new(value))
    }
}

impl From<oci::HelmOciError> for HelmRepositoryError {
    fn from(value: oci::HelmOciError) -> Self {
        HelmRepositoryError::InvalidRequest(value.to_string())
    }
}

impl IntoErrorResponse for HelmRepositoryError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        match *self {
            HelmRepositoryError::ChartParse(message) => {
                ResponseBuilder::bad_request().body(message).into_response()
            }
            HelmRepositoryError::ChartAlreadyExists { name, version } => {
                ResponseBuilder::conflict()
                    .body(format!("Chart {name}@{version} already exists"))
                    .into_response()
            }
            HelmRepositoryError::ChartNotFound(name) => {
                ResponseBuilder::not_found().body(name).into_response()
            }
            HelmRepositoryError::InvalidRequest(message) => {
                ResponseBuilder::bad_request().body(message).into_response()
            }
            HelmRepositoryError::Forbidden => ResponseBuilder::forbidden()
                .body("forbidden")
                .into_response(),
            HelmRepositoryError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct HelmRepositoryType;

impl RepositoryType for HelmRepositoryType {
    fn get_type(&self) -> &'static str {
        "helm"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            HelmRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "helm",
            name: "Helm",
            description: "Helm chart repository supporting HTTP and OCI clients.",
            documentation_url: None,
            is_stable: false,
            required_configs: vec![HelmRepositoryConfigType::get_type_static()],
        }
    }

    fn create_new(
        &self,
        name: String,
        uuid: uuid::Uuid,
        mut configs: HashMap<String, serde_json::Value>,
        _storage: DynStorage,
    ) -> BoxFuture<'static, Result<NewRepository, RepositoryFactoryError>> {
        Box::pin(async move {
            let config_value = configs
                .get(HelmRepositoryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    HelmRepositoryConfigType::get_type_static(),
                ))?
                .clone();
            serde_json::from_value::<HelmRepositoryConfig>(config_value)
                .map_err(|err| RepositoryFactoryError::InvalidConfig("helm", err.to_string()))?;

            if configs
                .get(RepositoryAuthConfigType::get_type_static())
                .is_none()
            {
                let auth_default = RepositoryConfigType::default(&RepositoryAuthConfigType)
                    .map_err(|err| {
                        RepositoryFactoryError::InvalidConfig("helm", err.to_string())
                    })?;
                configs.insert(
                    RepositoryAuthConfigType::get_type_static().to_string(),
                    auth_default,
                );
            }

            Ok(NewRepository {
                name,
                uuid,
                repository_type: "helm".to_string(),
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
            let config = DBRepositoryConfig::<HelmRepositoryConfig>::get_config(
                repo.id,
                HelmRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            let auth_config = DBRepositoryConfig::<RepositoryAuthConfig>::get_config(
                repo.id,
                RepositoryAuthConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();

            let hosted = HelmHosted::load(website, storage, repo, config, auth_config).await?;

            Ok(DynRepository::Helm(HelmRepository::Hosted(hosted)))
        })
    }
}
