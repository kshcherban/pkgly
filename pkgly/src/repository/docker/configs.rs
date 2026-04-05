use nr_core::repository::config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType};
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::proxy::DockerProxyConfig;

/// Docker Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "config")]
pub enum DockerRegistryConfig {
    /// Hosted Docker registry that stores images locally
    Hosted,
    /// Proxy Docker registry that caches images from upstream registries
    Proxy(DockerProxyConfig),
}

impl DockerRegistryConfig {
    pub fn is_same_type(&self, other: &DockerRegistryConfig) -> bool {
        matches!(
            (self, other),
            (DockerRegistryConfig::Hosted, DockerRegistryConfig::Hosted)
                | (
                    DockerRegistryConfig::Proxy(_),
                    DockerRegistryConfig::Proxy(_)
                )
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct DockerRegistryConfigType;

impl RepositoryConfigType for DockerRegistryConfigType {
    fn get_type(&self) -> &'static str {
        "docker"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "docker"
    }

    fn schema(&self) -> Option<Schema> {
        Some(schema_for!(DockerRegistryConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let _config: DockerRegistryConfig = serde_json::from_value(config)?;
        Ok(())
    }

    fn validate_change(&self, old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        let new: DockerRegistryConfig = serde_json::from_value(new)?;
        let old: DockerRegistryConfig = serde_json::from_value(old)?;
        if !old.is_same_type(&new) {
            return Err(RepositoryConfigError::InvalidChange(
                "docker",
                "Cannot change the type of Docker Registry",
            ));
        }
        Ok(())
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        let config = DockerRegistryConfig::Hosted;
        serde_json::to_value(config).map_err(RepositoryConfigError::SerdeError)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Docker Registry Config",
            description: Some("Handles the type of Docker Registry (Hosted or Proxy)"),
            documentation_link: None,
            ..Default::default()
        }
    }
}

/// Docker push rules configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DockerPushRules {
    /// If overwriting tags is allowed
    #[schemars(title = "Allow Tag Overwrite")]
    pub allow_tag_overwrite: bool,

    /// If the user must be a project member to push
    #[schemars(title = "Project Members Only")]
    pub must_be_project_member: bool,

    /// Require auth token for push operations
    #[schemars(title = "Require Auth Token for Push")]
    pub must_use_auth_token_for_push: bool,

    /// Enable content trust/image signing validation
    #[schemars(title = "Require Content Trust")]
    pub require_content_trust: bool,
}

impl Default for DockerPushRules {
    fn default() -> Self {
        Self {
            allow_tag_overwrite: false, // Docker best practice: immutable tags
            must_be_project_member: false,
            must_use_auth_token_for_push: false,
            require_content_trust: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DockerPushRulesConfigType;

impl RepositoryConfigType for DockerPushRulesConfigType {
    fn get_type(&self) -> &'static str {
        Self::get_type_static()
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "docker_push_rules"
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Docker Push Rules",
            description: Some("Rules for pushing to a Docker registry"),
            documentation_link: None,
            ..Default::default()
        }
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let _config: DockerPushRules = serde_json::from_value(config)?;
        Ok(())
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(DockerPushRules::default())?)
    }

    fn schema(&self) -> Option<Schema> {
        Some(schema_for!(DockerPushRules))
    }
}
