use ahash::{HashSet, HashSetExt};
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
pub enum GoRepositoryConfig {
    #[default]
    Hosted,
    Proxy(GoProxyConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GoProxyConfig {
    #[serde(default)]
    pub routes: Vec<GoProxyRoute>,
    #[serde(default)]
    pub go_module_cache_ttl: Option<u64>,
}

impl Default for GoProxyConfig {
    fn default() -> Self {
        let routes = match nr_core::repository::proxy_url::ProxyURL::try_from(
            "https://proxy.golang.org".to_string(),
        ) {
            Ok(url) => vec![GoProxyRoute {
                url,
                name: Some("Go Official Proxy".to_string()),
                priority: Some(0),
            }],
            Err(err) => {
                tracing::warn!(
                    ?err,
                    "Default Go proxy URL invalid, falling back to empty route set"
                );
                Vec::new()
            }
        };
        Self {
            routes,
            go_module_cache_ttl: Some(3600), // 1 hour default TTL
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GoProxyRoute {
    pub url: ProxyURL,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}

impl GoProxyRoute {
    /// Get the priority of this route, defaulting to 0 if not set
    pub fn priority(&self) -> i32 {
        self.priority.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GoRepositoryConfigType;

impl RepositoryConfigType for GoRepositoryConfigType {
    fn get_type(&self) -> &'static str {
        "go"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "go"
    }

    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(GoRepositoryConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let parsed: GoRepositoryConfig =
            serde_json::from_value(config).map_err(|e| RepositoryConfigError::SerdeError(e))?;

        match parsed {
            GoRepositoryConfig::Hosted => Ok(()),
            GoRepositoryConfig::Proxy(proxy_config) => {
                // Validate proxy configuration
                if proxy_config.routes.is_empty() {
                    return Err(RepositoryConfigError::InvalidConfig(
                        "Go proxy configuration must have at least one route",
                    ));
                }

                // Validate each route
                for (_index, route) in proxy_config.routes.iter().enumerate() {
                    // Validate URL format
                    let url_str = route.url.as_str();
                    if url_str.is_empty() {
                        return Err(RepositoryConfigError::InvalidConfig(
                            "Go proxy route has empty URL",
                        ));
                    }

                    // Try to parse as URL to ensure it's valid
                    let parsed_url = url::Url::parse(url_str).map_err(|_| {
                        RepositoryConfigError::InvalidConfig(
                            "Go proxy route has invalid URL format",
                        )
                    })?;

                    if !matches!(parsed_url.scheme(), "http" | "https") {
                        return Err(RepositoryConfigError::InvalidConfig(
                            "Go proxy routes must use http or https",
                        ));
                    }

                    // Ensure URL ends with / if it's supposed to be a base proxy URL
                    if !url_str.ends_with('/') {
                        tracing::warn!(
                            "Go proxy route URL '{}' should end with '/' for proper operation",
                            url_str
                        );
                    }
                }

                // Check for duplicate priorities
                let mut priorities = HashSet::new();
                for route in proxy_config.routes.iter() {
                    let priority = route.priority();
                    if !priorities.insert(priority) {
                        return Err(RepositoryConfigError::InvalidConfig(
                            "Go proxy routes must have unique priorities",
                        ));
                    }
                }

                // Validate TTL if provided
                if let Some(ttl) = proxy_config.go_module_cache_ttl {
                    if ttl == 0 {
                        tracing::warn!("Go proxy cache TTL is 0, caching will be disabled");
                    }
                }

                Ok(())
            }
        }
    }

    fn validate_change(&self, _old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        self.validate_config(new)
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(GoRepositoryConfig::Hosted)?)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Go Repository Config",
            description: Some("Handles the type of Go repository."),
            documentation_link: Some("https://pkgly.kingtux.dev/repositoryTypes/go/configs/"),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod go_config_tests;
