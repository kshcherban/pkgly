// ABOUTME: Tests that the default test logger config keeps third-party debug spam out.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn default_levels_silence_third_party_crates() {
    let config = TestingLoggerConfig::default();
    assert_eq!(config.levels.default, LevelSerde::Warn);
    for crate_name in ["pkgly", "nr_core", "nr_storage"] {
        assert_eq!(
            config.levels.others.get(crate_name),
            Some(&LevelSerde::Debug),
            "{crate_name} should stay at Debug"
        );
    }
    assert!(!config.levels.others.contains_key("aws_smithy_runtime"));
    assert!(!config.levels.others.contains_key("h2"));
}
