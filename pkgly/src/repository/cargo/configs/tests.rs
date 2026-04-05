#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn default_returns_hosted() {
    let config_type = CargoRepositoryConfigType;
    let value = config_type.default().expect("default config");
    let parsed: CargoRepositoryConfig = serde_json::from_value(value).expect("parse default");
    assert_eq!(parsed, CargoRepositoryConfig::Hosted);
}

#[test]
fn validate_accepts_hosted() {
    let config_type = CargoRepositoryConfigType;
    let value = serde_json::to_value(CargoRepositoryConfig::Hosted).unwrap();
    assert!(config_type.validate_config(value).is_ok());
}
