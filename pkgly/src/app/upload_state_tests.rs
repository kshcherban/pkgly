#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn docker_upload_state_only_emits_sha256() {
    let mut state = UploadState::new_sha256_only();
    state.update(b"hello world");

    let finalized = state.finalize();
    assert!(finalized.hashes.md5.is_none());
    assert!(finalized.hashes.sha1.is_none());
    assert!(finalized.hashes.sha3_256.is_none());
    assert!(finalized.hashes.sha2_256.is_some());
    assert_eq!(
        finalized.digest,
        "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}
