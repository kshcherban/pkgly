use nr_core::repository::{
    config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType},
    proxy_url::ProxyURL,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{npm_virtual::NpmVirtualConfig, validate_virtual_config};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", content = "config")]
pub enum NPMRegistryConfig {
    #[default]
    Hosted,
    Proxy(NpmProxyConfig),
    Virtual(NpmVirtualConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct NpmProxyConfig {
    #[serde(default)]
    pub routes: Vec<NpmProxyRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NpmProxyRoute {
    pub url: ProxyURL,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NPMRegistryConfigType;
impl RepositoryConfigType for NPMRegistryConfigType {
    fn get_type(&self) -> &'static str {
        "npm"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "npm"
    }
    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(NPMRegistryConfig))
    }
    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let parsed = serde_json::from_value::<NPMRegistryConfig>(config)?;
        if let NPMRegistryConfig::Virtual(virtual_cfg) = &parsed {
            validate_virtual_config(virtual_cfg)
                .map_err(|_| RepositoryConfigError::InvalidConfig("Invalid virtual config"))?;
        }
        Ok(())
    }
    fn validate_change(&self, _old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        self.validate_config(new)
    }
    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(NPMRegistryConfig::Hosted)?)
    }
    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "NPM Repository Config",
            description: Some("Handles the type of NPM Registry"),
            documentation_link: None,
            ..Default::default()
        }
    }
}
