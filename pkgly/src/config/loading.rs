use std::{fs::read_to_string, path::PathBuf};

use super::{PkglyConfig, ReadConfigType};
use crate::app::config::{CONFIG_PREFIX, env_or_file_or_default, env_or_file_or_none};

/// Load the configuration from the environment or a configuration file.
///
/// Config file gets precedence over environment variables.
pub fn load_config(path: Option<PathBuf>) -> anyhow::Result<PkglyConfig> {
    let environment: ReadConfigType = serde_env::from_env_with_prefix(CONFIG_PREFIX)?;
    let config_from_file = if let Some(path) = path.filter(|path| path.exists() && path.is_file()) {
        let contents = read_to_string(path)?;
        toml::from_str(&contents)?
    } else {
        ReadConfigType::default()
    };

    let (mode, web_server, database, log, opentelemetry, sessions, site, security, staging) = env_or_file_or_default!(
        config_from_file,
        environment,
        mode,
        web_server,
        database,
        log,
        opentelemetry,
        sessions,
        site,
        security,
        staging
    );

    let email = env_or_file_or_none!(config_from_file, environment, email);
    let suggested_local_storage_path =
        env_or_file_or_none!(config_from_file, environment, suggested_local_storage_path);

    let opentelemetry = opentelemetry.apply_env_fallback();

    Ok(PkglyConfig {
        mode,
        web_server,
        database,
        log,
        opentelemetry,
        sessions,
        site,
        security,
        staging,
        email,
        suggested_local_storage_path,
    })
}
