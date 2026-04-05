#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use sqlx::postgres::PgConnectOptions;

struct StubAuth {
    allow: bool,
}

impl HasPermissions for StubAuth {
    fn user_id(&self) -> Option<i32> {
        Some(1)
    }

    fn get_permissions(&self) -> Option<UserPermissions> {
        None
    }

    async fn has_action(
        &self,
        _action: RepositoryActions,
        _repository: Uuid,
        _db: &PgPool,
    ) -> Result<bool, sqlx::Error> {
        Ok(self.allow)
    }
}

use nr_core::user::permissions::UserPermissions;

fn test_pool() -> PgPool {
    PgPool::connect_lazy_with(
        PgConnectOptions::new()
            .host("localhost")
            .username("postgres")
            .database("postgres"),
    )
}

#[tokio::test]
async fn auth_config_enabled_requires_permission() {
    let pool = test_pool();
    let repository_id = Uuid::new_v4();
    let config = RepositoryAuthConfig { enabled: true };
    let allowed = StubAuth { allow: true };
    let denied = StubAuth { allow: false };

    assert!(
        can_read_repository_with_auth(&allowed, Visibility::Public, repository_id, &pool, &config)
            .await
            .unwrap()
    );
    assert!(
        !can_read_repository_with_auth(&denied, Visibility::Public, repository_id, &pool, &config)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn public_repository_without_auth_config_allows_guests() {
    let pool = test_pool();
    let repository_id = Uuid::new_v4();
    let config = RepositoryAuthConfig { enabled: false };
    let denied = StubAuth { allow: false };

    assert!(
        can_read_repository_with_auth(&denied, Visibility::Public, repository_id, &pool, &config)
            .await
            .unwrap()
    );
}
