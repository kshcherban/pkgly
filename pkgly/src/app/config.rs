use serde::{Deserialize, Serialize};
use std::{env, path::PathBuf};
use strum::EnumIs;
use tuxs_config_types::size_config::InvalidSizeError;
use utoipa::ToSchema;
mod max_upload;
mod security;
pub use max_upload::*;
pub use security::*;
pub const CONFIG_PREFIX: &str = "PKGLY";
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid size: {0}")]
    InvalidSize(#[from] InvalidSizeError),
    #[error(
        "Invalid max upload size. Expected a valid size or 'unlimited', error: {error}, got: {value}"
    )]
    InvalidMaxUpload {
        error: InvalidSizeError,
        value: String,
    },
}
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, EnumIs, ToSchema)]
pub enum Mode {
    Debug,
    Release,
}

impl Default for Mode {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        return Mode::Debug;
        #[cfg(not(debug_assertions))]
        return Mode::Release;
    }
}
pub fn get_current_directory() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::new())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WebServer {
    pub bind_address: String,
    /// Should OpenAPI routes be enabled.
    pub open_api_routes: bool,
    /// The maximum upload size for the web server.
    pub max_upload: MaxUpload,
    /// The TLS configuration for the web server.
    pub tls: Option<TlsConfig>,
    /// Number of Tokio worker threads for the HTTP server. None -> use CPU cores.
    pub worker_threads: Option<usize>,
}
impl Default for WebServer {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:6742".to_owned(),
            open_api_routes: true,
            max_upload: Default::default(),
            tls: None,
            worker_threads: None,
        }
    }
}
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct SiteSetting {
    /// If not set, the app will load the url from the request.
    pub app_url: Option<String>,
    pub name: String,
    pub description: String,
    pub is_https: bool,
    #[cfg(feature = "frontend")]
    pub frontend_path: Option<PathBuf>,
}

impl Default for SiteSetting {
    fn default() -> Self {
        SiteSetting {
            app_url: None,
            name: "Pkgly".to_string(),
            description: "An Open Source artifact manager.".to_string(),
            is_https: false,
            #[cfg(feature = "frontend")]
            frontend_path: None,
        }
    }
}

macro_rules! env_or_file_or_default {
    (
        $config:ident,
        $env:ident,
        $key:ident
    ) => {
        $config.$key.or($env.$key).unwrap_or_default()
    };
    ( $config:ident, $env:ident, $($key:ident),* ) => {
        (
            $(
                env_or_file_or_default!($config, $env, $key),
            )*
        )
    }
}
macro_rules! env_or_file_or_none {
    (
        $config:ident,
        $env:ident,
        $key:ident
    ) => {
        $config.$key.or($env.$key)
    };
    ( $config:ident, $env:ident, $($key:ident),* ) => {
        (
            $(
                env_or_file_or_none!($config, $env, $key),
            )*
        )
    }
}
pub(crate) use env_or_file_or_default;
pub(crate) use env_or_file_or_none;
