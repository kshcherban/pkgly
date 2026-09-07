// ABOUTME: Refreshes cached repository storage usage from configured storage backends.
// ABOUTME: Supports API-triggered refreshes and background scheduler maintenance.
use std::collections::VecDeque;

use chrono::{DateTime, FixedOffset, Utc};
use nr_core::database::entities::repository::{DBRepository, DBRepositoryWithStorageName};
use nr_core::storage::StoragePath;
use nr_storage::{FileType, Storage, StorageFile};
use tracing::warn;
use uuid::Uuid;

use crate::{
    app::Pkgly,
    error::{InternalError, OtherInternalError},
    repository::{DynRepository, Repository},
};

const STORAGE_USAGE_REFRESH_INTERVAL_HOURS: i64 = 1;

#[derive(Debug, Default)]
pub(crate) struct StorageUsageRefreshSummary {
    pub due_repositories: usize,
    pub refreshed: usize,
    pub missing_repository: usize,
    pub failed: usize,
}

enum StorageUsageComputeResult {
    Usage(u64),
    MissingRepository,
}

fn storage_usage_refresh_due(
    now: DateTime<Utc>,
    last_updated_at: Option<DateTime<FixedOffset>>,
) -> bool {
    match last_updated_at {
        Some(last_updated_at) => {
            let elapsed = now.signed_duration_since(last_updated_at.to_utc());
            elapsed.num_hours() >= STORAGE_USAGE_REFRESH_INTERVAL_HOURS
        }
        None => true,
    }
}

async fn compute_repository_storage_usage(
    site: &Pkgly,
    repository_id: Uuid,
) -> Result<StorageUsageComputeResult, nr_storage::StorageError> {
    let Some(repository) = site.get_repository(repository_id) else {
        return Ok(StorageUsageComputeResult::MissingRepository);
    };

    let usage = calculate_repository_storage_usage(&repository).await?;

    // Docker object accounting lives in PostgreSQL but storage is the source of truth. Repair
    // drift opportunistically during the periodic usage refresh instead of scanning on every read.
    if let DynRepository::Docker(docker) = &repository {
        match crate::repository::docker::metadata::reconcile_docker_objects(
            &site.database,
            &docker.get_storage(),
            repository_id,
        )
        .await
        {
            Ok(summary) => {
                if summary.backfilled > 0 || summary.corrected > 0 || summary.removed > 0 {
                    tracing::info!(
                        %repository_id,
                        backfilled = summary.backfilled,
                        corrected = summary.corrected,
                        removed = summary.removed,
                        "Reconciled Docker object accounting"
                    );
                }
            }
            Err(err) => {
                tracing::warn!(
                    %repository_id,
                    %err,
                    "Docker object accounting reconciliation failed"
                );
            }
        }
    }

    Ok(StorageUsageComputeResult::Usage(usage))
}

async fn calculate_repository_storage_usage(
    repository: &DynRepository,
) -> Result<u64, nr_storage::StorageError> {
    let storage = repository.get_storage();
    let repository_id = repository.id();

    if let nr_storage::DynStorage::Local(local) = storage.clone() {
        match local.repository_size_bytes(repository_id).await {
            Ok(size) => return Ok(size),
            Err(err) => {
                warn!(
                    %repository_id,
                    %err,
                    "Fast local storage usage refresh failed; falling back to metadata traversal"
                );
            }
        }
    }

    if let nr_storage::DynStorage::S3(s3) = storage.clone() {
        match s3.repository_size_bytes(repository_id).await {
            Ok(size) => return Ok(size),
            Err(err) => {
                warn!(
                    %repository_id,
                    %err,
                    "Fast S3 storage usage refresh failed; falling back to metadata traversal"
                );
            }
        }
    }

    calculate_repository_storage_usage_fallback(storage, repository_id).await
}

