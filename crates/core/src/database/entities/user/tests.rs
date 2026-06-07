// ABOUTME: Tests user entity serialization, request parsing, and persistence.
// ABOUTME: Verifies passwords stay private and initial permissions persist atomically.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use crate::{
    database::entities::user::{NewUserRequest, UserType},
    testing::TestCore,
    user::permissions::{InitialUserPermissions, RepositoryActions},
};

/// Just incase a bug get's introduced from serde where the password is serialized. We want to error out.
#[test]
pub fn assert_no_serialize_password() {
    let user = super::User {
        password: Some("password".to_owned()),
        id: Default::default(),
        name: Default::default(),
        username: "username".parse().unwrap(),
        email: Some("email@email.com".parse().unwrap()),
        active: Default::default(),
        password_last_changed: Default::default(),
        require_password_change: Default::default(),
        admin: Default::default(),
        user_manager: Default::default(),
        system_manager: Default::default(),
        default_repository_actions: Default::default(),
        updated_at: Default::default(),
        created_at: Default::default(),
    };
    let json = serde_json::to_value(&user).unwrap();

    assert!(
        json.get("password").is_none(),
        "Password should not be serialized"
    );
}

#[test]
fn new_user_request_deserializes_supplied_permissions() {
    let request: NewUserRequest = serde_json::from_str(
        r#"{
            "name":"Test User",
            "username":"test-user",
            "email":"test@example.com",
            "password":"password",
            "permissions":{
                "admin":false,
                "user_manager":true,
                "system_manager":false,
                "default_repository_actions":["Read","Write"]
            }
        }"#,
    )
    .unwrap();

    assert_eq!(
        request.permissions,
        Some(InitialUserPermissions {
            admin: false,
            user_manager: true,
            system_manager: false,
            default_repository_actions: vec![RepositoryActions::Read, RepositoryActions::Write],
        })
    );
}

#[test]
fn new_user_request_allows_omitted_permissions() {
    let request: NewUserRequest = serde_json::from_str(
        r#"{
            "name":"Test User",
            "username":"test-user",
            "email":null,
            "password":null
        }"#,
    )
    .unwrap();

    assert_eq!(request.permissions, None);
}

#[tokio::test]
#[ignore = "requires nr_tests.env with a PostgreSQL test database"]
async fn insert_persists_user_with_initial_permissions() {
    let (core, entry) = TestCore::new(
        "database::entities::user::tests::insert_persists_user_with_initial_permissions".into(),
    )
    .await
    .unwrap();
    let username = format!("permission-test-{}", entry.id);
    sqlx::query("DELETE FROM users WHERE username = $1")
        .bind(&username)
        .execute(&core.db)
        .await
        .unwrap();

    let inserted = serde_json::from_value::<NewUserRequest>(serde_json::json!({
        "name": "Permission Test",
        "username": username,
        "email": null,
        "password": null,
        "permissions": {
            "admin": false,
            "user_manager": true,
            "system_manager": false,
            "default_repository_actions": ["Read", "Edit"]
        }
    }))
    .unwrap()
    .insert(&core.db)
    .await
    .unwrap();

    let persisted = super::UserSafeData::get_by_id(inserted.id, &core.db)
        .await
        .unwrap()
        .unwrap();
    assert!(!persisted.admin);
    assert!(persisted.user_manager);
    assert!(!persisted.system_manager);
    assert_eq!(
        persisted.default_repository_actions,
        vec![RepositoryActions::Read, RepositoryActions::Edit]
    );

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(inserted.id)
        .execute(&core.db)
        .await
        .unwrap();
    entry.set_success(&core.db).await.unwrap();
}
