#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::{
    StaticStorageFactory, StorageConfig, StorageConfigInner, StorageTypeConfig,
    fs::FileContent,
    local::{LocalConfig, LocalStorageFactory, LocalStorageInner},
    testing::storage::TestingStorage,
};
use fs2::FileExt;
use nr_core::storage::StoragePath;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};
use tracing::warn;
use uuid::Uuid;

#[tokio::test]
pub async fn generic_test() -> anyhow::Result<()> {
    let Some(config) = crate::testing::start_storage_test("Local")? else {
        warn!("Local Storage Test Skipped");
        return Ok(());
    };
    let local_storage =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(config).await?;
    let testing_storage = TestingStorage::new(local_storage);
    crate::testing::tests::full_test(testing_storage).await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn save_file_waits_for_lock_without_truncating() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let storage =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(StorageConfig {
            storage_config: StorageConfigInner::test_config(),
            type_config: StorageTypeConfig::Local(LocalConfig {
                path: temp.path().to_path_buf(),
            }),
        })
        .await?;

    let repository = Uuid::new_v4();
    let location = StoragePath::from("locks/save.bin");
    let file_path = storage.get_path(&repository, &location);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&file_path, b"original")?;

    let guard_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&file_path)?;
    guard_file.lock_exclusive()?;

    let storage_clone = storage.clone();
    let location_clone = location.clone();
    let handle = tokio::spawn(async move {
        storage_clone
            .save_file(
                repository,
                FileContent::from(b"replacement".as_slice()),
                &location_clone,
            )
            .await
            .unwrap();
    });

    sleep(Duration::from_millis(50)).await;
    assert!(
        !handle.is_finished(),
        "save_file should wait for the OS lock to release"
    );

    // If save_file truncates the file before taking the lock, the content will already be gone.
    let content_during_lock = std::fs::read(&file_path)?;
    assert_eq!(
        content_during_lock, b"original",
        "save_file must not truncate an in-use file before acquiring the lock"
    );

    guard_file.unlock()?;
    drop(guard_file);
    tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .expect("save_file should complete once the lock is released")
        .unwrap();

    let content_after = std::fs::read(&file_path)?;
    assert_eq!(
        content_after, b"replacement",
        "save_file should replace the content once it acquires the lock"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn append_waits_for_lock_release() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let storage =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(StorageConfig {
            storage_config: StorageConfigInner::test_config(),
            type_config: StorageTypeConfig::Local(LocalConfig {
                path: temp.path().to_path_buf(),
            }),
        })
        .await?;

    let repository = Uuid::new_v4();
    let location = StoragePath::from("locks/blob.bin");
    let file_path = storage.get_path(&repository, &location);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let guard_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)?;
    guard_file.lock_exclusive()?;

    let storage_clone = storage.clone();
    let location_clone = location.clone();
    let handle = tokio::spawn(async move {
        storage_clone
            .append_file(
                repository,
                FileContent::from(vec![1u8; 16]),
                &location_clone,
            )
            .await
            .unwrap();
    });

    sleep(Duration::from_millis(50)).await;
    assert!(
        !handle.is_finished(),
        "append_file should wait for the OS lock to release"
    );

    guard_file.unlock()?;
    drop(guard_file);
    tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .expect("append_file should complete once the lock is released")
        .unwrap();
    Ok(())
}

#[tokio::test]
async fn metadata_updates_wait_instead_of_dropping() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel();

    let inner = LocalStorageInner {
        config: LocalConfig {
            path: temp.path().to_path_buf(),
        },
        storage_config: StorageConfigInner::test_config(),
        shutdown_signal: Mutex::new(Some(shutdown_tx)),
        meta_update_sender: sender,
    };

    let target = temp.path().join("repo/pkg/file.bin");
    let parent = target.parent().unwrap().to_path_buf();

    let receiver_task = tokio::spawn(async move {
        sleep(Duration::from_millis(100)).await;
        let mut seen = Vec::new();
        while let Some(path) = receiver.recv().await {
            seen.push(path);
            if seen.len() >= 2 {
                break;
            }
        }
        seen
    });

    let updated = inner
        .update_meta_and_parent_metas(&target, None)
        .await
        .expect("metadata updates should succeed");

    assert_eq!(updated, 2, "both file and parent updates must be queued");

    let seen = receiver_task.await.unwrap();
    assert!(seen.contains(&target));
    assert!(seen.contains(&parent));

    Ok(())
}

