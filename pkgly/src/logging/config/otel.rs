use ahash::{HashMap, HashMapExt};
use opentelemetry::{KeyValue, StringValue};
use serde::{Deserialize, Serialize};

use super::{AppLoggerType, LoggingLevels};

pub(crate) trait EnvProvider {
    fn get(&self, key: &str) -> Option<String>;
}

struct RealEnv;

impl EnvProvider for RealEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}
/// Tracing Config Resource Values.
///
/// ```toml
/// "service.name" = "pkgly"
/// "service.version" = "3.0.0-BETA"
/// "service.environment" = "development"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelResourceMap(pub HashMap<String, String>);
impl Default for OtelResourceMap {
    fn default() -> Self {
        let mut trace_config = HashMap::new();
        trace_config.insert("service.name".to_string(), "pkgly".to_string());
        trace_config.insert(
            "service.version".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );
        trace_config.insert("service.environment".to_string(), "development".to_string());
        Self(trace_config)
    }
}
impl From<OtelResourceMap> for opentelemetry_sdk::Resource {
    fn from(mut value: OtelResourceMap) -> Self {
        if !value.0.contains_key("service.name") {
            value
                .0
                .insert("service.name".to_string(), "pkgly".to_string());
        }
        let resources: Vec<KeyValue> = value
            .0
            .into_iter()
            .map(|(k, v)| KeyValue::new(k, Into::<StringValue>::into(v)))
            .collect();
        opentelemetry_sdk::Resource::builder()
            .with_attributes(resources)
            .build()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub protocol: TracingProtocol,
    /// Endpoint for the tracing collector.
    pub endpoint: String,
    /// Tracing Config Resource Values.
    pub config: OtelResourceMap,
}
impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            protocol: TracingProtocol::GRPC,
            endpoint: "http://localhost:4317".to_owned(),
            config: OtelResourceMap::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtelConfig {
    pub enabled: bool,
    pub protocol: TracingProtocol,
    /// Endpoint for the tracing collector.
    pub endpoint: String,
    /// Tracing Config Resource Values.
    pub config: OtelResourceMap,
    pub traces: bool,
    pub logs: bool,
    pub levels: LoggingLevels,
}

impl OtelConfig {
    /// Apply environment variable fallback logic
    /// This should be called after deserialization to apply environment variable overrides
    /// Note: Config file values take precedence over environment variables
    pub fn apply_env_fallback(self) -> Self {
        self.apply_env_fallback_with_env(&RealEnv)
    }

    pub(crate) fn apply_env_fallback_with_env<P: EnvProvider>(self, env: &P) -> Self {
        // Environment variables are applied during the Default() implementation
        // Since config file deserialization overrides defaults, we don't need to
        // do anything special here - the config file values already take precedence

        // Only apply endpoint env var if it wasn't overridden in config
        let mut endpoint = self.endpoint;
        if endpoint == "http://localhost:4317" {
            if let Some(otel_endpoint) = env.get("OTEL_EXPORTER_OTLP_ENDPOINT") {
                endpoint = otel_endpoint;
            }
        }

        OtelConfig { endpoint, ..self }
    }

    fn default_with_env<P: EnvProvider>(env: &P) -> Self {
        // Enable tracing if PKGLY_TRACING_ENABLED environment variable is set
        // This can be overridden by config file settings
        let enabled = env.get("PKGLY_TRACING_ENABLED").is_some();

        Self {
            enabled,
            protocol: TracingProtocol::GRPC,
            endpoint: env
                .get("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|| "http://localhost:4317".to_owned()),
            config: OtelResourceMap::default(),
            traces: true,
            logs: false, // Don't send logs to OTLP - logs should always be available locally
            levels: LoggingLevels::default(),
        }
    }
}
impl AppLoggerType for OtelConfig {
    fn get_levels_mut(&mut self) -> &mut LoggingLevels {
        &mut self.levels
    }
}
impl Default for OtelConfig {
    fn default() -> Self {
        Self::default_with_env(&RealEnv)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TracingProtocol {
    GRPC,
    /// Not Implemented Yet
    HttpBinary,
    HttpJson,
}

impl Default for TracingProtocol {
    fn default() -> Self {
        TracingProtocol::GRPC
    }
}
impl From<TracingProtocol> for opentelemetry_otlp::Protocol {
    fn from(value: TracingProtocol) -> Self {
        match value {
            TracingProtocol::GRPC => opentelemetry_otlp::Protocol::Grpc,
            TracingProtocol::HttpBinary => opentelemetry_otlp::Protocol::HttpBinary,
            TracingProtocol::HttpJson => opentelemetry_otlp::Protocol::HttpJson,
        }
    }
}

#[cfg(test)]
mod tests;
