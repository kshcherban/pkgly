// ABOUTME: Verifies serialized public app state exposed by API responses.
// ABOUTME: Covers build metadata fields consumed by the frontend.
use semver::Version;

use crate::app::{config::Mode, state::Instance};

#[test]
fn instance_serializes_commit_id_with_version() {
    let instance = Instance {
        app_url: "http://localhost:6742".to_string(),
        name: "Pkgly".to_string(),
        description: "Repository Server".to_string(),
        is_https: false,
        is_installed: true,
        version: Version::new(1, 2, 3),
        commit_id: Some("abc1234".to_string()),
        mode: Mode::Debug,
        password_rules: None,
        sso: None,
        oauth2: None,
    };

    let value = serde_json::to_value(instance).expect("instance should serialize");

    assert_eq!(value["version"], "1.2.3");
    assert_eq!(value["commit_id"], "abc1234");
}
