use std::{
    fs::{self},
    io::{self, ErrorKind, Seek, SeekFrom},
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use dashmap::DashMap;

pub use stream::*;
pub mod error;
mod stream;
use error::LocalStorageError;
use nr_core::storage::StoragePath;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::Mutex,
    task::{JoinSet, spawn_blocking},
    time::sleep,
};
use tracing::{
    Instrument as _, Level, Span, debug, debug_span, error, event,
    field::{Empty, debug},
    info, info_span, instrument, trace, warn,
};
use utils::new_type_arc_type;
use walkdir::WalkDir;

use crate::*;

use ahash::RandomState;
use fs2::FileExt;
use nr_core::storage::FileHashes;
use std::sync::OnceLock;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConfig {
    pub path: PathBuf,
}
impl utoipa::__dev::ComposeSchema for LocalConfig {
    fn compose(
        _generics: Vec<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>,
    ) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .property(
                "path",
                utoipa::openapi::ObjectBuilder::new().schema_type(
                    utoipa::openapi::schema::SchemaType::new(utoipa::openapi::schema::Type::String),
                ),
            )
            .required("path")
            .into()
    }
}
impl utoipa::ToSchema for LocalConfig {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("LocalConfig")
    }
}
impl utoipa::__dev::SchemaReferences for LocalConfig {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        schemas.extend([]);
    }
}
fn meta_update_task(
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
    mut receiver: tokio::sync::mpsc::Receiver<PathBuf>,
) {
    tokio::task::spawn(async move {
        const MAX_CONCURRENT_UPDATES: usize = 20;
        let mut update_tasks = JoinSet::new();

        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    break;
                }
                path = receiver.recv(), if update_tasks.len() < MAX_CONCURRENT_UPDATES => {
                    let Some(path) = path else {
                        break;
                    };

                    if !path.exists() {
                        warn!(?path, "Path does not exist");
                        continue;
                    }

                    let precomputed = take_precomputed_hash(&path);
                    update_tasks.spawn(async move {
                        let span = info_span!(
                            "Meta Update Task",
                            path = debug(&path),
                            otel.status_code = Empty,
                            otel.exception = Empty,
                        );

                        let result = spawn_blocking({
                            let span = span.clone();
                            let path = path.clone();
                            move || {
                                span.in_scope(|| {
                                    LocationMeta::create_meta_or_update(&path, precomputed.as_ref())
                                })
                            }
                        })
                        .await;

                        match result {
                            Ok(Ok(_)) => {
                                span.record("otel.status_code", "OK");
                                debug!("Updated Meta");
                            }
                            Ok(Err(err)) => {
                                span.record("otel.status_code", "ERROR");
                                event!(Level::ERROR, ?err, "Error Updating Meta");
                            }
                            Err(err) => {
                                span.record("otel.status_code", "ERROR");
                                event!(Level::ERROR, ?err, "Metadata update task panicked");
                            }
                        }
                    });
                }
                Some(result) = update_tasks.join_next() => {
                    if let Err(err) = result {
                        event!(Level::ERROR, ?err, "Metadata update task failed");
                    }
                }
            }
        }

        // Wait for remaining tasks to complete
        while let Some(result) = update_tasks.join_next().await {
            if let Err(err) = result {
                event!(
                    Level::ERROR,
                    ?err,
                    "Metadata update task failed during shutdown"
                );
            }
        }

        receiver.close();
    });
}

#[derive(Debug)]
pub struct LocalStorageInner {
    pub config: LocalConfig,
    pub storage_config: StorageConfigInner,
    pub shutdown_signal: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    pub meta_update_sender: tokio::sync::mpsc::Sender<PathBuf>,
}
impl LocalStorageInner {}

fn precomputed_hashes() -> &'static DashMap<PathBuf, FileHashes, RandomState> {
    static PRECOMPUTED: OnceLock<DashMap<PathBuf, FileHashes, RandomState>> = OnceLock::new();
    PRECOMPUTED.get_or_init(|| DashMap::with_hasher(RandomState::default()))
}

fn store_precomputed_hash(path: PathBuf, hashes: FileHashes) {
    precomputed_hashes().insert(path, hashes);
}

fn take_precomputed_hash(path: &Path) -> Option<FileHashes> {
    precomputed_hashes().remove(path).map(|(_, hashes)| hashes)
}
#[derive(Debug, Clone)]
pub struct LocalStorage(Arc<LocalStorageInner>);
new_type_arc_type!(LocalStorage(LocalStorageInner));

