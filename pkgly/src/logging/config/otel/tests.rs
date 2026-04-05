#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use std::collections::HashMap;

#[derive(Default)]
struct FakeEnv {
    values: HashMap<&'static str, &'static str>,
}

impl FakeEnv {
    fn with(mut self, key: &'static str, value: &'static str) -> Self {
        self.values.insert(key, value);
        self
    }
}

impl EnvProvider for FakeEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).map(|value| (*value).to_string())
    }
}

#[test]
fn test_tracing_protocol_default() {
    let protocol = TracingProtocol::default();
    assert!(matches!(protocol, TracingProtocol::GRPC));
}

#[test]
fn test_env_var_fallback() {
    let empty_env = FakeEnv::default();
    let config1 = OtelConfig::default_with_env(&empty_env);
    assert!(!config1.enabled);

    let enabled_env = FakeEnv::default().with("PKGLY_TRACING_ENABLED", "1");
    let config2 = OtelConfig::default_with_env(&enabled_env);
    assert!(config2.enabled);

    let config3 = OtelConfig {
        enabled: false,
        ..config2.clone()
    };
    assert!(!config3.enabled); // Should stay false as explicitly set

    let config4 = config3.apply_env_fallback_with_env(&enabled_env);
    assert!(!config4.enabled);
}

#[test]
fn test_endpoint_env_fallback() {
    let empty_env = FakeEnv::default();
    let config1 = OtelConfig::default_with_env(&empty_env).apply_env_fallback_with_env(&empty_env);
    assert_eq!(config1.endpoint, "http://localhost:4317");

    let endpoint_env = FakeEnv::default().with(
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "http://custom-collector:9999",
    );
    let config2 =
        OtelConfig::default_with_env(&endpoint_env).apply_env_fallback_with_env(&endpoint_env);
    assert_eq!(config2.endpoint, "http://custom-collector:9999");

    // Test that explicit config file value overrides env var
    let config3 = OtelConfig {
        endpoint: "http://explicit-config:8080".to_string(),
        ..OtelConfig::default_with_env(&empty_env)
    }
    .apply_env_fallback_with_env(&endpoint_env);
    assert_eq!(config3.endpoint, "http://explicit-config:8080");
}
