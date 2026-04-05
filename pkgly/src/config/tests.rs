use std::io::Write;

use tempfile::NamedTempFile;

use crate::config::{Mode, load_config};

#[test]
fn load_config_uses_defaults_without_file_or_env() {
    let config = load_config(None).expect("load_config should succeed without config");

    assert_eq!(config.mode, Mode::default());
    assert_eq!(config.web_server.bind_address, "0.0.0.0:6742");
}

#[test]
fn load_config_prefers_file_over_environment_for_mode() {
    let mut file = NamedTempFile::new().expect("temp config file");
    // Explicitly set mode to Release in the config file.
    write!(file, "mode = \"Release\"").expect("write config");

    let config =
        load_config(Some(file.path().to_path_buf())).expect("load_config should succeed with file");

    assert_eq!(config.mode, Mode::Release);
}
