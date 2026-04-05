use chrono::{DateTime, FixedOffset, Utc};
use sqlx::{FromRow, PgPool};
use std::str::FromStr;
use tracing::{instrument, warn};
use uuid::Uuid;

use crate::{
    app::Pkgly,
    error::InternalError,
    repository::{DynRepository, deb::DebRepository},
};

use super::{
    DebProxyRepository,
    configs::{DebProxyRefreshSchedule, DebRepositoryConfig, normalize_cron_expression},
    refresh_status::{
        DebProxyRefreshLockOutcome, mark_deb_proxy_refresh_failed,
        mark_deb_proxy_refresh_succeeded, try_mark_deb_proxy_refresh_started,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebProxySchedulerTickSummary {
    pub due_repositories: usize,
    pub started: usize,
    pub skipped_running: usize,
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug)]
struct ScheduledDebProxyRepository {
    repository_id: Uuid,
    schedule: DebProxyRefreshSchedule,
    last_started_at: Option<DateTime<FixedOffset>>,
}

fn fixed_to_utc(value: DateTime<FixedOffset>) -> DateTime<Utc> {
    value.with_timezone(&Utc)
}

pub(crate) fn is_due(
    now: DateTime<Utc>,
    schedule: &DebProxyRefreshSchedule,
    last_started_at: Option<DateTime<FixedOffset>>,
) -> bool {
    match schedule {
        DebProxyRefreshSchedule::IntervalSeconds(interval) => match last_started_at {
            None => true,
            Some(last) => {
                let elapsed = now.signed_duration_since(fixed_to_utc(last));
                elapsed.num_seconds() >= interval.interval_seconds as i64
            }
        },
        DebProxyRefreshSchedule::Cron(cron) => {
            let normalized = match normalize_cron_expression(&cron.expression) {
                Ok(value) => value,
                Err(_) => return false,
            };
            let schedule = match cron::Schedule::from_str(&normalized) {
                Ok(schedule) => schedule,
                Err(_) => return false,
            };
            let anchor = match last_started_at {
                Some(last) => fixed_to_utc(last),
                None => now - chrono::Duration::seconds(60),
            };
            let Some(next) = schedule.after(&anchor).next() else {
                return false;
            };
            next <= now
        }
    }
}

pub fn next_run_at(
    now: DateTime<Utc>,
    schedule: &DebProxyRefreshSchedule,
    last_started_at: Option<DateTime<FixedOffset>>,
) -> Option<DateTime<Utc>> {
    match schedule {
        DebProxyRefreshSchedule::IntervalSeconds(interval) => match last_started_at {
            Some(last) => Some(
                fixed_to_utc(last) + chrono::Duration::seconds(interval.interval_seconds as i64),
            ),
            None => Some(now),
        },
        DebProxyRefreshSchedule::Cron(cron) => {
            let normalized = normalize_cron_expression(&cron.expression).ok()?;
            let schedule = cron::Schedule::from_str(&normalized).ok()?;
            schedule.after(&now).next()
        }
    }
}

#[instrument(skip(pool))]
async fn list_scheduled_deb_proxy_repositories(
    pool: &PgPool,
) -> Result<Vec<ScheduledDebProxyRepository>, InternalError> {
    #[derive(Debug, FromRow)]
    struct Row {
        repository_id: Uuid,
        deb_config: sqlx::types::Json<serde_json::Value>,
        last_started_at: Option<DateTime<FixedOffset>>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT r.id as repository_id,
               c.value as deb_config,
               s.last_started_at as last_started_at
        FROM repositories r
        INNER JOIN repository_configs c
            ON c.repository_id = r.id AND c.key = 'deb'
        LEFT JOIN deb_proxy_refresh_status s
            ON s.repository_id = r.id
        WHERE r.repository_type = 'deb' AND r.active = TRUE
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(InternalError::from)?;

    let mut scheduled = Vec::new();
    for row in rows {
        let config: serde_json::Value = row.deb_config.0;
        let Ok(config) = serde_json::from_value::<DebRepositoryConfig>(config) else {
            continue;
        };
        let DebRepositoryConfig::Proxy(proxy) = config else {
            continue;
        };
        let Some(refresh) = proxy.refresh.as_ref() else {
            continue;
        };
        if !refresh.enabled {
            continue;
        }
        scheduled.push(ScheduledDebProxyRepository {
            repository_id: row.repository_id,
            schedule: refresh.schedule.clone(),
            last_started_at: row.last_started_at,
        });
    }
    Ok(scheduled)
}

fn get_deb_proxy_repository(site: &Pkgly, repository_id: Uuid) -> Option<DebProxyRepository> {
    match site.get_repository(repository_id) {
        Some(DynRepository::Deb(DebRepository::Proxy(proxy))) => Some(proxy),
        _ => None,
    }
}

#[instrument(skip(site))]
async fn run_deb_proxy_scheduled_refresh(
    site: &Pkgly,
    repository_id: Uuid,
    proxy: DebProxyRepository,
) -> Result<bool, InternalError> {
    let lock = match try_mark_deb_proxy_refresh_started(&site.database, repository_id)
        .await
        .map_err(InternalError::from)?
    {
        DebProxyRefreshLockOutcome::Acquired(lock) => lock,
        DebProxyRefreshLockOutcome::AlreadyRunning => return Ok(false),
    };

    let refresh_result = proxy.refresh_offline_mirror().await;
    match refresh_result {
        Ok(summary) => {
            if let Err(err) =
                mark_deb_proxy_refresh_succeeded(&site.database, repository_id, summary).await
            {
                let _ = lock.release().await;
                return Err(err.into());
            }
        }
        Err(err) => {
            if let Err(status_err) =
                mark_deb_proxy_refresh_failed(&site.database, repository_id, &err.to_string()).await
            {
                let _ = lock.release().await;
                return Err(status_err.into());
            }
        }
    }

    lock.release().await.map_err(InternalError::from)?;
    Ok(true)
}

#[instrument(skip(site))]
pub async fn deb_proxy_scheduler_tick(
    site: Pkgly,
    now: DateTime<Utc>,
) -> Result<DebProxySchedulerTickSummary, InternalError> {
    let scheduled = list_scheduled_deb_proxy_repositories(&site.database).await?;
    let mut summary = DebProxySchedulerTickSummary {
        due_repositories: 0,
        started: 0,
        skipped_running: 0,
        succeeded: 0,
        failed: 0,
    };

    for repo in scheduled {
        if !is_due(now, &repo.schedule, repo.last_started_at) {
            continue;
        }
        summary.due_repositories += 1;

        let Some(proxy) = get_deb_proxy_repository(&site, repo.repository_id) else {
            continue;
        };

        summary.started += 1;
        match run_deb_proxy_scheduled_refresh(&site, repo.repository_id, proxy).await {
            Ok(true) => summary.succeeded += 1,
            Ok(false) => summary.skipped_running += 1,
            Err(err) => {
                summary.failed += 1;
                warn!(repository_id = %repo.repository_id, error = %err, "Deb proxy scheduled refresh failed");
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests;
