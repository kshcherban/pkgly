#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::app::config::TokenSource;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, Header, encode};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, rand_core::OsRng, traits::PublicKeyParts};
use serde::Serialize;

#[derive(Clone)]
struct StaticFetcher {
    doc: JwkDocument,
    calls: Arc<AtomicUsize>,
}

impl StaticFetcher {
    fn new(doc: JwkDocument) -> Self {
        Self {
            doc,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl JwksFetcher for StaticFetcher {
    async fn fetch(&self, _url: &str) -> Result<JwkDocument, JwksError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.doc.clone())
    }
}

#[async_trait]
impl JwksResolver for StaticFetcher {
    async fn discover_jwks_url(&self, _issuer: &str) -> Result<String, JwksError> {
        Err(JwksError::MissingJwksUrl)
    }
}

#[derive(Debug, Serialize)]
struct TestClaims {
    sub: String,
    email: String,
    iss: String,
    aud: String,
    exp: usize,
    name: String,
}

fn generate_rsa_material(kid: &str) -> anyhow::Result<(jsonwebtoken::EncodingKey, JwkDocument)> {
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
    let encoding_key = jsonwebtoken::EncodingKey::from_rsa_der(der.as_bytes());
    Ok((encoding_key, JwkDocument { keys: vec![jwk] }))
}

fn sign_test_token(
    kid: &str,
    encoding_key: &jsonwebtoken::EncodingKey,
    issuer: &str,
    audience: &str,
) -> anyhow::Result<String> {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| anyhow::anyhow!(err))?
        .as_secs();
    let claims = TestClaims {
        sub: "user-123".to_string(),
        email: "user@example.com".to_string(),
        iss: issuer.to_string(),
        aud: audience.to_string(),
        exp: (now + 3600) as usize,
        name: "Test User".to_string(),
    };

    let token = encode(&header, &claims, encoding_key)?;
    Ok(token)
}

fn provider_config(name: &str, issuer: &str, audience: &str, jwks_url: &str) -> OidcProviderConfig {
    OidcProviderConfig {
        name: name.to_string(),
        issuer: issuer.to_string(),
        audience: audience.to_string(),
        jwks_url: Some(jwks_url.to_string()),
        token_source: TokenSource::default(),
        subject_claim: None,
        email_claim: None,
        display_name_claim: None,
        role_claims: Vec::new(),
    }
}

#[tokio::test]
async fn jwks_manager_caches_key_until_ttl() -> anyhow::Result<()> {
    let kid = "key-id";
    let issuer = "https://issuer.example";
    let audience = "pkgly";
    let jwks_url = "https://issuer.example/keys";
    let (encoding_key, jwks) = generate_rsa_material(kid)?;
    let token = sign_test_token(kid, &encoding_key, issuer, audience)?;
    let fetcher = StaticFetcher::new(jwks);
    let manager = JwksManager::new(fetcher.clone(), Duration::from_secs(3600));
    let provider = provider_config("test", issuer, audience, jwks_url);

    let claims = manager.verify(&token, &provider).await?;
    assert_eq!(claims.get("sub").and_then(|v| v.as_str()), Some("user-123"));
    let first_calls = fetcher.calls();

    let claims_second = manager.verify(&token, &provider).await?;
    assert_eq!(
        claims_second.get("email").and_then(|v| v.as_str()),
        Some("user@example.com")
    );
    assert_eq!(fetcher.calls(), first_calls, "JWKS fetch should be cached");
    Ok(())
}