impl LocalStorage {
    pub fn register_precomputed_hash(
        &self,
        repository: Uuid,
        location: &StoragePath,
        hashes: FileHashes,
    ) {
        let path = self.get_path(&repository, location);
        store_precomputed_hash(path, hashes);
    }

    pub async fn open_append_handle(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<(tokio::fs::File, PathBuf), LocalStorageError> {
        let storage = self.clone();
        let location_clone = location.clone();
        let (std_file, path) = tokio::task::spawn_blocking(move || {
            let CreatePath {
                path,
                parent_directory,
                new_directory_start,
            } = storage
                .0
                .get_path_for_creation(repository, &location_clone)?;

            if new_directory_start.is_some() {
                std::fs::create_dir_all(parent_directory)?;
            }

            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            file.lock_exclusive()?;

            Ok::<_, LocalStorageError>((file, path))
        })
        .await
        .map_err(|err| LocalStorageError::IOError(std::io::Error::other(err)))??;

        Ok((tokio::fs::File::from_std(std_file), path))
    }

    #[instrument(
        level = "debug",
        skip(self),
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            repository = %repository,
        )
    )]
    pub async fn repository_size_bytes(&self, repository: Uuid) -> Result<u64, LocalStorageError> {
        let root_path = self.get_path(&repository, &StoragePath::from("/"));
        if !root_path.exists() {
            return Ok(0);
        }

        let path = root_path.clone();
        spawn_blocking(move || -> Result<u64, LocalStorageError> {
            let mut total = 0u64;
            for entry in WalkDir::new(&path).follow_links(false) {
                let entry = entry.map_err(LocalStorageError::other)?;
                if !entry.file_type().is_file() {
                    continue;
                }
                let file_path = entry.path();
                if is_hidden_file(file_path) {
                    continue;
                }
                let metadata = entry.metadata().map_err(LocalStorageError::other)?;
                total += metadata.len();
            }
            Ok(total)
        })
        .await
        .map_err(LocalStorageError::other)?
    }
}
struct CreatePath {
    path: PathBuf,
    parent_directory: PathBuf,
    /// The point at which the new directory starts
    ///
    /// If None, then the directory already exists
    new_directory_start: Option<PathBuf>,
}

impl LocalStorageInner {
    /// Get the path for a file to be created
    #[instrument(level = "debug")]
    fn get_path_for_creation(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<CreatePath, LocalStorageError> {
        let mut path = self.config.path.join(repository.to_string());
        let mut parent_directory = path.clone();
        let mut new_directory_start = None;
        let mut conflicting_path = StoragePath::default();
        let mut iter = location.clone().into_iter().peekable();
        while let Some(part) = iter.next() {
            if iter.peek().is_none() {
                debug!(?part, "Last Part of Path");
                parent_directory = path.clone();
            }
            path = path.join(part.as_ref());
            conflicting_path.push_mut(part.as_ref());
            trace!(?path, ?conflicting_path, "Checking Path");
            if new_directory_start.is_some() {
                continue;
            }
            let metadata = match path.metadata() {
                // If the current path is a directory then we can continue as it can have a file inside it
                Ok(ok) if ok.is_dir() => {
                    continue;
                }
                Ok(ok) => ok,
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    // Only Log this in debug mode or testing
                    #[cfg(any(debug_assertions, test))]
                    if tracing::enabled!(tracing::Level::TRACE) {
                        trace!(?path, "Path does not exist");
                    }
                    new_directory_start = Some(path.clone());
                    continue;
                }
                Err(err) => return Err(LocalStorageError::IOError(err)),
            };

            // If the current path is a file and we have more parts to add then we have a collision
            // Because you can't have a file inside a file
            if metadata.is_file() && iter.peek().is_some() {
                warn!(?path, "Path is a file");
                return Err(PathCollisionError {
                    path: location.clone(),
                    conflicts_with: conflicting_path,
                }
                .into());
            }
        }
        Ok(CreatePath {
            path,
            parent_directory,
            new_directory_start,
        })
    }
    #[instrument(skip(location))]
    pub fn get_path(&self, repository: &Uuid, location: &StoragePath) -> PathBuf {
        let location: PathBuf = location.into();
        let path = self.config.path.join(repository.to_string());
        path.join(location)
    }

