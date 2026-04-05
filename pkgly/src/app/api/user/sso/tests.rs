#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::{
    SsoSettings, build_user_email, extract_principal, normalize_username, sanitize_redirect,
};
use crate::app::authentication::jwks::{
    JwkDocument, JwkKey, JwksError, JwksFetcher, JwksManager, JwksResolver,
};
use crate::app::config::{OidcProviderConfig, TokenSource};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http::{HeaderMap, HeaderValue};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, rand_core::OsRng, traits::PublicKeyParts};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn build_user_email_uses_raw_value_when_valid() {
    let email = build_user_email(Some("user@example.com"), "username").unwrap();
    assert_eq!(email.to_string(), "user@example.com");
}

#[test]
fn build_user_email_generates_fallback_with_domain() {
    let email = build_user_email(None, "ab").unwrap();
    assert!(email.to_string().starts_with("abusr"));
    assert!(email.to_string().ends_with("@sso.local"));
}

#[tokio::test]
async fn oidc_provider_principal_is_used_when_token_present() -> anyhow::Result<()> {
    let kid = "kid-1";
    let issuer = "https://issuer.example";
    let audience = "pkgly";
    let (encoding_key, jwks) = generate_rsa_material(kid)?;
    let token = sign_test_token(kid, &encoding_key, issuer, audience)?;

    let fetcher = StaticFetcher::new(jwks);
    let manager = JwksManager::new(fetcher, Duration::from_secs(3600));

    let settings = SsoSettings {
        enabled: true,
        providers: vec![OidcProviderConfig {
            name: "example".into(),
            issuer: issuer.into(),
            audience: audience.into(),
            jwks_url: Some("https://issuer.example/keys".into()),
            token_source: TokenSource::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
            subject_claim: None,
            email_claim: None,
            display_name_claim: None,
            role_claims: vec!["roles".into()],
        }],
        ..SsoSettings::default()
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );

    let principal = extract_principal(&manager, &settings, &headers)
        .await
        .expect("principal");

    assert_eq!(principal.username, normalize_username("user-123"));
    assert_eq!(principal.email.as_deref(), Some("user@example.com"));
    assert_eq!(principal.display_name, "Test User");
    assert!(principal.roles.contains(&"admin".to_string()));
    Ok(())
}
#[tokio::test]
async fn sanitize_redirect_rejects_external_urls() {
    let external = sanitize_redirect(Some("https://example.com"));
    assert_eq!(external, HeaderValue::from_static("/"));
}

#[derive(Clone)]
struct StaticFetcher {
    doc: JwkDocument,
}

impl StaticFetcher {
    fn new(doc: JwkDocument) -> Self {
        Self { doc }
    }
}

#[async_trait]
impl JwksFetcher for StaticFetcher {
    async fn fetch(&self, _url: &str) -> Result<JwkDocument, JwksError> {
        Ok(self.doc.clone())
    }
}

#[async_trait]
impl JwksResolver for StaticFetcher {
    async fn discover_jwks_url(&self, _issuer: &str) -> Result<String, JwksError> {
        Err(JwksError::MissingJwksUrl)
    }
}

fn generate_rsa_material(kid: &str) -> anyhow::Result<(EncodingKey, JwkDocument)> {
    let mut rng = OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048)?;
    let n = URL_SAFE_NO_PAD.encode(private_key.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(private_key.e().to_bytes_be());
    let jwk = JwkKey {
        kid: kid.to_string(),
        kty: Some("RSA".to_string()),
        n: Some(n),
        e: Some(e),
        x: None,
        y: None,
        crv: None,
    };
    let der = private_key.to_pkcs1_der()?;
    let encoding_key = EncodingKey::from_rsa_der(der.as_bytes());
    Ok((encoding_key, JwkDocument { keys: vec![jwk] }))
}

fn sign_test_token(
    kid: &str,
    encoding_key: &EncodingKey,
    issuer: &str,
    audience: &str,
) -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    struct Claims {
        sub: String,
        email: String,
        iss: String,
        aud: String,
        exp: usize,
        name: String,
        roles: Vec<String>,
    }

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let exp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + 900;
    let claims = Claims {
        sub: "user-123".into(),
        email: "user@example.com".into(),
        iss: issuer.into(),
        aud: audience.into(),
        exp: exp as usize,
        name: "Test User".into(),
        roles: vec!["admin".into(), "editor".into()],
    };
    let token = encode(&header, &claims, encoding_key)?;
    Ok(token)
}
