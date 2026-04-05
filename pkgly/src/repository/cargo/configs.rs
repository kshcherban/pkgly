use nr_core::repository::config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Configuration options for the Cargo repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", content = "config")]
pub enum CargoRepositoryConfig {
    #[default]
    Hosted,
}

#[derive(Debug, Clone, Default)]
pub struct CargoRepositoryConfigType;

impl RepositoryConfigType for CargoRepositoryConfigType {
    fn get_type(&self) -> &'static str {
        "cargo"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "cargo"
    }

    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(CargoRepositoryConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let _ = serde_json::from_value::<CargoRepositoryConfig>(config)?;
        Ok(())
    }

    fn validate_change(&self, _old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        self.validate_config(new)
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(CargoRepositoryConfig::Hosted)?)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Cargo Repository Config",
            description: Some("Configures the Cargo registry (hosted mode)."),
            documentation_link: Some("https://doc.rust-lang.org/cargo/reference/registries.html"),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests;
