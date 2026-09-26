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

#[test]
fn session_id_has_minimum_length() {
    use crate::app::authentication::session::create_session_id;

    // Verify session IDs are long enough to resist brute-force
    let id = create_session_id(|_| false);
    assert!(
        id.len() >= 30,
        "session ID length {} should be >= 30, got: {}",
        id.len(),
        id
    );
}

#[test]
fn session_id_collision_avoidance() {
    use crate::app::authentication::session::create_session_id;

    let mut seen = std::collections::HashSet::new();
    for _ in 0..1000 {
        let id = create_session_id(|s| seen.contains(s));
        assert!(seen.insert(id), "session IDs must be unique");
    }
}