    #[instrument]
    pub fn open_file(&self, path: PathBuf) -> Result<StorageFile, LocalStorageError> {
        let meta = StorageFileMeta::read_from_file(&path)?;
        let file = fs::File::open(&path)?;
        Ok(StorageFile::File {
            meta,
            content: StorageFileReader::from(file),
        })
    }
    #[instrument(skip(path), fields(entries.read, entries.skipped))]
    pub async fn open_folder(&self, path: PathBuf) -> Result<StorageFile, LocalStorageError> {
        let mut set = JoinSet::<Result<StorageFileMeta<FileType>, LocalStorageError>>::new();
        let current_span = Span::current();
        let mut files_read = 0;
        let mut files_skipped = 0;
        let mut read_dir = tokio::fs::read_dir(&path).await?;
        while let Some(entry) = read_dir.next_entry().await.transpose() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    current_span.record("entries.read", files_read);
                    current_span.record("entries.skipped", files_skipped);
                    error!(?err, "Error reading directory");
                    set.shutdown().await;
                    return Err(LocalStorageError::from(err));
                }
            };
            let entry = entry;
            let path = entry.path();
            if path.is_file() && is_hidden_file(&path) {
                trace!(?path, "Skipping Meta File");
                files_skipped += 1;
                // Check if file is a meta file
                continue;
            }
            files_read += 1;
            let span_clone = current_span.clone();
            set.spawn_blocking(move || {
                span_clone.in_scope(|| StorageFileMeta::read_from_path(&path))
            });
        }
        current_span.record("entries.read", files_read);
        current_span.record("entries.skipped", files_skipped);
        let meta = StorageFileMeta::read_from_directory(path)?;

        let mut files = vec![];
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(ok)) => files.push(ok),
                Ok(Err(err)) => {
                    set.shutdown().await;
                    return Err(err);
                }
                Err(err) => {
                    error!(?err, "Some unknown error occurred in reading the file!");
                    set.shutdown().await;
                    return Err(LocalStorageError::other(err));
                }
            }
        }
        Ok(StorageFile::Directory { meta, files })
    }

    async fn queue_meta_update(&self, path: PathBuf) -> Result<(), LocalStorageError> {
        self.meta_update_sender
            .send(path)
            .await
            .map_err(|_| LocalStorageError::MetaUpdateChannelClosed)
    }

    pub async fn update_meta_and_parent_metas(
        &self,
        path: &Path,
        greatest_parent: Option<PathBuf>,
    ) -> Result<usize, LocalStorageError> {
        let mut metas_updated = 0;
        if let Some(greatest_parent) = greatest_parent {
            if let Some(parent) = path.parent() {
                if parent == self.config.path {
                    trace!("Do not update root directory");
                } else {
                    self.queue_meta_update(parent.to_path_buf()).await?;
                    metas_updated += 1;
                }
            }

            let mut next_path = greatest_parent.clone();
            for part in path
                .strip_prefix(&greatest_parent)
                .unwrap_or(Path::new(""))
                .components()
            {
                event!(Level::DEBUG, ?next_path, "Updating Meta");
                self.queue_meta_update(next_path.clone()).await?;
                metas_updated += 1;
                next_path = next_path.join(part);
            }
        } else {
            self.queue_meta_update(path.to_path_buf()).await?;
            metas_updated += 1;
            if let Some(parent) = path.parent() {
                self.queue_meta_update(parent.to_path_buf()).await?;
                metas_updated += 1;
            }
        }

        Ok(metas_updated)
    }
}
impl LocalStorage {
    pub fn run_post_save_file(
        self,
        path: PathBuf,
        new_directory_start: Option<PathBuf>,
        span: Span,
    ) -> Result<(), LocalStorageError> {
        let post_save_span = debug_span!(
            parent: &span,
            "Post Save File",
            metas.updated = Empty,
            file.path = debug(&path),
            new.dir = ?new_directory_start,
            otel.exception = Empty,
            otel.status_code = Empty,
        );
        let post_save_span_for_records = post_save_span.clone();
        tokio::task::spawn(
            async move {
                match self
                    .0
                    .update_meta_and_parent_metas(&path, new_directory_start)
                    .await
                {
                    Ok(ok) => {
                        post_save_span_for_records.record("metas.updated", ok);
                        post_save_span_for_records.record("otel.status_code", "OK");
                        debug!(metas.updated = ok, "Updated Metas");
                    }
                    Err(err) => {
                        span.record("exception.message", err.to_string());
                        span.record("otel.status_code", "ERROR");
                        event!(Level::ERROR, ?err, "Error Updating Metas");
                    }
                }
            }
            .instrument(post_save_span),
        );
        Ok(())
    }

