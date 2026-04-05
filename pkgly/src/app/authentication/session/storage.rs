use chrono::Duration;

use super::{Session, SessionError, SessionManager};

/// Abstract session storage backend.
///
/// This trait allows `SessionManager` to work with different storage
/// implementations (e.g. Redb, in-memory, or external services) without
/// changing its public API.
pub trait SessionStorage {
    /// Create a new session with a custom lifetime.
    fn create_session(
        &self,
        user_id: i32,
        user_agent: String,
        ip_address: String,
        life: Duration,
    ) -> Result<Session, SessionError>;

    /// Get a session by id.
    fn get_session(&self, session_id: &str) -> Result<Option<Session>, SessionError>;

    /// Delete a session by id and return it if it existed.
    fn delete_session(&self, session_id: &str) -> Result<Option<Session>, SessionError>;

    /// Delete all sessions for the given user and return the number removed.
    fn delete_sessions_for_user(&self, user_id: i32) -> Result<u32, SessionError>;

    /// Return the number of active sessions.
    fn number_of_sessions(&self) -> Result<u64, SessionError>;
}

#[cfg(test)]
mod tests {
    use super::{SessionManager, SessionStorage};
    use crate::app::{authentication::session::SessionManagerConfig, config::Mode};
    use chrono::Duration;
    use tempfile::tempdir;

    #[test]
    fn session_manager_implements_session_storage_trait() {
        let tmp_dir = tempdir().expect("create temp dir");
        let db_path = tmp_dir.path().join("sessions.redb");
        let config = SessionManagerConfig {
            lifespan: Duration::seconds(60),
            cleanup_interval: Duration::seconds(60),
            database_location: db_path,
        };

        let manager =
            SessionManager::new(config, Mode::Debug).expect("session manager should build");

        // This is a compile-time check that SessionManager implements SessionStorage.
        fn assert_storage<T: SessionStorage>(_value: &T) {}
        assert_storage(&manager);
    }
}

impl SessionStorage for SessionManager {
    fn create_session(
        &self,
        user_id: i32,
        user_agent: String,
        ip_address: String,
        life: Duration,
    ) -> Result<Session, SessionError> {
        SessionManager::create_session(self, user_id, user_agent, ip_address, life)
    }

    fn get_session(&self, session_id: &str) -> Result<Option<Session>, SessionError> {
        SessionManager::get_session(self, session_id)
    }

    fn delete_session(&self, session_id: &str) -> Result<Option<Session>, SessionError> {
        SessionManager::delete_session(self, session_id)
    }

    fn delete_sessions_for_user(&self, user_id: i32) -> Result<u32, SessionError> {
        SessionManager::delete_sessions_for_user(self, user_id)
    }

    fn number_of_sessions(&self) -> Result<u64, SessionError> {
        SessionManager::number_of_sessions(self)
    }
}
