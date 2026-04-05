use serde::{Deserialize, Serialize};
use sqlx::postgres::PgConnectOptions;

use super::DBError;

/// The configuration for the database.
///
/// Currently only supports PostgreSQL.
#[derive(Clone, Deserialize, Serialize, clap::Args)]
#[serde(default)]
pub struct DatabaseConfig {
    /// The username to connect to the database.
    ///
    /// Default is `postgres`.
    /// Environment variable: PKGLY_DATABASE__USER
    #[clap(long = "database-user", default_value = "postgres")]
    pub user: String,
    /// The password to connect to the database.
    ///
    /// Default is `password`.
    /// Environment variable: PKGLY_DATABASE__PASSWORD
    #[clap(long = "database-password", default_value = "password")]
    pub password: String,
    #[clap(long = "database-name", default_value = "pkgly")]
    #[serde(alias = "name")]
    pub database: String,
    // The host can be in the format host:port or just host.
    #[clap(long = "database-host", default_value = "localhost:5432")]
    pub host: String,
    // The port is optional. If not specified the default port is used. or will be extracted from the host.
    #[clap(long = "database-port")]
    pub port: Option<u16>,
}

impl std::fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("user", &self.user)
            .field("password", &"********") // Mask password
            .field("database", &self.database)
            .field("host", &self.host)
            .field("port", &self.port)
            .finish()
    }
}

impl DatabaseConfig {
    /// Returns the host and port
    ///
    /// If it is not specified in the port field it will attempt to extract it from the host field.
    pub fn host_name_port(&self) -> Result<(&str, u16), DBError> {
        if let Some(port) = self.port {
            Ok((self.host.as_str(), port))
        } else {
            // The port can be specified in the host field. If it is, we need to extract it.
            let host = self.host.split(':').collect::<Vec<&str>>();

            match host.len() {
                // The port is not specified. Use the default port.
                1 => Ok((host[0], 5432)),
                // The port is specified within the host. The port option is ignored.
                2 => Ok((host[0], host[1].parse::<u16>().unwrap_or(5432))),
                _ => {
                    // Not in the format host:port. Possibly IPv6 but we don't support that.
                    // If it is IPv6 please specify the port separately.
                    Err(DBError::InvalidHost(self.host.clone()))
                }
            }
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            user: "postgres".to_string(),
            password: "password".to_string(),
            database: "pkgly".to_string(),
            host: "localhost".to_string(),
            port: Some(5432),
        }
    }
}
impl TryFrom<DatabaseConfig> for PgConnectOptions {
    type Error = DBError;
    fn try_from(settings: DatabaseConfig) -> Result<PgConnectOptions, Self::Error> {
        let (host, port) = settings.host_name_port()?;
        let options = PgConnectOptions::new()
            .username(&settings.user)
            .password(&settings.password)
            .host(host)
            .port(port)
            .database(&settings.database);

        Ok(options)
    }
}

#[cfg(test)]
mod tests;
