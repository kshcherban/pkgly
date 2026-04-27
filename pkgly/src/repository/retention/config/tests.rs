#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn package_retention_default_is_disabled() {
    let value = PackageRetentionConfigType
        .default()
        .expect("default config");
    let config: PackageRetentionConfig = serde_json::from_value(value).expect("deserialize");

    assert!(!config.enabled);
    assert_eq!(config.max_age_days, 30);
    assert_eq!(config.keep_latest_per_package, 1);
}

#[test]
fn validates_positive_age_and_zero_keep_latest() {
    let value = serde_json::json!({
        "enabled": true,
        "max_age_days": 1,
        "keep_latest_per_package": 0
    });

    PackageRetentionConfigType
        .validate_config(value)
        .expect("valid retention config");
}

#[test]
fn rejects_zero_max_age_days() {
    let value = serde_json::json!({
        "enabled": true,
        "max_age_days": 0,
        "keep_latest_per_package": 1
    });

    let err = PackageRetentionConfigType
        .validate_config(value)
        .expect_err("zero age must be rejected");
    assert!(err.to_string().contains("max_age_days"));
}
