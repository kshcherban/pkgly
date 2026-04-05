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
