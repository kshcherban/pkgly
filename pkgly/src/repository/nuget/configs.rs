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
pub enum NugetRepositoryConfig {
    #[default]
    Hosted,
    Proxy(NugetProxyConfig),
    Virtual(VirtualRepositoryConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NugetProxyConfig {
    pub upstream_url: ProxyURL,
}

#[derive(Debug, Clone, Default)]
pub struct NugetRepositoryConfigType;

impl RepositoryConfigType for NugetRepositoryConfigType {
    fn get_type(&self) -> &'static str {
        "nuget"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "nuget"
    }

    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(NugetRepositoryConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let parsed = serde_json::from_value::<NugetRepositoryConfig>(config)?;
        if let NugetRepositoryConfig::Virtual(virtual_cfg) = &parsed {
            crate::repository::r#virtual::config::validate_virtual_repository_config(virtual_cfg)
                .map_err(|_| RepositoryConfigError::InvalidConfig("Invalid virtual config"))?;
        }
        Ok(())
    }

    fn validate_change(&self, _old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        self.validate_config(new)
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(NugetRepositoryConfig::Hosted)?)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "NuGet Repository Config",
            description: Some(
                "Controls whether the NuGet repository is hosted, proxy, or virtual.",
            ),
            documentation_link: Some("https://pkgly.kingtux.dev/repositoryTypes/nuget/configs"),
            ..Default::default()
        }
    }
}
