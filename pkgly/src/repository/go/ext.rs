use nr_core::storage::StoragePath;
use nr_storage::{FileContent, Storage, StorageFile};
use tracing::{debug, instrument, warn};

use super::types::{GoModuleError, GoModulePath, GoVersion};

use crate::repository::{Repository, RepositoryHandlerError};

/// Extension trait for Go repositories providing common operations around storage-backed modules.
pub trait GoRepositoryExt: Repository {
    /// List all versions for a Go module by reading the `@v/list` file.
    #[instrument(skip(self))]
    async fn list_go_module_versions(
        &self,
        module_path: &GoModulePath,
    ) -> Result<Vec<String>, RepositoryHandlerError> {
        let mut list_path = StoragePath::from(module_path.as_str());
        list_path.push_mut("@v/list");

        match self.get_storage().open_file(self.id(), &list_path).await {
            Ok(Some(StorageFile::File { meta, content })) => {
                let size_hint = usize::try_from(meta.file_type.file_size).unwrap_or(0);
                let bytes = content
                    .read_to_vec(size_hint)
                    .await
                    .map_err(|err| RepositoryHandlerError::Other(Box::new(err)))?;
                let list = String::from_utf8(bytes).map_err(|err| {
                    RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(format!(
                            "Invalid UTF-8 in version list: {err}"
                        )),
                    ))
                })?;
                let mut versions = Vec::new();
                for line in list.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    versions.push(trimmed.to_string());
                }
                Ok(versions)
            }
            Ok(Some(StorageFile::Directory { .. })) | Ok(None) => Ok(Vec::new()),
            Err(err) => Err(RepositoryHandlerError::Other(Box::new(err))),
        }
    }

    /// Get the latest version of a Go module from the stored version list.
    #[instrument(skip(self))]
    async fn get_latest_go_version(
        &self,
        module_path: &GoModulePath,
    ) -> Result<Option<GoVersion>, RepositoryHandlerError> {
        let versions = self.list_go_module_versions(module_path).await?;
        if versions.is_empty() {
            return Ok(None);
        }
        let mut parsed_versions: Vec<GoVersion> = versions
            .into_iter()
            .filter_map(|version| match GoVersion::new(version) {
                Ok(parsed) => Some(parsed),
                Err(err) => {
                    warn!(
                        module = %module_path.as_str(),
                        ?err,
                        "Skipping malformed version entry"
                    );
                    None
                }
            })
            .collect();
        if parsed_versions.is_empty() {
            return Ok(None);
        }
        parsed_versions.sort();
        Ok(parsed_versions.last().cloned())
    }

    /// Check if a specific module artifact exists in storage without loading it.
    #[instrument(skip(self))]
    async fn module_file_exists(
        &self,
        module_path: &GoModulePath,
        version: &GoVersion,
        file_type: GoFileType,
    ) -> Result<bool, RepositoryHandlerError> {
        let storage_path = match file_type {
            GoFileType::GoMod => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.mod", version.as_str()));
                path
            }
            GoFileType::Info => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.info", version.as_str()));
                path
            }
            GoFileType::Zip => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.zip", version.as_str()));
                path
            }
            GoFileType::GoModWithoutVersion => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut("go.mod");
                path
            }
        };

        match self.get_storage().open_file(self.id(), &storage_path).await {
            Ok(Some(StorageFile::File { .. })) => Ok(true),
            Ok(Some(StorageFile::Directory { .. })) | Ok(None) => Ok(false),
            Err(err) => Err(RepositoryHandlerError::Other(Box::new(err))),
        }
    }

    /// Save Go module file (go.mod, .info, or .zip) to storage.
    #[instrument(skip(self, content))]
    async fn save_go_module_file(
        &self,
        module_path: &GoModulePath,
        version: &GoVersion,
        file_type: GoFileType,
        content: Vec<u8>,
    ) -> Result<(), RepositoryHandlerError> {
        let storage_path = match file_type {
            GoFileType::GoMod => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.mod", version.as_str()));
                path
            }
            GoFileType::Info => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.info", version.as_str()));
                path
            }
            GoFileType::Zip => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.zip", version.as_str()));
                path
            }
            GoFileType::GoModWithoutVersion => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut("go.mod");
                path
            }
        };

        self.get_storage()
            .save_file(self.id(), FileContent::Content(content), &storage_path)
            .await
            .map_err(|err| RepositoryHandlerError::Other(Box::new(err)))?;

        debug!(path = %storage_path.to_string(), "Saved Go module file");
        Ok(())
    }

    /// Load Go module file from storage.
    #[instrument(skip(self))]
    async fn load_go_module_file(
        &self,
        module_path: &GoModulePath,
        version: Option<&GoVersion>,
        file_type: GoFileType,
    ) -> Result<Option<Vec<u8>>, RepositoryHandlerError> {
        let storage_path = match file_type {
            GoFileType::GoMod => {
                let Some(version) = version else {
                    return Err(RepositoryHandlerError::Other(Box::new(
                        GoModuleError::InvalidVersion(
                            "Version required for go.mod file".to_string(),
                        ),
                    )));
                };
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.mod", version.as_str()));
                path
            }
            GoFileType::Info => {
                let Some(version) = version else {
                    return Err(RepositoryHandlerError::Other(Box::new(
                        GoModuleError::InvalidVersion("Version required for info file".to_string()),
                    )));
                };
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.info", version.as_str()));
                path
            }
            GoFileType::Zip => {
                let Some(version) = version else {
                    return Err(RepositoryHandlerError::Other(Box::new(
                        GoModuleError::InvalidVersion("Version required for zip file".to_string()),
                    )));
                };
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut(&format!("@v/{}.zip", version.as_str()));
                path
            }
            GoFileType::GoModWithoutVersion => {
                let mut path = StoragePath::from(module_path.as_str());
                path.push_mut("go.mod");
                path
            }
        };

        match self.get_storage().open_file(self.id(), &storage_path).await {
            Ok(Some(StorageFile::File { meta, content })) => {
                debug!(path = %storage_path.to_string(), "Loaded Go module file");
                let size_hint = usize::try_from(meta.file_type.file_size).unwrap_or(0);
                match content.read_to_vec(size_hint).await {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(err) => {
                        warn!(
                            path = %storage_path.to_string(),
                            ?err,
                            "Failed to read Go module file content"
                        );
                        Ok(None)
                    }
                }
            }
            Ok(Some(_)) | Ok(None) => Ok(None),
            Err(err) => {
                warn!(
                    path = %storage_path.to_string(),
                    ?err,
                    "Error opening Go module file"
                );
                Ok(None)
            }
        }
    }

    /// Ensure the version list contains the provided version.
    #[instrument(skip(self))]
    async fn ensure_version_list(
        &self,
        module_path: &GoModulePath,
        version: &GoVersion,
    ) -> Result<(), RepositoryHandlerError> {
        let mut versions: Vec<GoVersion> = self
            .list_go_module_versions(module_path)
            .await?
            .into_iter()
            .filter_map(|entry| match GoVersion::new(entry) {
                Ok(parsed) => Some(parsed),
                Err(err) => {
                    warn!(
                        module = %module_path.as_str(),
                        ?err,
                        "Skipping malformed version entry while updating list"
                    );
                    None
                }
            })
            .collect();

        if !versions
            .iter()
            .any(|existing| existing.as_str() == version.as_str())
        {
            versions.push(version.clone());
        }
        versions.sort();

        let mut list_path = StoragePath::from(module_path.as_str());
        list_path.push_mut("@v/list");

        let mut body = versions
            .iter()
            .map(|entry| entry.as_str().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        body.push('\n');

        self.get_storage()
            .save_file(
                self.id(),
                FileContent::Content(body.into_bytes()),
                &list_path,
            )
            .await
            .map_err(|err| RepositoryHandlerError::Other(Box::new(err)))?;

        Ok(())
    }

    /// Refresh cached aliases (`@latest` and `go.mod`) for the module.
    #[instrument(skip(self))]
    async fn refresh_latest_aliases(
        &self,
        module_path: &GoModulePath,
    ) -> Result<(), RepositoryHandlerError> {
        let Some(latest_version) = self.get_latest_go_version(module_path).await? else {
            self.delete_latest_aliases(module_path).await?;
            return Ok(());
        };

        let storage = self.get_storage();

        if let Some(info_bytes) = self
            .load_go_module_file(module_path, Some(&latest_version), GoFileType::Info)
            .await?
        {
            let mut latest_path = StoragePath::from(module_path.as_str());
            latest_path.push_mut("@latest");
            storage
                .save_file(
                    self.id(),
                    FileContent::Content(info_bytes.clone()),
                    &latest_path,
                )
                .await
                .map_err(|err| RepositoryHandlerError::Other(Box::new(err)))?;
        }

        if let Some(go_mod_bytes) = self
            .load_go_module_file(module_path, Some(&latest_version), GoFileType::GoMod)
            .await?
        {
            let mut go_mod_path = StoragePath::from(module_path.as_str());
            go_mod_path.push_mut("go.mod");
            storage
                .save_file(
                    self.id(),
                    FileContent::Content(go_mod_bytes.clone()),
                    &go_mod_path,
                )
                .await
                .map_err(|err| RepositoryHandlerError::Other(Box::new(err)))?;
        }

        Ok(())
    }

    /// Remove cached aliases when no versions remain.
    #[instrument(skip(self))]
    async fn delete_latest_aliases(
        &self,
        module_path: &GoModulePath,
    ) -> Result<(), RepositoryHandlerError> {
        let storage = self.get_storage();

        let mut latest_path = StoragePath::from(module_path.as_str());
        latest_path.push_mut("@latest");
        if let Err(err) = storage.delete_file(self.id(), &latest_path).await {
            warn!(
                path = %latest_path.to_string(),
                ?err,
                "Failed to delete @latest alias"
            );
        }

        let mut go_mod_path = StoragePath::from(module_path.as_str());
        go_mod_path.push_mut("go.mod");
        if let Err(err) = storage.delete_file(self.id(), &go_mod_path).await {
            warn!(
                path = %go_mod_path.to_string(),
                ?err,
                "Failed to delete go.mod alias"
            );
        }

        Ok(())
    }
}

/// Types of Go module files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoFileType {
    GoMod,
    Info,
    Zip,
    GoModWithoutVersion,
}

// Blanket implementation for all Repository types.
impl<T: Repository> GoRepositoryExt for T {}
