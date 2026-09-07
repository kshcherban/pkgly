// ABOUTME: Provides small path transformations used by local storage metadata.
// ABOUTME: Reports non-UTF-8 extensions without hiding filesystem errors.
use std::path::PathBuf;

use thiserror::Error;
use tracing::instrument;

#[derive(Debug, Error)]
pub enum ExtensionError {
    #[error("The extension of path {0} is not UTF-8")]
    ExtensionNotUtf8(PathBuf),
}

pub trait PathUtils {
    /// Appends an extension to the path.
    fn add_extension(&self, extension: &str) -> Result<PathBuf, ExtensionError>;
    /// Gets the current extension and attempts to convert it to a string.
    fn extension_to_string(&self) -> Result<Option<&str>, ExtensionError>;
}
impl PathUtils for PathBuf {
    fn extension_to_string(&self) -> Result<Option<&str>, ExtensionError> {
        self.extension()
            .map(|v| {
                v.to_str()
                    .ok_or_else(|| ExtensionError::ExtensionNotUtf8(self.clone()))
            })
            .transpose()
    }
    #[instrument]
    fn add_extension(&self, extension: &str) -> Result<PathBuf, ExtensionError> {
        let mut path = self.clone();
        let old_extension = path.extension_to_string()?;
        match old_extension {
            Some(old_extension) => {
                path.set_extension(format!("{}.{}", old_extension, extension));
            }
            None => {
                path.set_extension(extension);
            }
        }
        Ok(path)
    }
}
