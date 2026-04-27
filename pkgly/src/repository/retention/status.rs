use sqlx::{PgPool, Postgres, pool::PoolConnection};
use thiserror::Error;
use tracing::instrument;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PackageRetentionStatusError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl crate::utils::IntoErrorResponse for PackageRetentionStatusError {
    fn into_response_boxed(self: Box<Self>) -> axum::response::Response {
        crate::utils::ResponseBuilder::internal_server_error()
            .body(format!("Package retention status error: {}", self))
    }
}

#[derive(Debug)]
pub enum PackageRetentionLockOutcome {
    Acquired(PackageRetentionAdvisoryLock),
    AlreadyRunning,
}

pub(crate) fn package_retention_advisory_key(repository_id: Uuid) -> i64 {
    let u = repository_id.as_u128();
    let folded = (u as u64) ^ ((u >> 64) as u64);
    (folded ^ 0x706b_6772_6574_6e31_u64) as i64
}

#[derive(Debug)]
pub struct PackageRetentionAdvisoryLock {
    key: i64,
    connection: PoolConnection<Postgres>,
}

impl PackageRetentionAdvisoryLock {
    pub async fn release(mut self) -> Result<(), PackageRetentionStatusError> {
        let unlocked = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
            .bind(self.key)
            .fetch_one(&mut *self.connection)
            .await;
        match unlocked {
            Ok(false) => {
                tracing::warn!(
                    repository_lock_key = self.key,
                    "Package retention advisory lock was not held when releasing"
                );
                Ok(())
            }
            Ok(true) => Ok(()),
            Err(err) => {
                if let Err(close_err) = self.connection.close().await {
                    tracing::warn!(
                        error = %close_err,
                        "Failed to close connection after advisory unlock error"
                    );
                }
                Err(PackageRetentionStatusError::Database(err))
            }
        }
    }
}

#[instrument(skip(pool))]
pub async fn ensure_package_retention_status_row(
    pool: &PgPool,
    repository_id: Uuid,
) -> Result<(), PackageRetentionStatusError> {
    sqlx::query(
        r#"
        INSERT INTO package_retention_status (repository_id)
        VALUES ($1)
        ON CONFLICT (repository_id) DO NOTHING
        "#,
    )
    .bind(repository_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[instrument(skip(pool))]
pub async fn try_mark_package_retention_started(
    pool: &PgPool,
    repository_id: Uuid,
) -> Result<PackageRetentionLockOutcome, PackageRetentionStatusError> {
    let key = package_retention_advisory_key(repository_id);
    let mut connection = pool.acquire().await?;
    let acquired = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
        .bind(key)
        .fetch_one(&mut *connection)
        .await?;
    if !acquired {
        return Ok(PackageRetentionLockOutcome::AlreadyRunning);
    }

    if let Err(err) = ensure_package_retention_status_row(pool, repository_id).await {
        if let Err(close_err) = connection.close().await {
            tracing::warn!(error = %close_err, "Failed to close advisory lock connection");
        }
        return Err(err);
    }

    if let Err(err) = sqlx::query(
        r#"
        UPDATE package_retention_status
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

    Ok(PackageRetentionLockOutcome::Acquired(
        PackageRetentionAdvisoryLock { key, connection },
    ))
}

#[instrument(skip(pool))]
pub async fn mark_package_retention_succeeded(
    pool: &PgPool,
    repository_id: Uuid,
    deleted_count: usize,
) -> Result<(), PackageRetentionStatusError> {
    ensure_package_retention_status_row(pool, repository_id).await?;
    sqlx::query(
        r#"
        UPDATE package_retention_status
        SET in_progress = FALSE,
            last_finished_at = NOW(),
            last_success_at = NOW(),
            last_error = NULL,
            last_deleted_count = $2,
            updated_at = NOW()
        WHERE repository_id = $1
        "#,
    )
    .bind(repository_id)
    .bind(deleted_count as i32)
    .execute(pool)
    .await?;
    Ok(())
}

#[instrument(skip(pool))]
pub async fn mark_package_retention_failed(
    pool: &PgPool,
    repository_id: Uuid,
    error: &str,
) -> Result<(), PackageRetentionStatusError> {
    ensure_package_retention_status_row(pool, repository_id).await?;
    sqlx::query(
        r#"
        UPDATE package_retention_status
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
