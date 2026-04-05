use chrono::Utc;
use nr_core::{ConfigTimeStamp, database::entities::storage::DBStorage};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{StorageError, local::LocalConfig, s3::S3Config};
#[derive(Debug, Clone, Error)]
#[error("Expected Config Type: {0}, Got: {1}")]
pub struct InvalidConfigType(&'static str, &'static str);
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StorageConfigInner {
    pub storage_name: String,
    pub storage_id: Uuid,
    pub storage_type: String,
    #[schema(value_type =  chrono::DateTime<chrono::FixedOffset>, format = DateTime)]
    pub created_at: ConfigTimeStamp,
}
impl StorageConfigInner {
    pub fn test_config() -> Self {
        StorageConfigInner {
            storage_name: "test".into(),
            storage_id: Uuid::new_v4(),
            storage_type: "test".into(),
            created_at: ConfigTimeStamp::from(Utc::now()),
        }
    }
}
pub trait StorageTypeConfigTrait: Into<StorageTypeConfig> {
    fn from_type_config(dyn_config: StorageTypeConfig) -> Result<Self, InvalidConfigType>
    where
        Self: Sized;

    fn type_name(&self) -> &'static str;
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StorageConfig {
    #[serde(flatten)]
    pub storage_config: StorageConfigInner,
    pub type_config: StorageTypeConfig,
}
impl TryFrom<DBStorage> for StorageConfig {
    type Error = StorageError;
    fn try_from(db_storage: DBStorage) -> Result<Self, Self::Error> {
        let type_config = serde_json::from_value(db_storage.config.0)?;
        Ok(StorageConfig {
            storage_config: StorageConfigInner {
                storage_name: db_storage.name.into(),
                storage_id: db_storage.id,
                storage_type: db_storage.storage_type,
                created_at: db_storage.created_at,
            },
            type_config,
        })
    }
}
impl<'a> From<BorrowedStorageConfig<'a>> for StorageConfig {
    fn from(borrowed: BorrowedStorageConfig<'a>) -> Self {
        StorageConfig {
            storage_config: borrowed.storage_config.clone(),
            type_config: borrowed.config.into(),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct BorrowedStorageConfig<'a> {
    #[serde(flatten)]
    pub storage_config: &'a StorageConfigInner,
    pub config: BorrowedStorageTypeConfig<'a>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "settings")]
pub enum StorageTypeConfig {
    Local(LocalConfig),
    S3(Box<S3Config>),
}

impl StorageTypeConfig {
    pub fn type_name(&self) -> &'static str {
        match self {
            StorageTypeConfig::Local(_) => "Local",
            StorageTypeConfig::S3(_) => "S3",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", content = "settings")]
pub enum BorrowedStorageTypeConfig<'a> {
    Local(&'a LocalConfig),
    S3(&'a S3Config),
}

impl<'a> From<BorrowedStorageTypeConfig<'a>> for StorageTypeConfig {
    fn from(borrowed: BorrowedStorageTypeConfig<'a>) -> Self {
        match borrowed {
            BorrowedStorageTypeConfig::Local(config) => StorageTypeConfig::Local(config.clone()),
            BorrowedStorageTypeConfig::S3(config) => {
                StorageTypeConfig::S3(Box::new(config.clone()))
            }
        }
    }
}

impl From<LocalConfig> for StorageTypeConfig {
    fn from(config: LocalConfig) -> Self {
        StorageTypeConfig::Local(config)
    }
}

impl From<S3Config> for StorageTypeConfig {
    fn from(config: S3Config) -> Self {
        StorageTypeConfig::S3(Box::new(config))
    }
}

impl StorageTypeConfigTrait for LocalConfig {
    fn from_type_config(dyn_config: StorageTypeConfig) -> Result<Self, InvalidConfigType>
    where
        Self: Sized,
    {
        match dyn_config {
            StorageTypeConfig::Local(config) => Ok(config),
            _ => Err(InvalidConfigType("LocalConfig", dyn_config.type_name())),
        }
    }

    fn type_name(&self) -> &'static str {
        "LocalConfig"
    }
}

impl StorageTypeConfigTrait for S3Config {
    fn from_type_config(dyn_config: StorageTypeConfig) -> Result<Self, InvalidConfigType>
    where
        Self: Sized,
    {
        match dyn_config {
            StorageTypeConfig::S3(config) => Ok(*config),
            _ => Err(InvalidConfigType("S3Config", dyn_config.type_name())),
        }
    }

    fn type_name(&self) -> &'static str {
        "S3Config"
    }
}
