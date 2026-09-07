// ABOUTME: Provides filesystem MIME detection shared by local storage readers.
// ABOUTME: Keeps path-based content classification in one small utility.
use std::{fs::File, path::PathBuf};

use nr_core::storage::SerdeMime;
use tracing::instrument;

#[instrument]
pub fn mime_type_for_file(file: &File, path: PathBuf) -> Option<SerdeMime> {
    if path.extension().unwrap_or_default() == "nr-meta" {
        return Some(SerdeMime(super::FILE_META_MIME));
    }
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    Some(SerdeMime(mime))
}