#[tokio::test]
async fn repository_size_bytes_counts_regular_files() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let storage =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(StorageConfig {
            storage_config: StorageConfigInner::test_config(),
            type_config: StorageTypeConfig::Local(LocalConfig {
                path: temp.path().to_path_buf(),
            }),
        })
        .await?;

    let repository = Uuid::new_v4();
    let repo_root = temp.path().join(repository.to_string());
    let nested = repo_root.join("nested");
    std::fs::create_dir_all(&nested)?;
    std::fs::write(repo_root.join("alpha.bin"), vec![0u8; 10])?;
    std::fs::write(nested.join("beta.bin"), vec![0u8; 25])?;
    std::fs::write(repo_root.join(".nr-meta"), vec![0u8; 100])?;
    std::fs::write(nested.join("beta.bin.nr-meta"), vec![0u8; 100])?;

    let size = storage.repository_size_bytes(repository).await?;

    assert_eq!(
        size, 35,
        "only regular files should be counted, meta files are ignored"
    );

    Ok(())
}

#[tokio::test]
async fn repository_size_bytes_missing_repo_returns_zero() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let storage =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(StorageConfig {
            storage_config: StorageConfigInner::test_config(),
            type_config: StorageTypeConfig::Local(LocalConfig {
                path: temp.path().to_path_buf(),
            }),
        })
        .await?;

    let repository = Uuid::new_v4();
    let size = storage.repository_size_bytes(repository).await?;
    assert_eq!(size, 0);

    Ok(())
}

#[tokio::test]
async fn delete_repository_removes_all_files() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let storage =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(StorageConfig {
            storage_config: StorageConfigInner::test_config(),
            type_config: StorageTypeConfig::Local(LocalConfig {
                path: temp.path().to_path_buf(),
            }),
        })
        .await?;

    let repository = Uuid::new_v4();
    let paths = [
        StoragePath::from("packages/file.bin"),
        StoragePath::from("packages/nested/inner.txt"),
    ];

    for path in &paths {
        storage
            .save_file(repository, FileContent::from(b"payload".as_slice()), path)
            .await?;
        assert!(storage.file_exists(repository, path).await?);
    }

    let repository_root = temp.path().join(repository.to_string());
    assert!(
        repository_root.exists(),
        "repository root must exist before deletion"
    );

    storage.delete_repository(repository).await?;

    assert!(
        !repository_root.exists(),
        "repository directory should be removed"
    );
    for path in &paths {
        assert!(!storage.file_exists(repository, path).await?);
    }

    Ok(())
}

#[tokio::test]
async fn delete_repository_is_idempotent_for_missing_repo() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let storage =
        <LocalStorageFactory as StaticStorageFactory>::create_storage_from_config(StorageConfig {
            storage_config: StorageConfigInner::test_config(),
            type_config: StorageTypeConfig::Local(LocalConfig {
                path: temp.path().to_path_buf(),
            }),
        })
        .await?;

    let repository_without_files = Uuid::new_v4();
    storage.delete_repository(repository_without_files).await?;

    let preserved_repository = Uuid::new_v4();
    let path = StoragePath::from("keep/me.txt");
    storage
        .save_file(
            preserved_repository,
            FileContent::from(b"keep".as_slice()),
            &path,
        )
        .await?;

    storage.delete_repository(repository_without_files).await?;

    assert!(storage.file_exists(preserved_repository, &path).await?);

    Ok(())
}

#[tokio::test]
async fn directory_entries_are_relative_and_sorted() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let root = temp.path();
    std::fs::create_dir_all(root.join("alpha/beta"))?;
    std::fs::write(root.join("alpha/file.txt"), b"data")?;
    std::fs::write(root.join("root.bin"), b"payload")?;

    let mut entries = super::LocalStorage::directory_entries(root).await?;

    // ensure deterministic ordering for assertions
    entries.sort();

    let expected = vec![
        std::path::PathBuf::from("alpha"),
        std::path::PathBuf::from("alpha/beta"),
        std::path::PathBuf::from("alpha/file.txt"),
        std::path::PathBuf::from("root.bin"),
    ];
    assert_eq!(entries, expected);

    Ok(())
}
