use nr_core::storage::StoragePath;
use uuid::Uuid;

use crate::{
    FileContent, FileType, Storage, StorageError, StorageFactory, StorageTypeConfig,
    local::{LocalStorage, LocalStorageFactory},
    meta::RepositoryMeta,
    s3::{S3Storage, S3StorageFactory},
    streaming::DynDirectoryListStream,
};
#[derive(Debug, Clone)]
pub enum DynStorage {
    Local(LocalStorage),
    S3(S3Storage),
}
impl Storage for DynStorage {
    type Error = StorageError;
    type DirectoryStream = DynDirectoryListStream;
    async fn unload(&self) -> Result<(), StorageError> {
        match self {
            DynStorage::Local(storage) => storage.unload().await.map_err(Into::into),
            DynStorage::S3(storage) => storage.unload().await.map_err(Into::into),
        }
    }
    fn storage_type_name(&self) -> &'static str {
        match self {
            DynStorage::Local(storage) => storage.storage_type_name(),
            DynStorage::S3(storage) => storage.storage_type_name(),
        }
    }

    fn storage_config(&self) -> crate::BorrowedStorageConfig<'_> {
        match self {
            DynStorage::Local(storage) => storage.storage_config(),
            DynStorage::S3(storage) => storage.storage_config(),
        }
    }

    async fn save_file(
        &self,
        repository: Uuid,
        file: FileContent,
        location: &StoragePath,
    ) -> Result<(usize, bool), StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .save_file(repository, file, location)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .save_file(repository, file, location)
                .await
                .map_err(Into::into),
        }
    }

    async fn append_file(
        &self,
        repository: Uuid,
        file: FileContent,
        location: &StoragePath,
    ) -> Result<usize, StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .append_file(repository, file, location)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .append_file(repository, file, location)
                .await
                .map_err(Into::into),
        }
    }

    async fn move_file(
        &self,
        repository: Uuid,
        from: &StoragePath,
        to: &StoragePath,
    ) -> Result<bool, StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .move_file(repository, from, to)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .move_file(repository, from, to)
                .await
                .map_err(Into::into),
        }
    }

    async fn delete_file(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<bool, StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .delete_file(repository, location)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .delete_file(repository, location)
                .await
                .map_err(Into::into),
        }
    }

    async fn get_file_information(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<crate::StorageFileMeta<FileType>>, StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .get_file_information(repository, location)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .get_file_information(repository, location)
                .await
                .map_err(Into::into),
        }
    }

    async fn open_file(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<crate::StorageFile>, StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .open_file(repository, location)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .open_file(repository, location)
                .await
                .map_err(Into::into),
        }
    }

    async fn validate_config_change(&self, config: StorageTypeConfig) -> Result<(), StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .validate_config_change(config)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .validate_config_change(config)
                .await
                .map_err(Into::into),
        }
    }
    async fn put_repository_meta(
        &self,
        repository: Uuid,
        location: &StoragePath,
        value: RepositoryMeta,
    ) -> Result<(), StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .put_repository_meta(repository, location, value)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .put_repository_meta(repository, location, value)
                .await
                .map_err(Into::into),
        }
    }
    async fn get_repository_meta(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<RepositoryMeta>, StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .get_repository_meta(repository, location)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .get_repository_meta(repository, location)
                .await
                .map_err(Into::into),
        }
    }
    async fn file_exists(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<bool, StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .file_exists(repository, location)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .file_exists(repository, location)
                .await
                .map_err(Into::into),
        }
    }

    async fn delete_repository(&self, repository: Uuid) -> Result<(), StorageError> {
        match self {
            DynStorage::Local(storage) => storage
                .delete_repository(repository)
                .await
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .delete_repository(repository)
                .await
                .map_err(Into::into),
        }
    }

    async fn stream_directory(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<Self::DirectoryStream>, Self::Error> {
        match self {
            DynStorage::Local(storage) => storage
                .stream_directory(repository, location)
                .await
                .map(|x| x.map(DynDirectoryListStream::new))
                .map_err(Into::into),
            DynStorage::S3(storage) => storage
                .stream_directory(repository, location)
                .await
                .map(|x| x.map(DynDirectoryListStream::new))
                .map_err(Into::into),
        }
    }
}

impl DynStorage {
    /// Delete multiple files in batch if supported by the storage backend.
    /// For S3, this uses the efficient delete_objects API.
    /// For local storage, falls back to sequential deletion.
    ///
    /// Returns the number of files actually deleted.
    #[tracing::instrument(name = "DynStorage::delete_files_batch", skip(self, paths), fields(count = paths.len()))]
    pub async fn delete_files_batch(
        &self,
        repository: Uuid,
        paths: &[StoragePath],
    ) -> Result<usize, StorageError> {
        match self {
            DynStorage::Local(_storage) => {
                // Local storage doesn't have batch delete, fall back to sequential
                let mut deleted = 0;
                for path in paths {
                    if self.delete_file(repository, path).await? {
                        deleted += 1;
                    }
                }
                Ok(deleted)
            }
            DynStorage::S3(storage) => storage
                .delete_files_batch(repository, paths)
                .await
                .map_err(Into::into),
        }
    }
}

pub static STORAGE_FACTORIES: &[&dyn StorageFactory] = &[&LocalStorageFactory, &S3StorageFactory];
