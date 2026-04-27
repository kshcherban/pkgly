pub mod config;
pub mod scheduler;
pub mod status;

use chrono::{DateTime, Utc};
use nr_core::database::entities::package_file::DBPackageFile;
use nr_core::database::entities::repository::DBRepositoryConfig;
use nr_core::repository::config::RepositoryConfigType;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    app::{Pkgly, webhooks::PackageWebhookActor},
    error::InternalError,
    repository::{
        DynRepository, Repository,
        retention::{
            config::{PackageRetentionConfig, PackageRetentionConfigType},
            status::{
                PackageRetentionLockOutcome, mark_package_retention_failed,
                mark_package_retention_succeeded, try_mark_package_retention_started,
            },
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageRetentionRunSummary {
    pub candidates: usize,
    pub deleted: usize,
}

pub fn is_virtual_repository(repository: &DynRepository) -> bool {
    matches!(
        repository,
        DynRepository::NPM(crate::repository::npm::NPMRegistry::Virtual(_))
            | DynRepository::Python(crate::repository::python::PythonRepository::Virtual(_))
            | DynRepository::Nuget(crate::repository::nuget::NugetRepository::Virtual(_))
    )
}

pub async fn get_package_retention_config(
    site: &Pkgly,
    repository_id: Uuid,
) -> Result<PackageRetentionConfig, sqlx::Error> {
    DBRepositoryConfig::<PackageRetentionConfig>::get_config(
        repository_id,
        PackageRetentionConfigType::get_type_static(),
        &site.database,
    )
    .await
    .map(|row| row.map(|cfg| cfg.value.0).unwrap_or_default())
}

pub fn package_retention_actor() -> PackageWebhookActor {
    PackageWebhookActor {
        user_id: None,
        username: Some("package-retention".to_string()),
    }
}

#[instrument(skip(site, now))]
pub async fn run_package_retention_for_repository(
    site: &Pkgly,
    repository_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<PackageRetentionRunSummary>, InternalError> {
    let Some(repository) = site.get_repository(repository_id) else {
        return Ok(None);
    };
    if is_virtual_repository(&repository) {
        return Ok(None);
    }

    let config = get_package_retention_config(site, repository_id).await?;
    if !config.enabled {
        return Ok(Some(PackageRetentionRunSummary {
            candidates: 0,
            deleted: 0,
        }));
    }

    let lock = match try_mark_package_retention_started(&site.database, repository_id).await? {
        PackageRetentionLockOutcome::Acquired(lock) => lock,
        PackageRetentionLockOutcome::AlreadyRunning => return Ok(None),
    };

    let run_result = run_locked_package_retention(site, repository, config, now).await;
    let status_result = match &run_result {
        Ok(summary) => {
            mark_package_retention_succeeded(&site.database, repository_id, summary.deleted).await
        }
        Err(err) => {
            mark_package_retention_failed(&site.database, repository_id, &err.to_string()).await
        }
    };
    let release_result = lock.release().await;
    status_result?;
    release_result?;

    run_result.map(Some)
}

async fn run_locked_package_retention(
    site: &Pkgly,
    repository: DynRepository,
    config: PackageRetentionConfig,
    now: DateTime<Utc>,
) -> Result<PackageRetentionRunSummary, InternalError> {
    let cutoff = now - chrono::Duration::days(config.max_age_days as i64);
    let candidates = DBPackageFile::retention_candidates(
        &site.database,
        repository.id(),
        cutoff.into(),
        config.keep_latest_per_package as i64,
    )
    .await?;
    if candidates.is_empty() {
        return Ok(PackageRetentionRunSummary {
            candidates: 0,
            deleted: 0,
        });
    }

    let paths = candidates
        .iter()
        .map(|candidate| candidate.path.clone())
        .collect::<Vec<_>>();
    let deletion = crate::app::api::repository::packages::delete_cached_package_paths(
        site,
        repository,
        &paths,
        package_retention_actor(),
    )
    .await?;

    Ok(PackageRetentionRunSummary {
        candidates: paths.len(),
        deleted: deletion.deleted,
    })
}

#[cfg(test)]
mod tests;
