#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use serde_json::json;

#[test]
fn default_config_sets_expected_values() {
    let config_type = HelmRepositoryConfigType;
    let default_value = config_type.default().expect("default config should build");
    let parsed: HelmRepositoryConfig = serde_json::from_value(default_value).unwrap();
    assert!(!parsed.overwrite);
    assert_eq!(parsed.mode, HelmRepositoryMode::Http);
    assert_eq!(parsed.max_chart_size, Some(10 * 1024 * 1024));
    assert_eq!(parsed.max_file_count, Some(1024));
}

#[test]
fn validates_public_base_url_format() {
    let config_type = HelmRepositoryConfigType;
    let valid = json!({
        "overwrite": true,
        "mode": "http",
        "public_base_url": "https://charts.example.com/helm",
        "max_chart_size": 20971520
    });
    assert!(config_type.validate_config(valid).is_ok());

    let invalid = json!({
        "mode": "http",
        "public_base_url": "not a url"
    });
    assert!(config_type.validate_config(invalid).is_err());
}

#[test]
fn rejects_negative_chart_limits() {
    let config_type = HelmRepositoryConfigType;
    let invalid = json!({
        "max_chart_size": -1,
        "max_file_count": -10
    });
    assert!(config_type.validate_config(invalid).is_err());
}

#[test]
fn rejects_hybrid_mode_config() {
    let err = serde_json::from_value::<HelmRepositoryConfig>(json!({
        "mode": "hybrid"
    }));
    assert!(err.is_err(), "hybrid mode should no longer deserialize");
}
