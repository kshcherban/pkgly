use nr_core::repository::{
    config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType},
    proxy_url::ProxyURL,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", content = "config")]
pub enum RubyRepositoryConfig {
    #[default]
    Hosted,
    Proxy(RubyProxyConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RubyProxyConfig {
    pub upstream_url: ProxyURL,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revalidation_ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct RubyRepositoryConfigType;

impl RepositoryConfigType for RubyRepositoryConfigType {
    fn get_type(&self) -> &'static str {
        "ruby"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "ruby"
    }

    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(RubyRepositoryConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        serde_json::from_value::<RubyRepositoryConfig>(config)?;
        Ok(())
    }

    fn validate_change(&self, _old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        self.validate_config(new)
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(RubyRepositoryConfig::Hosted)?)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Ruby Repository Config",
            description: Some("Handles the type of Ruby (RubyGems) repository."),
            documentation_link: Some(
                "https://pkgly.kingtux.dev/repositoryTypes/ruby/configs/",
            ),
            ..Default::default()
        }
    }
}
