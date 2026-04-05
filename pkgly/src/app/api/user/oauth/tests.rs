#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn extract_roles_combines_roles_and_groups() {
    let claims = IdTokenClaims {
        sub: "user-1".to_string(),
        email: Some("user@example.com".to_string()),
        preferred_username: None,
        name: None,
        given_name: None,
        roles: Some(vec!["admin".to_string(), "admin".to_string()]),
        groups: Some(vec!["team-a".to_string()]),
    };

    let roles = extract_roles(OAuth2ProviderKind::Microsoft, &claims);
    assert_eq!(roles, vec!["admin", "team-a"]);
}

#[test]
fn extract_roles_adds_google_fallback_when_missing() {
    let claims = IdTokenClaims {
        sub: "user-2".to_string(),
        email: Some("user@example.com".to_string()),
        preferred_username: None,
        name: None,
        given_name: None,
        roles: None,
        groups: None,
    };

    let roles = extract_roles(OAuth2ProviderKind::Google, &claims);
    assert_eq!(roles, vec!["group:user@example.com".to_string()]);
}

#[test]
fn map_roles_from_claims_matches_configured_groups() {
    let claims = vec!["GROUP-Admins".to_string(), "team-engineering".to_string()];
    let mappings = vec![
        OAuth2GroupRoleMapping {
            provider: OAuth2ProviderKind::Microsoft,
            group: "group-admins".to_string(),
            roles: vec!["admin".to_string()],
        },
        OAuth2GroupRoleMapping {
            provider: OAuth2ProviderKind::Google,
            group: "team-engineering".to_string(),
            roles: vec!["engineering".to_string()],
        },
    ];

    let result = map_roles_from_claims(OAuth2ProviderKind::Microsoft, &claims, &mappings);
    assert_eq!(result, vec!["admin".to_string()]);
}

#[test]
fn oauth_denied_redirect_sets_location_header() {
    let response = super::oauth_denied_redirect("invalid_state");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok());
    assert_eq!(location, Some("/oauth/denied?reason=invalid_state"));
}
