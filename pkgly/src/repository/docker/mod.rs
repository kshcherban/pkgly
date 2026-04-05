//! Docker Registry V2 and OCI Image Format Implementation
//!
//! This module implements the Docker Registry HTTP API V2 specification
//! and supports OCI Image Format manifests.
//!
//! References:
//! - Docker Registry API V2: https://docs.docker.com/registry/spec/api/
//! - OCI Distribution Spec: https://github.com/opencontainers/distribution-spec
//! - OCI Image Spec: https://github.com/opencontainers/image-spec

use ahash::HashMap;
use futures::future::BoxFuture;
use hosted::DockerHosted;
use nr_core::{
    database::{
        DBError,
        entities::repository::{DBRepository, DBRepositoryConfig},
    },
    repository::config::RepositoryConfigType,
};
use nr_macros::DynRepositoryHandler;
use nr_storage::DynStorage;
use proxy::DockerProxy;

pub mod auth;
pub mod configs;
pub mod handlers;
pub mod hosted;
pub mod metadata;
pub mod proxy;
pub mod types;

pub use super::prelude::*;
pub use configs::*;

use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};
use crate::{
    app::authentication::AuthenticationError,
    repository::proxy_indexing::ProxyIndexingError,
    utils::{IntoErrorResponse, ResponseBuilder, bad_request::BadRequestErrors},
};

pub static REPOSITORY_TYPE_ID: &str = "docker";

/// Main Docker Registry enum supporting different registry types
#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = DockerError)]
pub enum DockerRegistry {
    Hosted(DockerHosted),
    Proxy(DockerProxy),
}

impl DockerRegistry {
    pub async fn load(
        repo: DBRepository,
        storage: DynStorage,
        website: Pkgly,
    ) -> Result<Self, RepositoryFactoryError> {
        let Some(docker_config_db) = DBRepositoryConfig::<DockerRegistryConfig>::get_config(
            repo.id,
            DockerRegistryConfigType::get_type_static(),
            &website.database,
        )
        .await?
        else {
            return Err(RepositoryFactoryError::MissingConfig(
                DockerRegistryConfigType::get_type_static(),
            ));
        };

        let docker_config = docker_config_db.value.0;
        match docker_config {
            DockerRegistryConfig::Hosted => {
                let hosted = DockerHosted::load(repo, storage, website).await?;
                Ok(DockerRegistry::Hosted(hosted))
            }
            DockerRegistryConfig::Proxy(proxy_config) => {
                let proxy = DockerProxy::load(repo, storage, website, proxy_config).await?;
                Ok(DockerRegistry::Proxy(proxy))
            }
        }
    }
}

/// Docker registry error types
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("Manifest not found: {0}")]
    ManifestNotFound(String),

    #[error("Blob not found: {0}")]
    BlobNotFound(String),

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("Invalid tag name: {0}")]
    InvalidTag(String),

    #[error("Invalid repository name: {0}")]
    InvalidRepositoryName(String),

    #[error("Unsupported manifest type: {0}")]
    UnsupportedManifestType(String),

    #[error("Digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error("Blob upload not found: {0}")]
    BlobUploadNotFound(String),

    #[error("Tag already exists and overwrite is not allowed: {0}")]
    TagOverwriteNotAllowed(String),

    #[error("{0}")]
    Other(Box<dyn IntoErrorResponse>),
}

// Implement error conversions
macro_rules! impl_from_error_for_other {
    ($t:ty) => {
        impl From<$t> for DockerError {
            fn from(e: $t) -> Self {
                DockerError::Other(Box::new(e))
            }
        }
    };
}

impl_from_error_for_other!(BadRequestErrors);
impl_from_error_for_other!(DBError);
impl_from_error_for_other!(sqlx::Error);
impl_from_error_for_other!(serde_json::Error);
impl_from_error_for_other!(std::io::Error);
impl_from_error_for_other!(reqwest::Error);
impl_from_error_for_other!(AuthenticationError);
impl_from_error_for_other!(RepositoryHandlerError);
impl_from_error_for_other!(nr_storage::StorageError);
impl_from_error_for_other!(ProxyIndexingError);

impl From<url::ParseError> for DockerError {
    fn from(err: url::ParseError) -> Self {
        DockerError::InvalidManifest(err.to_string())
    }
}

impl From<DockerError> for RepositoryHandlerError {
    fn from(err: DockerError) -> Self {
        RepositoryHandlerError::Other(Box::new(err))
    }
}

impl From<DockerError> for DynRepositoryHandlerError {
    fn from(err: DockerError) -> Self {
        DynRepositoryHandlerError(Box::new(err))
    }
}

impl IntoErrorResponse for DockerError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        self.into_response()
    }
}

fn docker_error_payload(code: &str, message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "errors": [{
            "code": code,
            "message": message.into(),
        }]
    })
}

fn docker_error_response(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    ResponseBuilder::default()
        .status(status)
        .header("Content-Type", "application/json")
        .json(&docker_error_payload(code, message))
}

impl IntoResponse for DockerError {
    fn into_response(self) -> Response {
        use http::StatusCode;

        match self {
            DockerError::ManifestNotFound(ref msg) => {
                docker_error_response(StatusCode::NOT_FOUND, "MANIFEST_UNKNOWN", msg)
            }
            DockerError::BlobNotFound(ref msg) => {
                docker_error_response(StatusCode::NOT_FOUND, "BLOB_UNKNOWN", msg)
            }
            DockerError::InvalidManifest(ref msg) | DockerError::InvalidTag(ref msg) => {
                docker_error_response(StatusCode::BAD_REQUEST, "MANIFEST_INVALID", msg)
            }
            DockerError::DigestMismatch { expected, actual } => docker_error_response(
                StatusCode::BAD_REQUEST,
                "DIGEST_INVALID",
                format!("Digest mismatch: expected {}, got {}", expected, actual),
            ),
            DockerError::TagOverwriteNotAllowed(ref tag) => docker_error_response(
                StatusCode::CONFLICT,
                "TAG_INVALID",
                format!("Tag {} already exists and overwrite is not allowed", tag),
            ),
            DockerError::Other(other) => other.into_response_boxed(),
            err => docker_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "UNKNOWN",
                err.to_string(),
            ),
        }
    }
}

/// Docker repository type implementation
#[derive(Debug, Default)]
pub struct DockerRepositoryType;

impl RepositoryType for DockerRepositoryType {
    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            DockerRegistryConfigType::get_type_static(),
            DockerPushRulesConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "docker",
            name: "Docker Registry",
            description: "Docker Registry V2 and OCI container image repository",
            documentation_url: Some("https://pkgly.kingtux.dev/repositoryTypes/docker/"),
            is_stable: true,
            required_configs: vec![DockerRegistryConfigType::get_type_static()],
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
            let sub_type = configs
                .get(DockerRegistryConfigType::get_type_static())
                .ok_or(RepositoryFactoryError::MissingConfig(
                    DockerRegistryConfigType::get_type_static(),
                ))?
                .clone();

            let _docker_config: DockerRegistryConfig = match serde_json::from_value(sub_type) {
                Ok(ok) => ok,
                Err(err) => {
                    return Err(RepositoryFactoryError::InvalidConfig(
                        DockerRegistryConfigType::get_type_static(),
                        err.to_string(),
                    ));
                }
            };

            Ok(NewRepository {
                name,
                uuid,
                repository_type: "docker".to_string(),
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
            DockerRegistry::load(repo, storage, website)
                .await
                .map(DynRepository::Docker)
        })
    }
}

#[cfg(test)]
mod tests;
