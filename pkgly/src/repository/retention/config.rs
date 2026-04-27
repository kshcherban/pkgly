use nr_core::repository::config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType};
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(default)]
pub struct PackageRetentionConfig {
    pub enabled: bool,
    #[serde(default = "PackageRetentionConfig::default_max_age_days")]
    pub max_age_days: u64,
    #[serde(default = "PackageRetentionConfig::default_keep_latest_per_package")]
    pub keep_latest_per_package: u32,
}

impl Default for PackageRetentionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_age_days: Self::default_max_age_days(),
            keep_latest_per_package: Self::default_keep_latest_per_package(),
        }
    }
}

impl PackageRetentionConfig {
    const fn default_max_age_days() -> u64 {
        30
    }

    const fn default_keep_latest_per_package() -> u32 {
        1
    }
}

#[derive(Debug, Clone, Default)]
pub struct PackageRetentionConfigType;

impl RepositoryConfigType for PackageRetentionConfigType {
    fn get_type(&self) -> &'static str {
        Self::get_type_static()
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "package_retention"
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Package Retention",
            description: Some(
                "Deletes package files older than the configured age while keeping the newest files per package.",
            ),
            documentation_link: Some("https://pkgly.kingtux.dev/sysAdmin/retention/"),
            ..Default::default()
        }
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let config: PackageRetentionConfig = serde_json::from_value(config)?;
        if config.max_age_days == 0 {
            return Err(RepositoryConfigError::InvalidConfig(
                "max_age_days must be greater than or equal to 1",
            ));
        }
        Ok(())
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(PackageRetentionConfig::default())?)
    }

    fn schema(&self) -> Option<Schema> {
        Some(schema_for!(PackageRetentionConfig))
    }
}

#[cfg(test)]
mod tests;
