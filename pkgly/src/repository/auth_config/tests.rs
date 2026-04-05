#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn default_auth_config_is_enabled() {
    let default = RepositoryAuthConfigType.default().expect("default config");
    let value: RepositoryAuthConfig = serde_json::from_value(default).expect("serde");
    assert!(value.enabled, "auth should be enabled by default");
}