    /// Enumerate entries under `root` returning repository-relative paths.
    /// Used for diagnostics when directory removal encounters transient errors.
    pub(crate) async fn directory_entries(root: &Path) -> io::Result<Vec<PathBuf>> {
        let root = root.to_path_buf();
        spawn_blocking(move || {
            let mut entries = Vec::new();
            for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
                let path = entry.path();
                if path == root {
                    continue;
                }
                let relative = path.strip_prefix(&root).unwrap_or(path).to_path_buf();
                entries.push(relative);
            }
            entries.sort();
            Ok(entries)
        })
        .await
        .map_err(|err| io::Error::other(err.to_string()))?
    }
}
impl Storage for LocalStorage {
    type Error = LocalStorageError;
    type DirectoryStream = LocalDirectoryListStream;
    fn storage_type_name(&self) -> &'static str {
        "Local"
    }
    fn storage_config(&self) -> BorrowedStorageConfig<'_> {
        BorrowedStorageConfig {
            storage_config: &self.storage_config,
            config: BorrowedStorageTypeConfig::Local(&self.config),
        }
    }
    #[instrument(
        fields(
            storage.type = "local",
            content.length = ?content.content_len_or_none(),
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
            file.new,
            file.path,
            repository.id = %repository,
        ),
        skip(self,content, repository)
    )]
    async fn save_file(
        &self,
        repository: Uuid,
        content: FileContent,
        location: &StoragePath,
    ) -> Result<(usize, bool), LocalStorageError> {
        let CreatePath {
            path,
            parent_directory,
            new_directory_start,
        } = self.0.get_path_for_creation(repository, location)?;
        if new_directory_start.is_some() {
            trace!("Creating Parent Directory");
            fs::create_dir_all(parent_directory)?;
        }
        let current_span = Span::current();
        let new_file = !path.exists();
        current_span.record("file.new", new_file);
        current_span.record("file.path", debug(&path));
        debug!(?path, "Saving File");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        file.lock_exclusive()?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        let write_result = content.write_to(&mut file);
        let unlock_result = file.unlock();
        let bytes_written = write_result?;
        unlock_result?;
        if !is_hidden_file(&path) {
            // Don't run post save file for meta files
            self.clone()
                .run_post_save_file(path, new_directory_start, current_span)?;
        }
        Ok((bytes_written, new_file))
    }
    #[instrument(
        fields(
            storage.type = "local",
            content.length = ?content.content_len_or_none(),
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
            file.path,
            repository.id = %repository,
        ),
        skip(self,content, repository)
    )]
    async fn append_file(
        &self,
        repository: Uuid,
        content: FileContent,
        location: &StoragePath,
    ) -> Result<usize, LocalStorageError> {
        let CreatePath {
            path,
            parent_directory,
            new_directory_start,
        } = self.0.get_path_for_creation(repository, location)?;
        if new_directory_start.is_some() {
            trace!("Creating Parent Directory");
            fs::create_dir_all(parent_directory)?;
        }
        let current_span = Span::current();
        current_span.record("file.path", debug(&path));
        debug!(?path, "Appending to File");

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.lock_exclusive()?;
        let write_result = content.write_to(&mut file);
        let unlock_result = file.unlock();
        let bytes_written = write_result?;
        unlock_result?;

        // Skip metadata updates for append operations entirely
        // Metadata will be updated when the file is finalized (moved/renamed)
        // This prevents O(n) metadata updates during chunked uploads
        Ok(bytes_written)
    }

    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
            repository.id = %repository,
        ),
        skip(self,repository)
    )]
    async fn delete_file(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<bool, LocalStorageError> {
        let path = self.get_path(&repository, location);
        if !path.exists() {
            debug!(?path, "File does not exist");
            return Ok(false);
        }
        if path.is_dir() {
            info!(?path, "Deleting Directory");
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(&path)?;
            LocationMeta::delete_local(path)?;
        }
        Ok(true)
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self,repository)
    )]
    async fn move_file(
        &self,
        repository: Uuid,
        from: &StoragePath,
        to: &StoragePath,
    ) -> Result<bool, LocalStorageError> {
        let from_path = self.get_path(&repository, from);
        if !from_path.exists() {
            debug!(?from_path, "Source file does not exist");
            return Ok(false);
        }

        let CreatePath {
            path: to_path,
            parent_directory,
            new_directory_start,
        } = self.0.get_path_for_creation(repository, to)?;

        // Create destination directory if needed
        if new_directory_start.is_some() {
            trace!("Creating Parent Directory");
            let parent = parent_directory.clone();
            spawn_blocking(move || fs::create_dir_all(parent))
                .await
                .map_err(LocalStorageError::other)??;
        }

        let current_span = Span::current();
        current_span.record("file.path", debug(&to_path));
        debug!(?from_path, ?to_path, "Moving file");

        // Use fs::rename which is O(1) on same filesystem
        // Wrap in spawn_blocking to avoid blocking async runtime
        let from = from_path.clone();
        let to = to_path.clone();
        spawn_blocking(move || fs::rename(&from, &to))
            .await
            .map_err(LocalStorageError::other)??;

        // Delete old metadata (wrap in spawn_blocking)
        let from_meta = from_path.clone();
        spawn_blocking(move || LocationMeta::delete_local(&from_meta))
            .await
            .map_err(LocalStorageError::other)??;

        // Update metadata for new location
        if !is_hidden_file(&to_path) {
            self.clone()
                .run_post_save_file(to_path, new_directory_start, current_span)?;
        }

        Ok(true)
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self)
    )]
    async fn get_file_information(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<StorageFileMeta<FileType>>, LocalStorageError> {
        let path = self.get_path(&repository, location);

        if !path.exists() {
            debug!(?path, "File does not exist");
            return Ok(None);
        }
        let meta = StorageFileMeta::read_from_path(path)?;
        Ok(Some(meta))
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self)
    )]
    async fn open_file(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<StorageFile>, LocalStorageError> {
        let path = self.get_path(&repository, location);
        if !path.exists() {
            debug!(?path, "File does not exist");
            return Ok(None);
        }
        let file = if path.is_dir() {
            self.open_folder(path).await?
        } else {
            let storage = self.0.clone();
            let path_clone = path.clone();
            spawn_blocking(move || storage.open_file(path_clone))
                .await
                .map_err(LocalStorageError::other)??
        };
        Ok(Some(file))
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self)
    )]
    async fn unload(&self) -> Result<(), LocalStorageError> {
        info!(?self, "Unloading Local Storage");
        let shutdown_signal = self.0.shutdown_signal.lock().await.take();
        if let Some(shutdown_signal) = shutdown_signal {
            if let Err(e) = shutdown_signal.send(()) {
                tracing::error!("Failed to send shutdown signal: {:?}", e);
            }
        } else {
            error!("Shutdown Signal already sent");
        }
        Ok(())
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self)
    )]
    async fn validate_config_change(
        &self,
        config: StorageTypeConfig,
    ) -> Result<(), LocalStorageError> {
        let config = LocalConfig::from_type_config(config)?;
        if self.config.path != config.path {
            return Err(LocalStorageError::PathCannotBeChanged);
        }
        Ok(())
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self)
    )]
    async fn get_repository_meta(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<RepositoryMeta>, LocalStorageError> {
        let path = self.get_path(&repository, location);
        if !path.exists() {
            return Ok(None);
        }
        let meta = LocationMeta::get_or_default_local(&path, None).map(|(meta, _)| meta)?;
        Ok(Some(meta.repository_meta))
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self)
    )]
    async fn put_repository_meta(
        &self,
        repository: Uuid,
        location: &StoragePath,
        value: RepositoryMeta,
    ) -> Result<(), LocalStorageError> {
        let path = self.get_path(&repository, location);

        if !path.exists() {
            return Err(LocalStorageError::IOError(io::Error::new(
                ErrorKind::NotFound,
                "File not found",
            )));
        }

        LocationMeta::set_repository_meta(path, value)?;
        Ok(())
    }
    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            storage.config = ?self.config,
        ),
        skip(self)
    )]
    async fn file_exists(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<bool, LocalStorageError> {
        let path = self.get_path(&repository, location);
        Ok(path.exists())
    }

    #[instrument(
        fields(
            storage.type = "local",
            storage.id = %self.storage_config.storage_id,
            repository = %repository,
        ),
        skip(self)
    )]
    async fn delete_repository(&self, repository: Uuid) -> Result<(), LocalStorageError> {
        let root = self.config.path.join(repository.to_string());
        if !root.exists() {
            return Ok(());
        }

        let is_dir = root.is_dir();
        let mut attempt = 0u8;
        loop {
            attempt = attempt.saturating_add(1);
            let result = if is_dir {
                tokio::fs::remove_dir_all(&root).await
            } else {
                tokio::fs::remove_file(&root).await
            };

            match result {
                Ok(()) => break,
                Err(err) if is_dir && err.kind() == ErrorKind::DirectoryNotEmpty && attempt < 3 => {
                    match Self::directory_entries(&root).await {
                        Ok(entries) => debug!(
                            attempt,
                            repository = %repository,
                            remaining = ?entries,
                            "Repository directory not empty; retrying removal"
                        ),
                        Err(snapshot_err) => debug!(
                            attempt,
                            repository = %repository,
                            error = %snapshot_err,
                            "Directory not empty; retrying removal (failed to snapshot contents)"
                        ),
                    }
                    sleep(Duration::from_millis(25 * attempt as u64)).await;
                }
                Err(err) => return Err(LocalStorageError::IOError(err)),
            }
        }

        Ok(())
    }

    async fn stream_directory(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<Self::DirectoryStream>, Self::Error> {
        let path = self.get_path(&repository, location);
        let stream = {
            let meta = path.metadata();
            match meta {
                Ok(meta) if meta.is_dir() => {
                    let meta = StorageFileMeta::read_from_directory(&path)?;

                    let read_dir = tokio::fs::read_dir(&path).await?;
                    LocalDirectoryListStream::new_directory(read_dir, meta)
                }
                Ok(_) => {
                    if is_hidden_file(&path) {
                        return Ok(None);
                    }
                    let meta = StorageFileMeta::read_from_file(&path)?;

                    LocalDirectoryListStream::new_file(path, meta)
                }
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    return Ok(None);
                }
                Err(err) => {
                    return Err(LocalStorageError::IOError(err));
                }
            }
        };
        Ok(Some(stream))
    }
}
#[derive(Debug, Default)]
pub struct LocalStorageFactory;
impl StaticStorageFactory for LocalStorageFactory {
    type StorageType = LocalStorage;

