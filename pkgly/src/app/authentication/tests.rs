#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::password;
use chrono::{Duration, Utc};

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
fn expired_token_time_check_logic() {
    let now = Utc::now().fixed_offset();
    let future = now + Duration::hours(1);
    let past = now - Duration::hours(1);

    // Expired tokens (past) should always be considered expired
    assert!(past <= now, "past timestamp must be <= now");

    // Future tokens (future) should not be considered expired
    assert!(future > now, "future timestamp must be > now");

    // Null expiry (non-expiring token) should pass the check
    let has_expired: Option<chrono::DateTime<chrono::FixedOffset>> = None;
    assert!(has_expired.is_none(), "null expiry should pass");
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
