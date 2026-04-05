use nr_core::repository::config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryAuthConfig {
    #[serde(default = "RepositoryAuthConfig::enabled_default")]
    pub enabled: bool,
}

impl Default for RepositoryAuthConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl RepositoryAuthConfig {
    const fn enabled_default() -> bool {
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct RepositoryAuthConfigType;

impl RepositoryConfigType for RepositoryAuthConfigType {
    fn get_type(&self) -> &'static str {
        "auth"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "auth"
    }

    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(RepositoryAuthConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        serde_json::from_value::<RepositoryAuthConfig>(config)?;
        Ok(())
    }

    fn validate_change(&self, _old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        self.validate_config(new)
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(RepositoryAuthConfig::default())?)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Repository Authentication".into(),
            description: Some("Toggle whether read operations require authentication.".into()),
            documentation_link: None,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests;
