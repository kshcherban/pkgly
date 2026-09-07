// ABOUTME: Defines staging directory configuration for incoming artifacts.
// ABOUTME: Supplies the default location and cleanup interval.
use std::{env, path::PathBuf};

use chrono::Duration;
use serde::{Deserialize, Serialize};
/// Stages are stored locally before being moved to the storage
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StagingConfig {
    pub staging_dir: PathBuf,
    #[serde(with = "nr_core::utils::duration_serde::as_seconds")]
    pub time_till_cleanup: Duration,
}
impl Default for StagingConfig {
    fn default() -> Self {
        Self {
            staging_dir: default_staging_directory(),
            time_till_cleanup: Duration::hours(1),
        }
    }
}

fn default_staging_directory() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::new())
        .join("staging")
}
