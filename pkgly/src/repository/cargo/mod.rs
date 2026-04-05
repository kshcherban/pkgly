//! Cargo registry implementation (Hosted mode with sparse index support).
//!
//! NOTE: Implementation is added in subsequent steps. This module currently
//! exists to host the initial TDD-focused test scaffolding.

pub mod configs;
mod error;
mod hosted;
pub mod utils;

use ahash::HashMap;
use futures::future::BoxFuture;
use nr_core::database::entities::repository::{DBRepository, DBRepositoryConfig};
use nr_core::repository::config::RepositoryConfigType;
use nr_macros::DynRepositoryHandler;
use nr_storage::DynStorage;

pub use super::prelude::*;
use super::{
    DynRepository, NewRepository, RepositoryAuthConfigType, RepositoryType,
    RepositoryTypeDescription,
};
pub use configs::{CargoRepositoryConfig, CargoRepositoryConfigType};
pub use error::CargoRepositoryError;
pub use hosted::CargoHosted;

#[derive(Debug, Clone, DynRepositoryHandler)]
#[repository_handler(error = CargoRepositoryError)]
pub enum CargoRegistry {
    Hosted(CargoHosted),
}

#[derive(Debug, Default)]
pub struct CargoRepositoryType;

impl RepositoryType for CargoRepositoryType {
    fn get_type(&self) -> &'static str {
        "cargo"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            configs::CargoRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn get_description(&self) -> RepositoryTypeDescription {
        RepositoryTypeDescription {
            type_name: "cargo",
            name: "Cargo",
            description: "A Cargo crate registry with sparse index support.",
            documentation_url: Some("https://doc.rust-lang.org/cargo/reference/registries.html"),
            is_stable: false,
            required_configs: vec![configs::CargoRepositoryConfigType::get_type_static()],
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
            let config_key = configs::CargoRepositoryConfigType::get_type_static();
            let config_value = configs
                .get(config_key)
                .ok_or(RepositoryFactoryError::MissingConfig(config_key))?
                .clone();
            let parsed: configs::CargoRepositoryConfig = serde_json::from_value(config_value)
                .map_err(|err| {
                    RepositoryFactoryError::InvalidConfig(config_key, err.to_string())
                })?;
            match parsed {
                configs::CargoRepositoryConfig::Hosted => {}
            }
            Ok(NewRepository {
                name,
                uuid,
                repository_type: "cargo".to_string(),
                configs,
            })
        })
    }

    fn load_repo(
        &self,
        repo: DBRepository,
        storage: DynStorage,
        website: crate::app::Pkgly,
    ) -> BoxFuture<'static, Result<DynRepository, RepositoryFactoryError>> {
        Box::pin(async move {
            let config = DBRepositoryConfig::<configs::CargoRepositoryConfig>::get_config(
                repo.id,
                configs::CargoRepositoryConfigType::get_type_static(),
                &website.database,
            )
            .await?
            .map(|cfg| cfg.value.0)
            .unwrap_or_default();
            let hosted = CargoHosted::load(website, storage, repo, config).await?;
            Ok(CargoRegistry::Hosted(hosted).into())
        })
    }
}

#[cfg(test)]
mod tests;
