mod loading;
mod validation;

pub use crate::app::config::{ConfigError, Mode, SiteSetting, WebServer};
pub use loading::load_config;
pub use validation::PkglyConfig;

/// ReadConfigType is kept here as an internal detail of configuration loading.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Default)]
#[serde(default)]
pub struct ReadConfigType {
    pub mode: Option<Mode>,
    pub suggested_local_storage_path: Option<std::path::PathBuf>,
    pub web_server: Option<WebServer>,
    pub database: Option<nr_core::database::DatabaseConfig>,
    pub log: Option<crate::logging::config::LoggingConfig>,
    pub opentelemetry: Option<crate::logging::config::OtelConfig>,
    pub sessions: Option<crate::app::authentication::session::SessionManagerConfig>,
    pub email: Option<crate::app::email::EmailSetting>,
    pub site: Option<SiteSetting>,
    pub security: Option<crate::app::config::SecuritySettings>,
    pub staging: Option<crate::repository::StagingConfig>,
}

#[cfg(test)]
mod tests;
