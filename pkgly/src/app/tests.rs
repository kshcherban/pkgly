#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

#[test]
fn upload_state_streaming_hashes_match_reference() {
    let chunks: &[&[u8]] = &[b"streamed ", b"payload ", b"verification"];
    let mut state = UploadState::new();
    for chunk in chunks {
        state.update(chunk);
    }

    let finalized = state.finalize();
    let combined = b"streamed payload verification";

    assert_eq!(finalized.length, combined.len() as u64);

    let expected_digest = format!("sha256:{:x}", Sha256::digest(combined));
    assert_eq!(finalized.digest, expected_digest);

    let expected_md5 = BASE64_STANDARD.encode(md5::Md5::digest(combined).as_slice());
    assert_eq!(finalized.hashes.md5.as_deref(), Some(expected_md5.as_str()));

    let expected_sha1 = BASE64_STANDARD.encode(sha1::Sha1::digest(combined).as_slice());
    assert_eq!(
        finalized.hashes.sha1.as_deref(),
        Some(expected_sha1.as_str())
    );

    let expected_sha2 = BASE64_STANDARD.encode(sha2::Sha256::digest(combined).as_slice());
    assert_eq!(
        finalized.hashes.sha2_256.as_deref(),
        Some(expected_sha2.as_str())
    );

    let expected_sha3 = BASE64_STANDARD.encode(sha3::Sha3_256::digest(combined).as_slice());
    assert_eq!(
        finalized.hashes.sha3_256.as_deref(),
        Some(expected_sha3.as_str())
    );
}
