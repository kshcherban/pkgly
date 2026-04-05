use nr_core::repository::config::{ConfigDescription, RepositoryConfigError, RepositoryConfigType};
use nr_core::repository::proxy_url::ProxyURL;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DebHostedConfig {
    pub distributions: Vec<String>,
    pub components: Vec<String>,
    pub architectures: Vec<String>,
}

impl Default for DebHostedConfig {
    fn default() -> Self {
        Self {
            distributions: vec!["stable".into()],
            components: vec!["main".into()],
            architectures: vec!["amd64".into(), "all".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "config", rename_all = "lowercase")]
enum DebRepositoryConfigTagged {
    Hosted(DebHostedConfig),
    Proxy(DebProxyConfig),
}

#[derive(Debug, Clone, JsonSchema, PartialEq, Eq)]
pub enum DebRepositoryConfig {
    Hosted(DebHostedConfig),
    Proxy(DebProxyConfig),
}

impl DebRepositoryConfig {
    pub fn is_same_type(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Hosted(_), Self::Hosted(_)) | (Self::Proxy(_), Self::Proxy(_))
        )
    }
}

impl Default for DebRepositoryConfig {
    fn default() -> Self {
        Self::Hosted(DebHostedConfig::default())
    }
}

impl Serialize for DebRepositoryConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            // Keep backward compatibility for hosted repositories: serialize using the legacy shape.
            Self::Hosted(config) => config.serialize(serializer),
            Self::Proxy(config) => {
                DebRepositoryConfigTagged::Proxy(config.clone()).serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for DebRepositoryConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if value.get("type").is_some() {
            let tagged: DebRepositoryConfigTagged =
                serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            return Ok(match tagged {
                DebRepositoryConfigTagged::Hosted(config) => Self::Hosted(config),
                DebRepositoryConfigTagged::Proxy(config) => Self::Proxy(config),
            });
        }
        let hosted: DebHostedConfig =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self::Hosted(hosted))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DebProxyConfig {
    pub upstream_url: ProxyURL,
    pub layout: DebProxyLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<DebProxyRefreshConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DebProxyRefreshConfig {
    #[serde(default)]
    pub enabled: bool,
    pub schedule: DebProxyRefreshSchedule,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum DebProxyRefreshSchedule {
    IntervalSeconds(DebProxyIntervalSchedule),
    Cron(DebProxyCronSchedule),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DebProxyIntervalSchedule {
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DebProxyCronSchedule {
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "config", rename_all = "lowercase")]
pub enum DebProxyLayout {
    Dists(DebProxyDistsLayout),
    Flat(DebProxyFlatLayout),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DebProxyDistsLayout {
    pub distributions: Vec<String>,
    pub components: Vec<String>,
    pub architectures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DebProxyFlatLayout {
    pub distribution: String,
    #[serde(default)]
    pub architectures: Vec<String>,
}

pub(crate) fn normalize_cron_expression(expression: &str) -> Result<String, &'static str> {
    let parts: Vec<&str> = expression.split_whitespace().collect();
    match parts.len() {
        5 => Ok(format!("0 {} *", parts.join(" "))),
        6 => Ok(format!("{} *", parts.join(" "))),
        7 => Ok(parts.join(" ")),
        _ => Err("Cron expression must have 5, 6, or 7 fields"),
    }
}

fn validate_refresh_config(refresh: &DebProxyRefreshConfig) -> Result<(), RepositoryConfigError> {
    if !refresh.enabled {
        return Ok(());
    }
    match &refresh.schedule {
        DebProxyRefreshSchedule::IntervalSeconds(interval) => {
            if interval.interval_seconds == 0 {
                return Err(RepositoryConfigError::InvalidConfig(
                    "Interval seconds must be greater than 0",
                ));
            }
        }
        DebProxyRefreshSchedule::Cron(cron) => {
            let normalized = normalize_cron_expression(&cron.expression)
                .map_err(RepositoryConfigError::InvalidConfig)?;
            cron::Schedule::from_str(&normalized)
                .map_err(|_| RepositoryConfigError::InvalidConfig("Invalid cron expression"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Default, Clone)]
pub struct DebRepositoryConfigType;

impl RepositoryConfigType for DebRepositoryConfigType {
    fn get_type(&self) -> &'static str {
        "deb"
    }

    fn get_type_static() -> &'static str
    where
        Self: Sized,
    {
        "deb"
    }

    fn schema(&self) -> Option<schemars::Schema> {
        Some(schema_for!(DebRepositoryConfig))
    }

    fn validate_config(&self, config: Value) -> Result<(), RepositoryConfigError> {
        let decoded: DebRepositoryConfig = serde_json::from_value(config)?;
        match decoded {
            DebRepositoryConfig::Hosted(hosted) => {
                validate_identifier_list(
                    &hosted.distributions,
                    "At least one distribution is required",
                )?;
                validate_identifier_list(&hosted.components, "At least one component is required")?;
                validate_identifier_list(
                    &hosted.architectures,
                    "At least one architecture is required",
                )?;
            }
            DebRepositoryConfig::Proxy(proxy) => {
                match proxy.layout {
                    DebProxyLayout::Dists(dists) => {
                        validate_identifier_list(
                            &dists.distributions,
                            "At least one distribution is required",
                        )?;
                        validate_identifier_list(
                            &dists.components,
                            "At least one component is required",
                        )?;
                        validate_identifier_list(
                            &dists.architectures,
                            "At least one architecture is required",
                        )?;
                    }
                    DebProxyLayout::Flat(flat) => {
                        if flat.distribution.trim().is_empty() {
                            return Err(RepositoryConfigError::InvalidConfig(
                                "Flat distribution is required",
                            ));
                        }
                        validate_optional_identifier_list(&flat.architectures)?;
                    }
                }

                if let Some(refresh) = proxy.refresh.as_ref() {
                    validate_refresh_config(refresh)?;
                }
            }
        }
        Ok(())
    }

    fn validate_change(&self, old: Value, new: Value) -> Result<(), RepositoryConfigError> {
        let new_decoded: DebRepositoryConfig = serde_json::from_value(new.clone())?;
        let old_decoded: DebRepositoryConfig = serde_json::from_value(old)?;
        if !old_decoded.is_same_type(&new_decoded) {
            return Err(RepositoryConfigError::InvalidChange(
                "deb",
                "Cannot change the type of Debian Repository",
            ));
        }
        self.validate_config(new)
    }

    fn default(&self) -> Result<Value, RepositoryConfigError> {
        Ok(serde_json::to_value(DebRepositoryConfig::default())?)
    }

    fn get_description(&self) -> ConfigDescription {
        ConfigDescription {
            name: "Debian Repository Config".into(),
            description: Some("Configure distributions, components, and architectures.".into()),
            documentation_link: None,
            ..Default::default()
        }
    }
}

fn validate_identifier_list(
    values: &[String],
    empty_error: &'static str,
) -> Result<(), RepositoryConfigError> {
    if values.is_empty() {
        return Err(RepositoryConfigError::InvalidConfig(empty_error));
    }
    for value in values {
        if !is_valid_identifier(value) {
            return Err(RepositoryConfigError::InvalidConfig(
                "Values may only contain alphanumeric characters, '-', '_' or '.'",
            ));
        }
    }
    Ok(())
}

fn validate_optional_identifier_list(values: &[String]) -> Result<(), RepositoryConfigError> {
    for value in values {
        if !is_valid_identifier(value) {
            return Err(RepositoryConfigError::InvalidConfig(
                "Values may only contain alphanumeric characters, '-', '_' or '.'",
            ));
        }
    }
    Ok(())
}

fn is_valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests;
