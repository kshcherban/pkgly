#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn default_config_has_reasonable_values() {
    let config = DebRepositoryConfig::default();
    let hosted = match config {
        DebRepositoryConfig::Hosted(hosted) => hosted,
        DebRepositoryConfig::Proxy(_) => panic!("default deb config should be hosted"),
    };
    assert_eq!(hosted.distributions, vec!["stable".to_string()]);
    assert_eq!(hosted.components, vec!["main".to_string()]);
    assert!(hosted.architectures.contains(&"amd64".to_string()));
}

#[test]
fn validation_rejects_empty_distribution() {
    let config = DebRepositoryConfig::Hosted(DebHostedConfig {
        distributions: vec![],
        components: vec!["main".into()],
        architectures: vec!["amd64".into()],
    });
    let serialized = serde_json::to_value(&config).expect("serde");
    let err = DebRepositoryConfigType
        .validate_config(serialized)
        .expect_err("should reject empty distributions");
    let message = err.to_string();
    assert!(
        message.contains("distribution"),
        "unexpected message: {message}"
    );
}

#[test]
fn validation_rejects_invalid_identifier() {
    let config = DebRepositoryConfig::Hosted(DebHostedConfig {
        distributions: vec!["stable".into()],
        components: vec!["main".into()],
        architectures: vec!["amd64!".into()],
    });
    let serialized = serde_json::to_value(&config).expect("serde");
    let err = DebRepositoryConfigType
        .validate_config(serialized)
        .expect_err("should reject invalid characters");
    assert!(
        err.to_string().contains("alphanumeric characters"),
        "unexpected message: {}",
        err
    );
}

#[test]
fn deserializes_legacy_hosted_shape_as_hosted() {
    let legacy = serde_json::json!({
        "distributions": ["stable"],
        "components": ["main"],
        "architectures": ["amd64", "all"]
    });

    let parsed: DebRepositoryConfig = serde_json::from_value(legacy).expect("legacy parse");
    assert!(
        matches!(parsed, DebRepositoryConfig::Hosted(_)),
        "expected legacy shape to deserialize as hosted"
    );
}

#[test]
fn validate_change_rejects_switching_hosted_to_proxy() {
    let old = serde_json::to_value(DebRepositoryConfig::Hosted(DebHostedConfig::default()))
        .expect("old config");
    let new = serde_json::json!({
        "type": "proxy",
        "config": {
            "upstream_url": "https://deb.example.com/debian",
            "layout": {
                "type": "dists",
                "config": {
                    "distributions": ["stable"],
                    "components": ["main"],
                    "architectures": ["amd64", "all"]
                }
            }
        }
    });

    let err = DebRepositoryConfigType
        .validate_change(old, new)
        .expect_err("should reject switching types");
    assert!(
        matches!(err, RepositoryConfigError::InvalidChange(..)),
        "unexpected error: {err}"
    );
}

#[test]
fn proxy_refresh_schedule_accepts_interval_seconds() {
    let config = serde_json::json!({
        "type": "proxy",
        "config": {
            "upstream_url": "https://deb.example.com/debian",
            "layout": {
                "type": "flat",
                "config": {
                    "distribution": "./",
                    "architectures": []
                }
            },
            "refresh": {
                "enabled": true,
                "schedule": {
                    "type": "interval_seconds",
                    "config": { "interval_seconds": 3600 }
                }
            }
        }
    });

    DebRepositoryConfigType
        .validate_config(config)
        .expect("config should validate");
}

#[test]
fn proxy_refresh_schedule_rejects_zero_interval() {
    let config = serde_json::json!({
        "type": "proxy",
        "config": {
            "upstream_url": "https://deb.example.com/debian",
            "layout": {
                "type": "flat",
                "config": {
                    "distribution": "./",
                    "architectures": []
                }
            },
            "refresh": {
                "enabled": true,
                "schedule": {
                    "type": "interval_seconds",
                    "config": { "interval_seconds": 0 }
                }
            }
        }
    });

    let err = DebRepositoryConfigType
        .validate_config(config)
        .expect_err("zero interval should be rejected");
    assert!(
        err.to_string().to_ascii_lowercase().contains("interval"),
        "unexpected error: {err}"
    );
}

#[test]
fn proxy_refresh_schedule_accepts_5_field_cron() {
    let config = serde_json::json!({
        "type": "proxy",
        "config": {
            "upstream_url": "https://deb.example.com/debian",
            "layout": {
                "type": "flat",
                "config": {
                    "distribution": "./",
                    "architectures": []
                }
            },
            "refresh": {
                "enabled": true,
                "schedule": {
                    "type": "cron",
                    "config": { "expression": "0 3 * * *" }
                }
            }
        }
    });

    DebRepositoryConfigType
        .validate_config(config)
        .expect("cron should validate");
}

#[test]
fn proxy_refresh_schedule_rejects_invalid_cron() {
    let config = serde_json::json!({
        "type": "proxy",
        "config": {
            "upstream_url": "https://deb.example.com/debian",
            "layout": {
                "type": "flat",
                "config": {
                    "distribution": "./",
                    "architectures": []
                }
            },
            "refresh": {
                "enabled": true,
                "schedule": {
                    "type": "cron",
                    "config": { "expression": "not a cron" }
                }
            }
        }
    });

    let err = DebRepositoryConfigType
        .validate_config(config)
        .expect_err("invalid cron should be rejected");
    assert!(
        err.to_string().to_ascii_lowercase().contains("cron"),
        "unexpected error: {err}"
    );
}
