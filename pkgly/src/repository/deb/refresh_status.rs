use sqlx::{PgPool, Postgres, pool::PoolConnection};
use thiserror::Error;
use tracing::instrument;
use uuid::Uuid;

use super::proxy_refresh::DebProxyRefreshSummary;

#[derive(Debug, Error)]
pub enum DebProxyRefreshStatusError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl crate::utils::IntoErrorResponse for DebProxyRefreshStatusError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        crate::utils::ResponseBuilder::internal_server_error()
            .body(format!("Deb proxy refresh status error: {}", self))
    }
}

#[derive(Debug)]
pub enum DebProxyRefreshLockOutcome {
    Acquired(DebProxyRefreshAdvisoryLock),
    AlreadyRunning,
}

pub(crate) fn deb_proxy_refresh_advisory_key(repository_id: Uuid) -> i64 {
    let u = repository_id.as_u128();
    let folded = (u as u64) ^ ((u >> 64) as u64);
    folded as i64
}

#[derive(Debug)]
pub struct DebProxyRefreshAdvisoryLock {
    key: i64,
    connection: PoolConnection<Postgres>,
}

impl DebProxyRefreshAdvisoryLock {
    pub async fn release(mut self) -> Result<(), DebProxyRefreshStatusError> {
        let unlocked = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
            .bind(self.key)
            .fetch_one(&mut *self.connection)
            .await;
        match unlocked {
            Ok(false) => {
                tracing::warn!(
                    repository_lock_key = self.key,
                    "Deb proxy refresh advisory lock was not held when releasing"
                );
                Ok(())
            }
            Ok(true) => Ok(()),
            Err(err) => {
                // Ensure the lock is released by force-closing the connection.
                if let Err(close_err) = self.connection.close().await {
                    tracing::warn!(
                        error = %close_err,
                        "Failed to close connection after advisory unlock error"
                    );
                }
                Err(DebProxyRefreshStatusError::Database(err))
            }
        }
    }
}

/// Ensure a status row exists for the repository.
#[instrument(skip(pool))]
pub async fn ensure_deb_proxy_refresh_status_row(
    pool: &PgPool,
    repository_id: Uuid,
) -> Result<(), DebProxyRefreshStatusError> {
    sqlx::query(
        r#"
        INSERT INTO deb_proxy_refresh_status (repository_id)
        VALUES ($1)
        ON CONFLICT (repository_id) DO NOTHING
        "#,
    )
    .bind(repository_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Attempt to start a refresh, returning `AlreadyRunning` if a refresh is in progress.
#[instrument(skip(pool))]
pub async fn try_mark_deb_proxy_refresh_started(
    pool: &PgPool,
    repository_id: Uuid,
) -> Result<DebProxyRefreshLockOutcome, DebProxyRefreshStatusError> {
    let key = deb_proxy_refresh_advisory_key(repository_id);
    let mut connection = pool.acquire().await?;
    let acquired = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
        .bind(key)
        .fetch_one(&mut *connection)
        .await?;
    if !acquired {
        return Ok(DebProxyRefreshLockOutcome::AlreadyRunning);
    }

    if let Err(err) = ensure_deb_proxy_refresh_status_row(pool, repository_id).await {
        if let Err(close_err) = connection.close().await {
            tracing::warn!(error = %close_err, "Failed to close advisory lock connection");
        }
        return Err(err);
    }

    if let Err(err) = sqlx::query(
        r#"
        UPDATE deb_proxy_refresh_status
        SET in_progress = TRUE,
            last_started_at = NOW(),
            last_error = NULL,
            updated_at = NOW()
        WHERE repository_id = $1
        "#,
    )
    .bind(repository_id)
    .execute(pool)
    .await
    {
        if let Err(close_err) = connection.close().await {
            tracing::warn!(error = %close_err, "Failed to close advisory lock connection");
        }
        return Err(err.into());
    }

    Ok(DebProxyRefreshLockOutcome::Acquired(
        DebProxyRefreshAdvisoryLock { key, connection },
    ))
}

#[instrument(skip(pool, summary))]
pub async fn mark_deb_proxy_refresh_succeeded(
    pool: &PgPool,
    repository_id: Uuid,
    summary: DebProxyRefreshSummary,
) -> Result<(), DebProxyRefreshStatusError> {
    ensure_deb_proxy_refresh_status_row(pool, repository_id).await?;
    sqlx::query(
        r#"
        UPDATE deb_proxy_refresh_status
        SET in_progress = FALSE,
            last_finished_at = NOW(),
            last_success_at = NOW(),
            last_error = NULL,
            last_downloaded_packages = $2,
            last_downloaded_files = $3,
            updated_at = NOW()
        WHERE repository_id = $1
        "#,
    )
    .bind(repository_id)
    .bind(summary.downloaded_packages as i32)
    .bind(summary.downloaded_files as i32)
    .execute(pool)
    .await?;
    Ok(())
}

#[instrument(skip(pool))]
pub async fn mark_deb_proxy_refresh_failed(
    pool: &PgPool,
    repository_id: Uuid,
    error: &str,
) -> Result<(), DebProxyRefreshStatusError> {
    ensure_deb_proxy_refresh_status_row(pool, repository_id).await?;
    sqlx::query(
        r#"
        UPDATE deb_proxy_refresh_status
        SET in_progress = FALSE,
            last_finished_at = NOW(),
            last_error = $2,
            updated_at = NOW()
        WHERE repository_id = $1
        "#,
    )
    .bind(repository_id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
