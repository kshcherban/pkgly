use std::path::PathBuf;

use pkgly_cli::config::{
    ConfigFile, ConfigOverrides, EnvConfig, ProfileConfig, ProfileMutation, ResolvedConfig,
};

fn sample_config() -> ConfigFile {
    let mut config = ConfigFile {
        active_profile: Some("local".to_string()),
        profiles: Default::default(),
    };
    config.profiles.insert(
        "local".to_string(),
        ProfileConfig {
            base_url: Some("http://file.example".to_string()),
            token: Some("file-token".to_string()),
            default_storage: Some("file-storage".to_string()),
        },
    );
    config
}

#[test]
fn config_resolution_prefers_flags_then_environment_then_profile() {
    let resolved = ResolvedConfig::resolve(
        &sample_config(),
        &ConfigOverrides {
            profile: None,
            config: None,
            base_url: Some("http://flag.example".to_string()),
            token: None,
            output: None,
        },
        &EnvConfig {
            base_url: Some("http://env.example".to_string()),
            token: Some("env-token".to_string()),
            profile: None,
            config: None,
        },
    )
    .and_then(|value| value.require_complete());

    let resolved = match resolved {
        Ok(value) => value,
        Err(err) => panic!("unexpected resolution error: {err}"),
    };
    assert_eq!(resolved.base_url, "http://flag.example");
    assert_eq!(resolved.token.as_deref(), Some("env-token"));
    assert_eq!(resolved.default_storage.as_deref(), Some("file-storage"));
}

#[test]
fn env_profile_selects_profile_when_flag_profile_is_absent() {
    let mut config = sample_config();
    config.profiles.insert(
        "prod".to_string(),
        ProfileConfig {
            base_url: Some("https://pkgly.example".to_string()),
            token: Some("prod-token".to_string()),
            default_storage: None,
        },
    );

    let resolved = ResolvedConfig::resolve(
        &config,
        &ConfigOverrides::default(),
        &EnvConfig {
            profile: Some("prod".to_string()),
            ..EnvConfig::default()
        },
    );

    let resolved = match resolved {
        Ok(value) => value,
        Err(err) => panic!("unexpected resolution error: {err}"),
    };
    assert_eq!(resolved.profile.as_deref(), Some("prod"));
    assert_eq!(resolved.base_url, "https://pkgly.example");
    assert_eq!(resolved.token.as_deref(), Some("prod-token"));
}

#[test]
fn profile_mutation_sets_active_profile_and_removes_profiles() {
    let mut config = sample_config();
    ProfileMutation::Use("local".to_string())
        .apply(&mut config)
        .unwrap_or_else(|err| panic!("failed to use profile: {err}"));
    assert_eq!(config.active_profile.as_deref(), Some("local"));

    ProfileMutation::Remove("local".to_string())
        .apply(&mut config)
        .unwrap_or_else(|err| panic!("failed to remove profile: {err}"));
    assert!(!config.profiles.contains_key("local"));
    assert_eq!(config.active_profile, None);
}

#[test]
fn default_config_path_prefers_explicit_then_env_then_xdg() {
    let explicit = PathBuf::from("/tmp/pkgly-explicit.toml");
    let env = EnvConfig {
        config: Some(PathBuf::from("/tmp/pkgly-env.toml")),
        ..EnvConfig::default()
    };
    let overrides = ConfigOverrides {
        config: Some(explicit.clone()),
        ..ConfigOverrides::default()
    };
    assert_eq!(pkgly_cli::config::config_path(&overrides, &env), explicit);

    let env_path = pkgly_cli::config::config_path(&ConfigOverrides::default(), &env);
    assert_eq!(env_path, PathBuf::from("/tmp/pkgly-env.toml"));
}
