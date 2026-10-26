// ABOUTME: Tests that the default test logger config keeps third-party debug spam out.
// ABOUTME: Preserves Debug logging for project crates and Warn for dependencies.
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

#[test]
fn passing_test_logs_use_the_test_harness() {
    const PROBE: &str = "pkgly-test-log-capture-probe";
    const CHILD: &str = "PKGLY_TEST_LOG_CAPTURE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        TestingLoggerConfig::default().init();
        tracing::warn!("{PROBE}");
        return;
    }

    for capture in [true, false] {
        let mut command = std::process::Command::new(std::env::current_exe().expect("test binary"));
        command
            .args([
                "--exact",
                "testing::logging::tests::passing_test_logs_use_the_test_harness",
            ])
            .env(CHILD, "1");
        if !capture {
            command.arg("--nocapture");
        }
        let output = command.output().expect("run child test");
        let stdout = String::from_utf8(output.stdout).expect("test output is UTF-8");
        assert!(output.status.success(), "{stdout}");
        assert!(
            stdout.contains("1 passed"),
            "probe test must execute: {stdout}"
        );
        assert_eq!(stdout.contains(PROBE), !capture, "{stdout}");
    }
}
