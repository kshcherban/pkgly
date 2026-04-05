use std::path::PathBuf;

use nr_core::database::DatabaseConfig;
use serde::{Deserialize, Serialize};

use crate::{
    app::authentication::session::SessionManagerConfig,
    app::config::{Mode, SecuritySettings, SiteSetting, WebServer},
    app::email::EmailSetting,
    logging::config::{LoggingConfig, OtelConfig},
    repository::StagingConfig,
};

/// Top-level configuration structure for Pkgly.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct PkglyConfig {
    pub mode: Mode,
    pub web_server: WebServer,
    pub suggested_local_storage_path: Option<PathBuf>,
    pub database: DatabaseConfig,
    pub log: LoggingConfig,
    pub opentelemetry: OtelConfig,
    pub sessions: SessionManagerConfig,
    pub site: SiteSetting,
    pub security: SecuritySettings,
    pub staging: StagingConfig,
    pub email: Option<EmailSetting>,
}