    type ConfigType = LocalConfig;

    type Error = LocalStorageError;

    fn storage_type_name() -> &'static str
    where
        Self: Sized,
    {
        "Local"
    }

    async fn test_storage_config(_: StorageTypeConfig) -> Result<(), LocalStorageError> {
        Ok(())
    }

    async fn create_storage(
        inner: StorageConfigInner,
        type_config: Self::ConfigType,
    ) -> Result<Self::StorageType, LocalStorageError> {
        if !type_config.path.exists() {
            fs::create_dir_all(&type_config.path)?;
        }
        let (shutdown_signal, shutdown_receiver) = tokio::sync::oneshot::channel();
        let (meta_update_sender, meta_update_receiver) = tokio::sync::mpsc::channel(100);
        let inner = LocalStorageInner {
            config: type_config,
            storage_config: inner,
            shutdown_signal: Mutex::new(Some(shutdown_signal)),
            meta_update_sender,
        };
        meta_update_task(shutdown_receiver, meta_update_receiver);
        let storage = LocalStorage::from(inner);

        Ok(storage)
    }
}
impl StorageFactory for LocalStorageFactory {
    fn storage_name(&self) -> &'static str {
        Self::storage_type_name()
    }

    fn test_storage_config(
        &self,
        _config: StorageTypeConfig,
    ) -> BoxFuture<'static, Result<(), StorageError>> {
        Box::pin(async move { Ok(()) })
    }

    fn create_storage(
        &self,
        config: StorageConfig,
    ) -> BoxFuture<'static, Result<DynStorage, StorageError>> {
        Box::pin(async move {
            <Self as StaticStorageFactory>::create_storage_from_config(config)
                .await
                .map(DynStorage::Local)
                .map_err(Into::into)
        })
    }
}

#[cfg(test)]
mod tests;
