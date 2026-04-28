use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cli::{GlobalArgs, OutputMode};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error("profile `{0}` not found")]
    ProfileNotFound(String),
    #[error("missing base URL; pass --base-url, set PKGLY_URL, or configure a profile")]
    MissingBaseUrl,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigFile {
    pub active_profile: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileConfig {
    pub base_url: Option<String>,
    pub token: Option<String>,
    pub default_storage: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    pub profile: Option<String>,
    pub config: Option<PathBuf>,
    pub base_url: Option<String>,
    pub token: Option<String>,
    pub output: Option<OutputMode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvConfig {
    pub base_url: Option<String>,
    pub token: Option<String>,
    pub profile: Option<String>,
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub profile: Option<String>,
    pub base_url: String,
    pub token: Option<String>,
    pub default_storage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileMutation {
    Use(String),
    Remove(String),
}

impl ConfigOverrides {
    pub fn from_global(global: &GlobalArgs) -> Self {
        Self {
            profile: global.profile.clone(),
            config: global.config.clone(),
            base_url: global.base_url.clone(),
            token: global.token.clone(),
            output: global.output,
        }
    }
}

impl EnvConfig {
    pub fn from_process() -> Self {
        Self {
            base_url: std::env::var("PKGLY_URL").ok(),
            token: std::env::var("PKGLY_TOKEN").ok(),
            profile: std::env::var("PKGLY_PROFILE").ok(),
            config: std::env::var_os("PKGLYCTL_CONFIG").map(PathBuf::from),
        }
    }
}

impl ConfigFile {
    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        set_owner_only_permissions(path)?;
        Ok(())
    }

    pub fn upsert_profile_token(&mut self, profile: &str, token: String) {
        let entry = self.profiles.entry(profile.to_string()).or_default();
        entry.token = Some(token);
    }
}

impl ResolvedConfig {
    pub fn resolve(
        config: &ConfigFile,
        overrides: &ConfigOverrides,
        env: &EnvConfig,
    ) -> Result<Self, ConfigError> {
        let profile_name = overrides
            .profile
            .clone()
            .or_else(|| env.profile.clone())
            .or_else(|| config.active_profile.clone());
        let profile = match &profile_name {
            Some(name) => Some(
                config
                    .profiles
                    .get(name)
                    .ok_or_else(|| ConfigError::ProfileNotFound(name.clone()))?,
            ),
            None => None,
        };

        let base_url = overrides
            .base_url
            .clone()
            .or_else(|| env.base_url.clone())
            .or_else(|| profile.and_then(|entry| entry.base_url.clone()))
            .unwrap_or_default();
        let token = overrides
            .token
            .clone()
            .or_else(|| env.token.clone())
            .or_else(|| profile.and_then(|entry| entry.token.clone()));
        let default_storage = profile.and_then(|entry| entry.default_storage.clone());

        Ok(Self {
            profile: profile_name,
            base_url,
            token,
            default_storage,
        })
    }

    pub fn require_complete(self) -> Result<Self, ConfigError> {
        if self.base_url.trim().is_empty() {
            return Err(ConfigError::MissingBaseUrl);
        }
        Ok(self)
    }

    pub fn require_base_url(&self) -> Result<String, ConfigError> {
        if self.base_url.trim().is_empty() {
            return Err(ConfigError::MissingBaseUrl);
        }
        Ok(self.base_url.clone())
    }
}

impl ProfileMutation {
    pub fn apply(self, config: &mut ConfigFile) -> Result<(), ConfigError> {
        match self {
            Self::Use(name) => {
                if !config.profiles.contains_key(&name) {
                    return Err(ConfigError::ProfileNotFound(name));
                }
                config.active_profile = Some(name);
            }
            Self::Remove(name) => {
                if config.profiles.remove(&name).is_none() {
                    return Err(ConfigError::ProfileNotFound(name));
                }
                if config.active_profile.as_deref() == Some(&name) {
                    config.active_profile = None;
                }
            }
        }
        Ok(())
    }
}

pub fn config_path(overrides: &ConfigOverrides, env: &EnvConfig) -> PathBuf {
    if let Some(path) = &overrides.config {
        return path.clone();
    }
    if let Some(path) = &env.config {
        return path.clone();
    }
    xdg_config_home().join("pkgly").join("config.toml")
}

fn xdg_config_home() -> PathBuf {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(value);
    }
    if let Some(value) = std::env::var_os("HOME") {
        return PathBuf::from(value).join(".config");
    }
    PathBuf::from(".")
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}
