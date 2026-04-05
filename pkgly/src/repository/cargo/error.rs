use axum::response::{IntoResponse, Response};
use semver::Version;

use crate::{
    repository::{DynRepositoryHandlerError, RepositoryHandlerError},
    utils::{IntoErrorResponse, ResponseBuilder},
};

use super::utils::CargoUtilError;

#[derive(Debug, thiserror::Error)]
pub enum CargoRepositoryError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Crate {crate_name} version {version} already exists")]
    VersionExists {
        crate_name: String,
        version: Version,
    },
    #[error("Unauthorized")]
    Unauthorized,
    #[error("{0}")]
    Other(Box<dyn IntoErrorResponse>),
}

impl From<CargoUtilError> for CargoRepositoryError {
    fn from(value: CargoUtilError) -> Self {
        CargoRepositoryError::InvalidRequest(value.to_string())
    }
}

macro_rules! impl_from_other {
    ($ty:ty) => {
        impl From<$ty> for CargoRepositoryError {
            fn from(value: $ty) -> Self {
                CargoRepositoryError::Other(Box::new(value))
            }
        }
    };
}

impl_from_other!(sqlx::Error);
impl_from_other!(serde_json::Error);
impl_from_other!(nr_storage::StorageError);
impl_from_other!(crate::app::authentication::AuthenticationError);
impl_from_other!(crate::repository::RepositoryHandlerError);
impl_from_other!(crate::error::OtherInternalError);

impl IntoErrorResponse for CargoRepositoryError {
    fn into_response_boxed(self: Box<Self>) -> Response {
        self.into_response()
    }
}

impl IntoResponse for CargoRepositoryError {
    fn into_response(self) -> Response {
        match self {
            CargoRepositoryError::InvalidRequest(message) => {
                ResponseBuilder::bad_request().body(message)
            }
            CargoRepositoryError::VersionExists {
                crate_name,
                version,
            } => ResponseBuilder::conflict().body(format!(
                "Crate {crate_name} version {version} already exists"
            )),
            CargoRepositoryError::Unauthorized => {
                ResponseBuilder::unauthorized().body("Missing permission to access repository")
            }
            CargoRepositoryError::Other(inner) => inner.into_response_boxed(),
        }
    }
}

impl From<CargoRepositoryError> for RepositoryHandlerError {
    fn from(value: CargoRepositoryError) -> Self {
        RepositoryHandlerError::Other(Box::new(value))
    }
}

impl From<CargoRepositoryError> for DynRepositoryHandlerError {
    fn from(value: CargoRepositoryError) -> Self {
        DynRepositoryHandlerError(Box::new(value))
    }
}
