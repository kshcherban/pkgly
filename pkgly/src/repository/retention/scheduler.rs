use chrono::{DateTime, FixedOffset, Utc};
use sqlx::{FromRow, PgPool};
use tracing::{instrument, warn};
use uuid::Uuid;

use crate::{app::Pkgly, error::InternalError};

use super::config::PackageRetentionConfig;

const PACKAGE_RETENTION_INTERVAL_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageRetentionSchedulerTickSummary {
    pub due_repositories: usize,
    pub started: usize,
    pub skipped_running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub deleted: usize,
}

#[derive(Debug)]
struct ScheduledPackageRetentionRepository {
    repository_id: Uuid,
    last_started_at: Option<DateTime<FixedOffset>>,
}

fn fixed_to_utc(value: DateTime<FixedOffset>) -> DateTime<Utc> {
    value.with_timezone(&Utc)
}

pub(crate) fn is_due(now: DateTime<Utc>, last_started_at: Option<DateTime<FixedOffset>>) -> bool {
    match last_started_at {
        None => true,
        Some(last) => {
            let elapsed = now.signed_duration_since(fixed_to_utc(last));
            elapsed.num_hours() >= PACKAGE_RETENTION_INTERVAL_HOURS
        }
    }
}

#[instrument(skip(pool))]
async fn list_scheduled_package_retention_repositories(
    pool: &PgPool,
) -> Result<Vec<ScheduledPackageRetentionRepository>, InternalError> {
    #[derive(Debug, FromRow)]
    struct Row {
        repository_id: Uuid,
        retention_config: sqlx::types::Json<serde_json::Value>,
        last_started_at: Option<DateTime<FixedOffset>>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT r.id AS repository_id,
               c.value AS retention_config,
               s.last_started_at AS last_started_at
        FROM repositories r
        INNER JOIN repository_configs c
            ON c.repository_id = r.id AND c.key = 'package_retention'
        LEFT JOIN package_retention_status s
            ON s.repository_id = r.id
        WHERE r.active = TRUE
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut scheduled = Vec::new();
    for row in rows {
        let Ok(config) = serde_json::from_value::<PackageRetentionConfig>(row.retention_config.0)
        else {
            continue;
        };
        if !config.enabled {
            continue;
        }
        scheduled.push(ScheduledPackageRetentionRepository {
            repository_id: row.repository_id,
            last_started_at: row.last_started_at,
        });
    }
    Ok(scheduled)
}

#[instrument(skip(site))]
pub async fn package_retention_scheduler_tick(
    site: Pkgly,
    now: DateTime<Utc>,
) -> Result<PackageRetentionSchedulerTickSummary, InternalError> {
    let scheduled = list_scheduled_package_retention_repositories(&site.database).await?;
    let mut summary = PackageRetentionSchedulerTickSummary {
        due_repositories: 0,
        started: 0,
        skipped_running: 0,
        succeeded: 0,
        failed: 0,
        deleted: 0,
    };

    for repo in scheduled {
        if !is_due(now, repo.last_started_at) {
            continue;
        }
        summary.due_repositories += 1;
        summary.started += 1;

        match super::run_package_retention_for_repository(&site, repo.repository_id, now).await {
            Ok(Some(run)) => {
                summary.succeeded += 1;
                summary.deleted += run.deleted;
            }
            Ok(None) => summary.skipped_running += 1,
            Err(err) => {
                summary.failed += 1;
                warn!(repository_id = %repo.repository_id, error = %err, "Package retention scheduled run failed");
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests;
