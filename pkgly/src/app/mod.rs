use std::sync::Arc;

use digest::Digest;
use md5::Md5;
use nr_core::{storage::FileHashes, utils::base64_utils};
use sha1::Sha1;
use sha2::Sha256;
use sha3::Sha3_256;
pub mod api;
pub mod authentication;
pub mod config;
pub mod email;
pub mod email_service;
pub mod frontend;
pub mod open_api;
pub mod request_logging;
pub mod resources;
pub mod scheduler;

pub mod responses;
pub mod routes;
pub mod site;
pub mod state;
pub mod web;

pub use self::state::{
    Instance, InstanceOAuth2Provider, InstanceOAuth2Settings, InstanceSsoSettings,
    RepositoryStorageName,
};

#[derive(Debug)]
struct UploadState {
    md5: Option<Md5>,
    sha1: Option<Sha1>,
    sha2: Sha256,
    sha3: Option<Sha3_256>,
    length: u64,
    sha256_only: bool,
}

#[derive(Clone)]
pub struct BlobUploadStateHandle(Arc<parking_lot::Mutex<UploadState>>);

#[derive(Debug, Clone)]
pub struct FinalizedUpload {
    pub digest: String,
    pub hashes: FileHashes,
    pub length: u64,
}

impl UploadState {
    /// Create upload state with all hash algorithms (for general use)
    fn new() -> Self {
        Self {
            md5: Some(Md5::new()),
            sha1: Some(Sha1::new()),
            sha2: Sha256::new(),
            sha3: Some(Sha3_256::new()),
            length: 0,
            sha256_only: false,
        }
    }

    /// Create upload state with only SHA256 (for Docker)
    fn new_sha256_only() -> Self {
        Self {
            md5: None,
            sha1: None,
            sha2: Sha256::new(),
            sha3: None,
            length: 0,
            sha256_only: true,
        }
    }

    fn update(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }

        if let Some(md5) = &mut self.md5 {
            md5.update(chunk);
        }
        if let Some(sha1) = &mut self.sha1 {
            sha1.update(chunk);
        }
        self.sha2.update(chunk);
        if let Some(sha3) = &mut self.sha3 {
            sha3.update(chunk);
        }
        self.length += chunk.len() as u64;
    }

    fn finalize(self) -> FinalizedUpload {
        let sha2_bytes = self.sha2.finalize();
        let digest = format!("sha256:{:x}", sha2_bytes);
        let hashes = FileHashes {
            md5: self.md5.map(|h| base64_utils::encode(h.finalize())),
            sha1: self.sha1.map(|h| base64_utils::encode(h.finalize())),
            sha2_256: Some(base64_utils::encode(&sha2_bytes)),
            sha3_256: self.sha3.map(|h| base64_utils::encode(h.finalize())),
        };

        FinalizedUpload {
            digest,
            hashes,
            length: self.length,
        }
    }

    fn take(&mut self) -> Self {
        let sha_only = self.sha256_only;
        let mut replacement = if sha_only {
            UploadState::new_sha256_only()
        } else {
            UploadState::new()
        };
        std::mem::swap(self, &mut replacement);
        replacement
    }
}

impl BlobUploadStateHandle {
    fn new(state: UploadState) -> Self {
        Self(Arc::new(parking_lot::Mutex::new(state)))
    }

    fn lock(&self) -> parking_lot::MutexGuard<'_, UploadState> {
        self.0.lock()
    }

    fn try_into_state(self) -> Result<UploadState, Self> {
        match Arc::try_unwrap(self.0) {
            Ok(inner) => Ok(inner.into_inner()),
            Err(arc) => Err(Self(arc)),
        }
    }
}

#[cfg(test)]
mod upload_state_tests;

pub use self::site::{
    AppMetrics, InternalServices, Pkgly, PkglyInner, PkglyState, REPOSITORY_CONFIG_TYPES,
    REPOSITORY_TYPES,
};

#[cfg(test)]
mod tests;
