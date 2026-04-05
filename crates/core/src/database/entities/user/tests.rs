#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

/// Just incase a bug get's introduced from serde where the password is serialized. We want to error out.
#[test]
pub fn assert_no_serialize_password() {
    let user = super::User {
        password: Some("password".to_owned()),
        id: Default::default(),
        name: Default::default(),
        username: "username".parse().unwrap(),
        email: "email@email.com".parse().unwrap(),
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
