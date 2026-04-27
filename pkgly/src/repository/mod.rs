pub mod prelude {
    pub use axum::response::{IntoResponse, Response};
    pub use http::StatusCode;
    pub use nr_core::{
        repository::{project::*, *},
        storage::*,
    };

    pub use super::{
        DynRepositoryHandlerError, RepoResponse, Repository, RepositoryFactoryError,
        RepositoryHandlerError, RepositoryRequest,
    };
    pub use crate::app::Pkgly;
}
use nr_core::{
    repository::{Visibility, project::ProjectResolution},
    storage::StoragePath,
};
use nr_macros::DynRepositoryHandler;
mod base;
pub use base::Repository;
mod staging;
pub use staging::*;
mod repo_http;
pub use repo_http::*;
mod auth_config;
pub mod cargo;
pub mod commands;
pub mod deb;
pub mod docker;
pub mod go;
pub mod helm;
pub mod hosted;
pub mod maven;
pub mod npm;
pub mod nuget;
pub mod php;
pub mod proxy;
pub mod python;
pub mod ruby;
pub mod r#virtual;
pub use auth_config::*;
pub mod proxy_indexing;
pub mod retention;
pub use proxy_indexing::{DatabaseProxyIndexer, ProxyIndexing, ProxyIndexingError};
mod repo_type;
pub use repo_type::*;

mod error;
pub mod utils;
pub use error::*;

use crate::{
    app::{Pkgly, authentication::AuthenticationError},
    utils::IntoErrorResponse,
};
#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = DynRepositoryHandlerError)]
pub enum DynRepository {
    Deb(deb::DebRepository),
    Docker(docker::DockerRegistry),
    Go(go::GoRepository),
    Helm(helm::HelmRepository),
    Cargo(cargo::CargoRegistry),
    Maven(maven::MavenRepository),
    Nuget(nuget::NugetRepository),
    NPM(npm::NPMRegistry),
    Python(python::PythonRepository),
    Php(php::PhpRepository),
    Ruby(ruby::RubyRepository),
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use nr_storage::{
        DynStorage, StaticStorageFactory, StorageConfig, StorageConfigInner, StorageTypeConfig,
        local::{LocalConfig, LocalStorageFactory},
    };
    use uuid::Uuid;

    pub async fn test_storage() -> DynStorage {
        let path = std::env::temp_dir().join(format!("pkgly_test_storage_{}", Uuid::new_v4()));
        if let Err(error) = std::fs::create_dir_all(&path) {
            panic!(
                "Failed to create test storage directory {:?}: {}",
                path, error
            );
        }
        let mut storage_config = StorageConfigInner::test_config();
        storage_config.storage_type = "Local".into();
        let config = StorageConfig {
            storage_config,
            type_config: StorageTypeConfig::Local(LocalConfig { path }),
        };
        let local =
            <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(config)
                .await
                .expect("create local storage for tests");
        DynStorage::Local(local)
    }
}

#[cfg(test)]
mod base_tests;
