use nr_core::repository::{
    config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType},
    proxy_url::ProxyURL,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::repository::r#virtual::config::VirtualRepositoryConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", content = "config")]
pub enum PythonRepositoryConfig {
    #[default]
    Hosted,
    Proxy(PythonProxyConfig),
    Virtual(VirtualRepositoryConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct PythonProxyConfig {
    #[serde(default)]
    pub routes: Vec<PythonProxyRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PythonProxyRoute {
    pub url: ProxyURL,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PythonRepositoryConfigType;

impl RepositoryConfigType for PythonRepositoryConfigType {
    fn get_type(&self) -> &'static str {
        "python"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "python"
    }

    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(PythonRepositoryConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let parsed = serde_json::from_value::<PythonRepositoryConfig>(config)?;
        if let PythonRepositoryConfig::Virtual(virtual_cfg) = &parsed {
            crate::repository::r#virtual::config::validate_virtual_repository_config(virtual_cfg)
                .map_err(|_| RepositoryConfigError::InvalidConfig("Invalid virtual config"))?;
        }
        Ok(())
    }

    fn validate_change(&self, _old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        self.validate_config(new)
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(PythonRepositoryConfig::Hosted)?)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Python Repository Config",
            description: Some("Handles the type of Python repository."),
            documentation_link: Some(
                "https://pkgly.kingtux.dev/repositoryTypes/python/configs/",
            ),
            ..Default::default()
        }
    }
}
