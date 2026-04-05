#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::password;

#[test]
fn encrypt_password_produces_verifiable_hash() {
    let hashed = password::encrypt_password("super-secret");
    assert!(hashed.is_some(), "password hashing should succeed");
    let hash = match hashed {
        Some(value) => value,
        None => unreachable!("hash guaranteed by previous assertion"),
    };
    assert!(password::verify_password("super-secret", Some(hash.as_str())).is_ok());
    assert!(password::verify_password("invalid", Some(hash.as_str())).is_err());
}
