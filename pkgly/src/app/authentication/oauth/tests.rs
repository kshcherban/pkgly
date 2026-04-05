#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

fn google_settings() -> OAuth2Settings {
    let mut settings = OAuth2Settings::default();
    settings.enabled = true;
    settings.redirect_base_url = Some("https://app.example.com".to_string());
    settings.google = Some(OAuth2GoogleConfig {
        client_id: "client-id".to_string(),
        client_secret: "client-secret".to_string(),
        scopes: vec!["openid".to_string(), "profile".to_string()],
        redirect_path: None,
    });
    settings
}

#[test]
fn normalize_scopes_returns_defaults_when_empty() {
    let scopes = Vec::<String>::new();
    let normalized = normalize_scopes(&scopes);
    assert_eq!(normalized, vec!["openid", "profile", "email"]);
}

#[test]
fn normalize_scopes_deduplicates_and_preserves_order() {
    let scopes = vec![
        "email".to_string(),
        "profile".to_string(),
        "email".to_string(),
        String::new(),
    ];
    let normalized = normalize_scopes(&scopes);
    assert_eq!(normalized, vec!["email", "profile"]);
}

#[tokio::test]
async fn oauth_service_generates_authorization_url() {
    let settings = google_settings();
    let service = OAuth2Service::new(settings.clone())
        .expect("service construction should succeed")
        .expect("service should be available");

    let redirect = service
        .begin_authorization(
            OAuth2ProviderKind::Google,
            settings.redirect_base_url.as_deref(),
            Some("/welcome".to_string()),
        )
        .expect("authorization URL should be generated");

    assert!(
        redirect
            .authorization_url
            .as_str()
            .starts_with("https://accounts.google.com")
    );
    assert!(!redirect.state.is_empty());
}

#[test]
fn oauth_service_rejects_missing_providers() {
    let mut settings = OAuth2Settings::default();
    settings.enabled = true;
    let result = OAuth2Service::new(settings);
    assert!(result.is_err());
}
