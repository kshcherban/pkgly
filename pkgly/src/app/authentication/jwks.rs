use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::app::config::OidcProviderConfig;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum JwksError {
    #[error("token missing kid header")]
    MissingKid,
    #[error("jwks url missing and discovery unavailable")]
    MissingJwksUrl,
    #[error("jwks fetch failed: {0}")]
    FetchFailed(String),
    #[error("jwks parse failed: {0}")]
    InvalidJwks(String),
    #[error("kid not found in jwks")]
    KidNotFound,
    #[error("token validation failed: {0}")]
    ValidationFailed(String),
}

#[derive(Debug, Deserialize, Clone)]
pub struct JwkDocument {
    pub keys: Vec<JwkKey>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JwkKey {
    pub kid: String,
    #[serde(default)]
    pub kty: Option<String>,
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub e: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default)]
    pub crv: Option<String>,
}

#[async_trait]
pub trait JwksFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<JwkDocument, JwksError>;
}

#[async_trait]
pub trait JwksResolver: Send + Sync {
    async fn discover_jwks_url(&self, issuer: &str) -> Result<String, JwksError>;
}

#[derive(Clone)]
pub struct JwksManager<F: JwksFetcher + JwksResolver + Clone> {
    fetcher: F,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

#[derive(Clone)]
struct CacheEntry {
    fetched_at: Instant,
    keys: HashMap<String, DecodingKey>,
}

impl<F: JwksFetcher + JwksResolver + Clone> JwksManager<F> {
    pub fn new(fetcher: F, ttl: Duration) -> Self {
        Self {
            fetcher,
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    pub async fn verify(
        &self,
        token: &str,
        provider: &OidcProviderConfig,
    ) -> Result<serde_json::Map<String, serde_json::Value>, JwksError> {
        let header =
            decode_header(token).map_err(|err| JwksError::ValidationFailed(err.to_string()))?;
        let kid = header.kid.ok_or(JwksError::MissingKid)?;
        let jwks_url = if let Some(url) = provider.jwks_url.as_deref() {
            url.to_string()
        } else {
            self.fetcher.discover_jwks_url(&provider.issuer).await?
        };

        let decoding_key = self.decoding_key(&provider.issuer, &jwks_url, &kid).await?;
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[provider.audience.as_str()]);
        validation.set_issuer(&[provider.issuer.as_str()]);
        // CRITICAL: Enforce that these claims MUST be present in the token
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);

        let token_data =
            decode::<serde_json::Map<String, serde_json::Value>>(token, &decoding_key, &validation)
                .map_err(|err| JwksError::ValidationFailed(err.to_string()))?;

        Ok(token_data.claims)
    }

    pub async fn decoding_key(
        &self,
        issuer: &str,
        jwks_url: &str,
        kid: &str,
    ) -> Result<DecodingKey, JwksError> {
        if let Some(key) = self.cached_key(issuer, kid).await {
            return Ok(key);
        }

        let document = self.fetcher.fetch(jwks_url).await?;
        let mut map = HashMap::new();
        for jwk in document.keys {
            if let (Some(n), Some(e)) = (jwk.n.as_deref(), jwk.e.as_deref()) {
                if let Ok(key) = DecodingKey::from_rsa_components(n, e) {
                    map.insert(jwk.kid, key);
                }
            }
        }

        let target = map.get(kid).cloned().ok_or(JwksError::KidNotFound)?;
        let mut cache = self.cache.write().await;
        cache.insert(
            issuer.to_string(),
            CacheEntry {
                fetched_at: Instant::now(),
                keys: map,
            },
        );
        Ok(target)
    }

    async fn cached_key(&self, issuer: &str, kid: &str) -> Option<DecodingKey> {
        let cache = self.cache.read().await;
        let entry = cache.get(issuer)?;
        if entry.fetched_at.elapsed() > self.ttl {
            return None;
        }
        entry.keys.get(kid).cloned()
    }
}

#[derive(Clone, Debug)]
pub struct ReqwestJwksFetcher {
    client: reqwest::Client,
}

impl ReqwestJwksFetcher {
    pub fn new() -> Result<Self, JwksError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|err| JwksError::FetchFailed(err.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl JwksFetcher for ReqwestJwksFetcher {
    async fn fetch(&self, url: &str) -> Result<JwkDocument, JwksError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|err| JwksError::FetchFailed(err.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(JwksError::FetchFailed(format!("jwks http status {status}")));
        }
        response
            .json::<JwkDocument>()
            .await
            .map_err(|err| JwksError::InvalidJwks(err.to_string()))
    }
}

#[async_trait]
impl JwksResolver for ReqwestJwksFetcher {
    async fn discover_jwks_url(&self, issuer: &str) -> Result<String, JwksError> {
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        );
        let response = self
            .client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|err| JwksError::FetchFailed(err.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(JwksError::FetchFailed(format!(
                "discovery http status {status}"
            )));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|err| JwksError::InvalidJwks(err.to_string()))?;
        if let Some(uri) = payload.get("jwks_uri").and_then(|value| value.as_str()) {
            return Ok(uri.to_string());
        }
        Err(JwksError::MissingJwksUrl)
    }
}

#[cfg(test)]
mod tests;