async fn calculate_repository_storage_usage_fallback(
    storage: nr_storage::DynStorage,
    repository_id: Uuid,
) -> Result<u64, nr_storage::StorageError> {
    let root_path = StoragePath::from("/");
    let Some(root_entry) = storage.open_file(repository_id, &root_path).await? else {
        return Ok(0);
    };

    match root_entry {
        StorageFile::File { meta, .. } => Ok(meta.file_type.file_size),
        StorageFile::Directory { files, .. } => {
            use tokio::task::JoinSet;
            const MAX_CONCURRENT_TASKS: usize = 20;

            let mut total = 0u64;
            let mut tasks = JoinSet::new();
            let mut queue: VecDeque<String> = VecDeque::new();

            for entry in &files {
                if let FileType::Directory(_) = entry.file_type() {
                    let mut path = String::from("/");
                    path.push_str(entry.name());
                    path.push('/');
                    queue.push_back(path);
                }
            }

            for entry in &files {
                if let FileType::File(file_meta) = entry.file_type() {
                    if tasks.len() < MAX_CONCURRENT_TASKS {
                        let file_size = file_meta.file_size;
                        tasks.spawn(async move { file_size });
                    } else {
                        while let Some(result) = tasks.join_next().await {
                            total += result.unwrap_or(0);
                        }
                        let file_size = file_meta.file_size;
                        tasks.spawn(async move { file_size });
                    }
                }
            }

            while let Some(path) = queue.pop_front() {
                let storage_path = StoragePath::from(path.as_str());
                if let Ok(Some(entry)) = storage.open_file(repository_id, &storage_path).await {
                    if let StorageFile::Directory { files, .. } = entry {
                        for file_entry in &files {
                            match file_entry.file_type() {
                                FileType::File(file_meta) => {
                                    if tasks.len() < MAX_CONCURRENT_TASKS {
                                        let file_size = file_meta.file_size;
                                        tasks.spawn(async move { file_size });
                                    } else {
                                        while let Some(result) = tasks.join_next().await {
                                            total += result.unwrap_or(0);
                                        }
                                        let file_size = file_meta.file_size;
                                        tasks.spawn(async move { file_size });
                                    }
                                }
                                FileType::Directory(_) => {
                                    let mut next_path = path.clone();
                                    next_path.push_str(file_entry.name());
                                    next_path.push('/');
                                    queue.push_back(next_path);
                                }
                            }
                        }
                    }
                }
            }

            while let Some(result) = tasks.join_next().await {
                total += result.unwrap_or(0);
            }

            Ok(total)
        }
    }
}

pub(crate) fn normalize_cached_usage(value: Option<i64>) -> Option<u64> {
    value.and_then(|raw| u64::try_from(raw).ok())
}

pub(crate) async fn refresh_repository_storage_usage(
    site: &Pkgly,
    repository_id: Uuid,
) -> Result<Option<(u64, DateTime<FixedOffset>)>, InternalError> {
    let usage = match compute_repository_storage_usage(site, repository_id).await {
        Ok(StorageUsageComputeResult::Usage(usage)) => usage,
        Ok(StorageUsageComputeResult::MissingRepository) => return Ok(None),
        Err(err) => {
            warn!(%repository_id, ?err, "Failed to calculate repository storage usage");
            return Err(InternalError::from(OtherInternalError::new(err)));
        }
    };

    let updated_at = DBRepository::update_storage_usage(repository_id, Some(usage), &site.database)
        .await
        .map_err(InternalError::from)?;

    Ok(Some((usage, updated_at)))
}

pub(crate) async fn storage_usage_scheduler_tick(
    site: Pkgly,
    now: DateTime<Utc>,
) -> Result<StorageUsageRefreshSummary, InternalError> {
    let repositories = DBRepositoryWithStorageName::get_all(&site.database)
        .await
        .map_err(InternalError::from)?;
    let mut summary = StorageUsageRefreshSummary::default();

    for repository in repositories {
        if !storage_usage_refresh_due(now, repository.storage_usage_updated_at) {
            continue;
        }

        summary.due_repositories += 1;
        match refresh_repository_storage_usage(&site, repository.id).await {
            Ok(Some(_)) => summary.refreshed += 1,
            Ok(None) => summary.missing_repository += 1,
            Err(err) => {
                summary.failed += 1;
                warn!(repository_id = %repository.id, error = %err, "Storage usage scheduled refresh failed");
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests;
