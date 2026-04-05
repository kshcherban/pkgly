use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, FixedOffset, Local};
use derive_more::derive::From;
use digest::Digest;
use mime::Mime;
use nr_core::{storage::FileHashes, utils::base64_utils};
use serde::{Deserialize, Serialize};
use tracing::{
    Level, Span, debug, event,
    field::{Empty, debug},
    instrument, trace, warn,
};

use crate::{
    fs::utils::MetadataUtils, local::error::LocalStorageError, meta::RepositoryMeta,
    path::PathUtils,
};
use uuid::Uuid;
pub static HIDDEN_FILE_EXTENSIONS: &[&str] = &["nr-meta"];
pub static PKGLY_REPO_META_EXTENSION: &str = "nr-meta";
pub static PKGLY_REPO_META_FILE: &str = ".nr-meta";
pub fn is_hidden_file(path: &Path) -> bool {
    if let Some(file_name) = path.file_name().and_then(|v| v.to_str())
        && (file_name.eq(PKGLY_REPO_META_FILE) || file_name.contains(".nr-meta.tmp-"))
    {
        return true;
    }
    if let Some(extension) = path.extension().and_then(|v| v.to_str()) {
        return HIDDEN_FILE_EXTENSIONS.contains(&extension);
    }
    false
}

pub fn generate_hashes_from_path(path: impl AsRef<Path>) -> Result<FileHashes, io::Error> {
    use md5::Md5;
    use sha1::Sha1;
    use sha2::Sha256;
    use sha3::Sha3_256;

    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha2_256 = Sha256::new();
    let mut sha3_256 = Sha3_256::new();

    let mut buf = [0u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        let chunk = &buf[..read];
        md5.update(chunk);
        sha1.update(chunk);
        sha2_256.update(chunk);
        sha3_256.update(chunk);
    }

    Ok(FileHashes {
        md5: Some(base64_utils::encode(md5.finalize())),
        sha1: Some(base64_utils::encode(sha1.finalize())),
        sha2_256: Some(base64_utils::encode(sha2_256.finalize())),
        sha3_256: Some(base64_utils::encode(sha3_256.finalize())),
    })
}
#[instrument(skip(buffer))]
pub fn generate_from_bytes(buffer: &[u8]) -> FileHashes {
    FileHashes {
        md5: Some(generate_md5(buffer)),
        sha1: Some(generate_sha1(buffer)),
        sha2_256: Some(generate_sha2_256(buffer)),
        sha3_256: Some(generate_sha3_256(buffer)),
    }
}
fn generate_md5(buffer: &[u8]) -> String {
    use md5::Md5;

    let mut hasher = Md5::new();
    hasher.update(buffer);
    let hash = hasher.finalize();
    base64_utils::encode(hash)
}
fn generate_sha1(buffer: &[u8]) -> String {
    use sha1::Sha1;

    let mut hasher = Sha1::new();
    hasher.update(buffer);
    let hash = hasher.finalize();
    base64_utils::encode(hash)
}
fn generate_sha2_256(buffer: &[u8]) -> String {
    use sha2::Sha256;

    let mut hasher = Sha256::new();
    hasher.update(buffer);
    let hash = hasher.finalize();
    base64_utils::encode(hash)
}
fn generate_sha3_256(buffer: &[u8]) -> String {
    use sha3::Sha3_256;

    let mut hasher = Sha3_256::new();
    hasher.update(buffer);
    let hash = hasher.finalize();
    base64_utils::encode(hash)
}

pub const FILE_META_MIME: Mime = mime::APPLICATION_JSON;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationMeta {
    /// None if the file is a directory or meta file
    pub created: DateTime<FixedOffset>,
    pub modified: DateTime<FixedOffset>,
    pub location_typed_meta: LocationTypedMeta,
    pub repository_meta: RepositoryMeta,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, From)]
