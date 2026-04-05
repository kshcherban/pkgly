#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::app::config::OAuth2ProviderKind;

#[test]
fn merge_google_settings_keeps_existing_secret_when_omitted() {
    let current = OAuth2GoogleConfig {
        client_id: "google-client".into(),
        client_secret: "super-secret".into(),
        scopes: vec!["openid".into(), "email".into()],
        redirect_path: None,
    };
    let request = Some(OAuth2ProviderSettingsRequest {
        client_id: "google-client".into(),
        client_secret: None,
        scopes: vec!["openid".into(), "email".into()],
        redirect_path: None,
    });

    let merged = merge_google_settings(Some(&current), request).expect("merge success");
    let new_config = merged.expect("google config");

    assert_eq!(new_config.client_id, "google-client");
    assert_eq!(new_config.client_secret, "super-secret");
    assert_eq!(
        new_config.scopes,
        vec!["openid".to_string(), "email".to_string()]
    );
}

#[test]
fn merge_google_settings_requires_secret_initially() {
    let request = Some(OAuth2ProviderSettingsRequest {
        client_id: "google-client".into(),
        client_secret: None,
        scopes: vec!["openid".into()],
        redirect_path: None,
    });

    let merged = merge_google_settings(None, request);

    assert!(merged.is_err());
}

#[test]
fn merge_oauth2_settings_sanitizes_paths_and_mappings() {
    let current = OAuth2Settings {
        enabled: false,
        login_path: "/api/user/oauth2/login".into(),
        callback_path: "/api/user/oauth2/callback".into(),
        redirect_base_url: None,
        auto_create_users: false,
        google: Some(OAuth2GoogleConfig {
            client_id: "google-client".into(),
            client_secret: "secret".into(),
            scopes: vec!["openid".into()],
            redirect_path: None,
        }),
        microsoft: None,
        casbin: None,
        group_role_mappings: vec![],
    };

    let request = OAuth2SettingsRequest {
        enabled: true,
        login_path: "oauth/login".into(),
        callback_path: "/oauth/callback".into(),
        redirect_base_url: Some("  ".into()),
        auto_create_users: true,
        google: Some(OAuth2ProviderSettingsRequest {
            client_id: "google-client".into(),
            client_secret: None,
            scopes: vec!["OpenID".into(), "email".into(), "".into()],
            redirect_path: Some("callback".into()),
        }),
        microsoft: None,
        casbin: None,
        group_role_mappings: vec![
            OAuth2GroupRoleMapping {
                provider: OAuth2ProviderKind::Google,
                group: "engineering".into(),
                roles: vec!["admin".into(), "admin".into(), " ".into()],
            },
            OAuth2GroupRoleMapping {
                provider: OAuth2ProviderKind::Google,
                group: "   ".into(),
                roles: vec!["ignored".into()],
            },
        ],
    };

    let merged =
        merge_oauth2_settings(Some(&current), request).expect("settings should merge safely");

    assert_eq!(merged.login_path, "/oauth/login");
    assert_eq!(merged.callback_path, "/oauth/callback");
    assert!(merged.redirect_base_url.is_none());
    assert!(merged.google.is_some());
    assert_eq!(merged.group_role_mappings.len(), 1);
    let mapping = &merged.group_role_mappings[0];
    assert_eq!(mapping.group, "engineering");
    assert_eq!(mapping.roles, vec!["admin"]);
    assert_eq!(
        merged
            .google
            .as_ref()
            .and_then(|cfg| cfg.redirect_path.clone())
            .as_deref(),
        Some("callback")
    );
    assert_eq!(
        merged.google.as_ref().map(|cfg| cfg.scopes.clone()),
        Some(vec!["OpenID".into(), "email".into()])
    );
}

#[test]
fn sanitize_sso_settings_trims_and_validates_providers() {
    let settings = SsoSettings {
        enabled: true,
        login_path: "sso/login".into(),
        login_button_text: "   ".into(),
        provider_login_url: Some(" https://login.example.com ".into()),
        provider_redirect_param: Some(" redirect ".into()),
        auto_create_users: true,
        providers: vec![OidcProviderConfig {
            name: " cloudflare ".into(),
            issuer: " https://issuer.example.com/ ".into(),
            audience: " pkgly ".into(),
            jwks_url: Some(" https://issuer.example.com/certs ".into()),
            token_source: TokenSource::Header {
                name: " Cf-Access-Jwt-Assertion ".into(),
                prefix: Some("Bearer ".into()),
            },
            subject_claim: Some(" preferred_username ".into()),
            email_claim: None,
            display_name_claim: Some(" name ".into()),
            role_claims: vec![" roles ".into()],
        }],
        role_claims: vec![" roles ".into(), "".into()],
    };

    let sanitized = sanitize_sso_settings(settings).expect("valid config");
    assert_eq!(sanitized.login_path, "/sso/login");
    assert_eq!(sanitized.login_button_text, "Sign in with SSO");
    assert_eq!(
        sanitized.provider_login_url.as_deref(),
        Some("https://login.example.com")
    );
    assert_eq!(
        sanitized.provider_redirect_param.as_deref(),
        Some("redirect")
    );
    assert_eq!(sanitized.providers.len(), 1);
    let provider = &sanitized.providers[0];
    assert_eq!(provider.name, "cloudflare");
    assert_eq!(provider.issuer, "https://issuer.example.com");
    assert_eq!(provider.audience, "pkgly");
    assert_eq!(
        provider.jwks_url.as_deref(),
        Some("https://issuer.example.com/certs")
    );
    assert_eq!(sanitized.role_claims, vec!["roles"]);
}

#[test]
fn sanitize_sso_settings_rejects_invalid_provider() {
    let settings = SsoSettings {
        enabled: true,
        login_path: "/api/user/sso/login".into(),
        login_button_text: "SSO".into(),
        provider_login_url: None,
        provider_redirect_param: None,
        auto_create_users: false,
        providers: vec![OidcProviderConfig {
            name: "".into(),
            issuer: "".into(),
            audience: "".into(),
            jwks_url: None,
            token_source: TokenSource::Cookie { name: "".into() },
            subject_claim: None,
            email_claim: None,
            display_name_claim: None,
            role_claims: vec![],
        }],
        role_claims: vec![],
    };

    let result = sanitize_sso_settings(settings);
    assert!(result.is_err());
}
