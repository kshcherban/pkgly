use std::{env, fmt::Debug, path::PathBuf, sync::Arc};

use axum::response::IntoResponse;
use chrono::Duration;
use derive_more::derive::Deref;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StagingManagerError {
    #[error("Database Error")]
    DBError(#[from] sqlx::Error),
    #[error("IO Error")]
    IOError(#[from] std::io::Error),
}
impl IntoResponse for StagingManagerError {
    fn into_response(self) -> axum::response::Response {
        error!("{}", self);
        let message = format!("Staging Manager Error {:?}. ", self);
        crate::utils::ResponseBuilder::internal_server_error().body(message)
    }
}
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
pub struct StagingManagerInner {
    repository: Uuid,
}
#[derive(Deref, Clone)]
pub struct StagingManager(Arc<StagingManagerInner>);

impl Debug for StagingManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagingManager")
            .field("repository_id", &self.repository)
            .finish()
    }
}