pub enum LocationTypedMeta {
    Directory(DirectoryMeta),
    File(FileMeta),
}
impl LocationTypedMeta {
    pub fn update(
        &mut self,
        path: impl AsRef<Path>,
        hashes: Option<&FileHashes>,
    ) -> Result<(), LocalStorageError> {
        match self {
            LocationTypedMeta::Directory(meta) => meta.recount_files(path),
            LocationTypedMeta::File(meta) => meta.update_hashes(path, hashes),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryMeta {
    pub number_of_files: u64,
}
impl DirectoryMeta {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, LocalStorageError> {
        let mut meta = Self { number_of_files: 0 };
        meta.recount_files(path)?;
        Ok(meta)
    }
    pub fn recount_files(&mut self, path: impl AsRef<Path>) -> Result<(), LocalStorageError> {
        self.number_of_files = path
            .as_ref()
            .read_dir()?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if is_hidden_file(&entry.path()) {
                    return None;
                }
                Some(entry)
            })
            .count() as u64;
        debug!(?self.number_of_files, path = ?path.as_ref(), "Counted Files");
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    pub hashes: FileHashes,
}
impl FileMeta {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, LocalStorageError> {
        Ok(Self {
            hashes: generate_hashes_from_path(path)?,
        })
    }
    pub fn from_hashes(hashes: FileHashes) -> Self {
        Self { hashes }
    }
    pub fn update_hashes(
        &mut self,
        path: impl AsRef<Path>,
        hashes: Option<&FileHashes>,
    ) -> Result<(), LocalStorageError> {
        if let Some(hash) = hashes {
            self.hashes = hash.clone();
            return Ok(());
        }
        self.hashes = generate_hashes_from_path(path)?;

        Ok(())
    }
}
impl LocationMeta {
    pub fn dir_meta_or_err(&self) -> Result<&DirectoryMeta, LocalStorageError> {
        if let LocationTypedMeta::Directory(meta) = &self.location_typed_meta {
            Ok(meta)
        } else {
            Err(LocalStorageError::expected_directory())
        }
    }
    pub fn file_meta_or_err(&self) -> Result<&FileMeta, LocalStorageError> {
        if let LocationTypedMeta::File(meta) = &self.location_typed_meta {
            Ok(meta)
        } else {
            Err(LocalStorageError::expected_file())
        }
    }
    #[instrument(
        level = "debug",
        skip(path),
        fields(
            path = ?path.as_ref(),
        )
    )]
    pub(crate) fn create_meta_or_update(
        path: impl AsRef<Path>,
        hashes: Option<&FileHashes>,
    ) -> Result<(), LocalStorageError> {
        let path_ref = path.as_ref();
        let (mut meta, was_created) = Self::get_or_default_local(path_ref, hashes)?;
        if !was_created {
            event!(Level::DEBUG, path = ?path_ref, "Updating Meta File");
            meta.location_typed_meta.update(path_ref, hashes)?;
            meta.modified = Local::now().into();
            meta.save_meta(path_ref)?;
        }

        Ok(())
    }
    #[instrument(
        level = "debug",
        skip(path),
        fields(
            path = ?path.as_ref(),
            path.meta = Empty,
            created = Empty,
        )
    )]
    pub(crate) fn get_or_default_local(
        path: impl AsRef<Path>,
        hashes: Option<&FileHashes>,
    ) -> Result<(LocationMeta, bool), LocalStorageError> {
        let span = Span::current();
        let meta_path = meta_path(&path)?;
        span.record("path.meta", debug(&meta_path));
        if meta_path.exists() {
            trace!(?meta_path, "Meta File exists. Reading");
            match LocationMeta::read_meta_file(&meta_path) {
                Ok(meta) => {
                    span.record("created", false);
                    return Ok((meta, false));
                }
                Err(LocalStorageError::Postcard(err)) => {
                    event!(
                        Level::ERROR,
                        ?meta_path,
                        ?err,
                        "Meta File is corrupted. Rebuilding"
                    );
                }
                Err(err) => {
                    return Err(err);
                }
            }
        } else if tracing::enabled!(Level::DEBUG) {
            debug!(?meta_path, "Meta File does not exist. Generating");
        }
        span.record("created", true);
        let path_ref = path.as_ref();
        let (created, modified) = {
            let file = File::open(path_ref)?;
            let metadata = file.metadata()?;
            let modified = metadata.modified_as_chrono_or_now()?;
            let created = metadata.created_as_chrono_or_now()?;
            (created, modified)
        };
        let location_meta = if path_ref.is_dir() {
            LocationTypedMeta::Directory(DirectoryMeta::new(path_ref)?)
        } else {
            let file_meta = if let Some(hash) = hashes {
                FileMeta::from_hashes(hash.clone())
            } else {
                FileMeta::new(path_ref)?
            };
            LocationTypedMeta::File(file_meta)
        };
        let meta = LocationMeta {
            created,
            modified,
            repository_meta: RepositoryMeta::default(),
            location_typed_meta: location_meta,
        };
        meta.save_meta(path_ref)?;

        Ok((meta, true))
    }

    #[instrument(
        level = "debug",
        skip(path),
        fields(
            path = ?path.as_ref(),
        )
    )]
    /// Assumes the path is the path to the `.nr-meta` file
    fn read_meta_file(path: impl AsRef<Path>) -> Result<LocationMeta, LocalStorageError> {
        let mut file = File::open(path)?;

        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let meta: LocationMeta = postcard::from_bytes(&bytes)?;
        Ok(meta)
    }

    #[instrument(
        level = "debug",
        skip(path),
        fields(
            path = ?path.as_ref(),
            path.meta = Empty,
        )
    )]
    pub(crate) fn delete_local(path: impl AsRef<Path>) -> Result<(), LocalStorageError> {
        let meta_path = meta_path(&path)?;
        Span::current().record("path.meta", debug(&meta_path));
        if !meta_path.exists() {
            warn!(?meta_path, "Meta File does not exist");
            return Ok(());
        }
        debug!(?meta_path, "Deleting Meta File");
        std::fs::remove_file(meta_path)?;
        Ok(())
    }
    #[instrument(
        level = "debug",
        skip(path),
        fields(
            path = ?path.as_ref(),
            path.meta = Empty,
            created = Empty,
        )
    )]
    pub(crate) fn save_meta(&self, path: impl AsRef<Path>) -> Result<(), LocalStorageError> {
        let span = Span::current();
        let meta_path = meta_path(path)?;
        span.record("path.meta", debug(&meta_path));
        span.record("created", !meta_path.exists());

        let bytes = postcard::to_allocvec(self)?;
        let file_name = meta_path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid meta file name"))?;
        let tmp_name = format!(
            "{file_name}.tmp-{}.{}",
            Uuid::new_v4(),
            PKGLY_REPO_META_EXTENSION
        );
        let tmp_path = meta_path.with_file_name(tmp_name);

        let mut tmp_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        tmp_file.write_all(&bytes)?;
        tmp_file.sync_all()?;

        let rename_result = std::fs::rename(&tmp_path, &meta_path);
        let rename_result = match rename_result {
            Ok(()) => Ok(()),
            Err(_err) if cfg!(windows) && meta_path.exists() => {
                std::fs::remove_file(&meta_path)?;
                std::fs::rename(&tmp_path, &meta_path)
            }
            Err(err) => Err(err),
        };

        if let Err(err) = rename_result {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err.into());
        }

        event!(Level::DEBUG, "Saved Meta File");
        Ok(())
    }

    #[instrument(
        level = "debug",
        skip(path),
        fields(
            path = ?path.as_ref(),
        )
    )]
    pub(crate) fn set_repository_meta(
        path: impl AsRef<Path>,
        repository_meta: RepositoryMeta,
    ) -> Result<(), LocalStorageError> {
        let (mut meta, _) = Self::get_or_default_local(&path, None)?;
        meta.repository_meta = repository_meta;
        meta.save_meta(path)
    }
}

fn meta_path(path: impl AsRef<Path>) -> Result<PathBuf, LocalStorageError> {
    let meta_path = path.as_ref().to_path_buf();
    let meta_path = if meta_path.is_dir() {
        meta_path.join(PKGLY_REPO_META_FILE)
    } else {
        meta_path.add_extension(PKGLY_REPO_META_EXTENSION)?
    };
    Ok(meta_path)
}

#[cfg(test)]
mod tests;
