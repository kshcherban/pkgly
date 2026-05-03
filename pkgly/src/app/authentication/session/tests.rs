#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::{SessionManager, SessionManagerConfig, cleanup_interval_to_std};
use crate::app::config::Mode;
use chrono::Duration;
use tempfile::tempdir;

#[test]
fn session_manager_creates_table_for_new_database() {
    let tmp_dir = tempdir().expect("create temp dir");
    let db_path = tmp_dir.path().join("sessions.redb");
    let config = SessionManagerConfig {
        lifespan: Duration::seconds(60),
        cleanup_interval: Duration::seconds(60),
        database_location: db_path.clone(),
    };

    let manager = SessionManager::new(config.clone(), Mode::Debug).expect("session manager builds");

    assert!(
        db_path.exists(),
        "session database file should be created on initialization"
    );
    assert_eq!(
        manager.number_of_sessions().expect("read session count"),
        0,
        "fresh session database should contain zero sessions"
    );
}

#[test]
fn cleanup_interval_to_std_rejects_negative_duration() {
    assert!(cleanup_interval_to_std(Duration::seconds(-5)).is_none());
}

#[test]
fn cleanup_interval_to_std_allows_positive_duration() {
    let converted = cleanup_interval_to_std(Duration::seconds(42));
    assert_eq!(converted.map(|duration| duration.as_secs()), Some(42));
}

#[test]
fn get_session_rejects_expired_sessions() {
    let tmp_dir = tempdir().expect("create temp dir");
    let db_path = tmp_dir.path().join("sessions.redb");
    let config = SessionManagerConfig {
        lifespan: Duration::seconds(60),
        cleanup_interval: Duration::seconds(60),
        database_location: db_path.clone(),
    };

    let manager = SessionManager::new(config, Mode::Debug).expect("session manager builds");

    // Create a session with zero-length lifespan (immediately expired)
    let session = manager
        .create_session(
            42,
            "test-agent".to_string(),
            "127.0.0.1".to_string(),
            Duration::seconds(0),
        )
        .expect("create session with zero lifespan");
    let sid = session.session_id.clone();

    // The session should be expired — get_session returns None
    let retrieved = manager.get_session(&sid).expect("get expired session");
    assert!(
        retrieved.is_none(),
        "expired session should not be returned"
    );

    // The expired session remains in storage until the cleaner runs
    let count = manager.number_of_sessions().expect("read session count");
    assert_eq!(
        count, 1,
        "expired session stays in storage until cleaner runs"
    );
}

#[test]
fn get_session_returns_valid_session() {
    let tmp_dir = tempdir().expect("create temp dir");
    let db_path = tmp_dir.path().join("sessions.redb");
    let config = SessionManagerConfig {
        lifespan: Duration::seconds(3600),
        cleanup_interval: Duration::seconds(60),
        database_location: db_path.clone(),
    };

    let manager = SessionManager::new(config, Mode::Debug).expect("session manager builds");

    let session = manager
        .create_session(
            99,
            "test-agent".to_string(),
            "127.0.0.1".to_string(),
            Duration::hours(1),
        )
        .expect("create valid session");
    let sid = session.session_id.clone();

    let retrieved = manager.get_session(&sid).expect("get valid session");
    assert!(
        retrieved.is_some(),
        "non-expired session should be returned"
    );

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.user_id, 99);
    assert_eq!(retrieved.user_agent, "test-agent");
    assert_eq!(retrieved.ip_address, "127.0.0.1");
}
