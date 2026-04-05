use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use ahash::{HashMap, HashMapExt};

use oauth2::basic::{
    BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse,
    BasicTokenType,
};
use oauth2::url::Url;
use oauth2::{
    AuthUrl, AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, ExtraTokenFields, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    StandardRevocableToken, StandardTokenResponse, TokenUrl,
};
use parking_lot::Mutex;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tracing::{trace, warn};

use crate::app::config::{
    OAuth2GoogleConfig, OAuth2MicrosoftConfig, OAuth2ProviderKind, OAuth2Settings,
};

mod rbac;
pub use rbac::OAuth2Rbac;

const STATE_TTL: Duration = Duration::from_secs(300);

type OAuthClient<
    HasAuthUrl = EndpointNotSet,
    HasDeviceAuthUrl = EndpointNotSet,
    HasIntrospectionUrl = EndpointNotSet,
    HasRevocationUrl = EndpointNotSet,
    HasTokenUrl = EndpointNotSet,
> = Client<
    BasicErrorResponse,
    OAuthTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    HasAuthUrl,
    HasDeviceAuthUrl,
    HasIntrospectionUrl,
    HasRevocationUrl,
    HasTokenUrl,
>;

type OAuthTokenResponse = StandardTokenResponse<OidcTokenExtraFields, BasicTokenType>;
type ConfiguredOAuthClient =
    OAuthClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

#[derive(Debug, Error)]
pub enum OAuth2ServiceError {
    #[error("OAuth2 is not enabled")]
    Disabled,
    #[error("OAuth2 is enabled but no providers are configured")]
    MissingProviders,
    #[error("OAuth2 provider {0} is not configured")]
    ProviderNotConfigured(OAuth2ProviderKind),
    #[error("OAuth2 redirect URL is invalid: {0}")]
    InvalidRedirectUrl(String),
    #[error("OAuth2 login state is invalid or expired")]
    InvalidState,
    #[error("Failed to construct OAuth2 client: {0}")]
    ClientConstruction(String),
    #[error("OAuth2 token request failed: {0}")]
    TokenRequestFailed(String),
}

#[derive(Clone)]
pub struct OAuth2Service {
    settings: OAuth2Settings,
    providers: HashMap<OAuth2ProviderKind, OAuth2ProviderRuntime>,
    state_store: Arc<OAuthStateStore>,
    http_client: HttpClient,
}

impl OAuth2Service {
    pub fn new(settings: OAuth2Settings) -> Result<Option<Self>, OAuth2ServiceError> {
        if !settings.enabled {
            return Ok(None);
        }

        let mut providers = HashMap::new();

        if let Some(cfg) = settings.google.as_ref() {
            if cfg.client_id.is_empty() || cfg.client_secret.is_empty() {
                warn!("Google OAuth2 provider is configured but missing client credentials");
            } else {
                let runtime = OAuth2ProviderRuntime::new_google(cfg, &settings)
                    .map_err(OAuth2ServiceError::ClientConstruction)?;
                providers.insert(OAuth2ProviderKind::Google, runtime);
            }
        }

        if let Some(cfg) = settings.microsoft.as_ref() {
            if cfg.client_id.is_empty() || cfg.client_secret.is_empty() {
                warn!("Microsoft OAuth2 provider is configured but missing client credentials");
            } else {
                let runtime = OAuth2ProviderRuntime::new_microsoft(cfg, &settings)
                    .map_err(OAuth2ServiceError::ClientConstruction)?;
                providers.insert(OAuth2ProviderKind::Microsoft, runtime);
            }
        }

        if providers.is_empty() {
            warn!("OAuth2 is enabled but no valid providers were configured");
            return Err(OAuth2ServiceError::MissingProviders);
        }

        Ok(Some(Self {
            settings,
            providers,
            state_store: Arc::new(OAuthStateStore::default()),
            http_client: HttpClient::new(),
        }))
    }

    pub fn settings(&self) -> &OAuth2Settings {
        &self.settings
    }

    pub fn providers(&self) -> impl Iterator<Item = OAuth2ProviderKind> + '_ {
        self.providers.keys().copied()
    }

    pub fn export_state(&self, state: &str) -> Option<OAuthStateExport> {
        self.state_store.export(state)
    }

    fn provider_config(
        &self,
        provider: OAuth2ProviderKind,
    ) -> Result<&OAuth2ProviderRuntime, OAuth2ServiceError> {
        self.providers
            .get(&provider)
            .ok_or(OAuth2ServiceError::ProviderNotConfigured(provider))
    }

    pub fn begin_authorization(
        &self,
        provider: OAuth2ProviderKind,
        base_url: Option<&str>,
        redirect: Option<String>,
    ) -> Result<AuthorizationRedirect, OAuth2ServiceError> {
        let runtime = self.provider_config(provider)?;
        let redirect_url = self
            .build_redirect_url(runtime, base_url)
            .map_err(OAuth2ServiceError::InvalidRedirectUrl)?;

        let client = runtime.client.clone().set_redirect_uri(
            RedirectUrl::new(redirect_url.to_string())
                .map_err(|err| OAuth2ServiceError::InvalidRedirectUrl(err.to_string()))?,
        );

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client.authorize_url(CsrfToken::new_random);
        for scope in runtime.scopes.iter() {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        let (auth_url, csrf_token) = request.set_pkce_challenge(pkce_challenge).url();

        trace!(
            provider = %runtime.provider,
            redirect = %redirect_url,
            "OAuth2 authorization URL generated"
        );

        self.state_store.insert(
            csrf_token.secret().to_string(),
            OAuthStateValue {
                provider,
                pkce_verifier,
                redirect,
                created_at: Instant::now(),
            },
        );

        Ok(AuthorizationRedirect {
            provider,
            authorization_url: auth_url,
            state: csrf_token.secret().to_string(),
        })
    }

    pub async fn exchange_code(
        &self,
        base_url: Option<&str>,
        code: AuthorizationCode,
        state: &str,
    ) -> Result<OAuth2Exchange, OAuth2ServiceError> {
        let Some(state_value) = self.state_store.take(state) else {
            return Err(OAuth2ServiceError::InvalidState);
        };
        let provider = state_value.provider;
        let runtime = self.provider_config(provider)?;
        let redirect_url = self
            .build_redirect_url(runtime, base_url)
            .map_err(OAuth2ServiceError::InvalidRedirectUrl)?;

        let client = runtime.client.clone().set_redirect_uri(
            RedirectUrl::new(redirect_url.to_string())
                .map_err(|err| OAuth2ServiceError::InvalidRedirectUrl(err.to_string()))?,
        );

        trace!(
            provider = %runtime.provider,
            redirect = %redirect_url,
            "Exchanging OAuth2 authorization code"
        );

        let token_response = client
            .exchange_code(code)
            .set_pkce_verifier(state_value.pkce_verifier)
            .request_async(&self.http_client)
            .await
            .map_err(|err| OAuth2ServiceError::TokenRequestFailed(err.to_string()))?;

        Ok(OAuth2Exchange {
            provider,
            token_response,
            redirect: state_value.redirect,
        })
    }

    pub async fn exchange_code_with_export(
        &self,
        base_url: Option<&str>,
        code: AuthorizationCode,
        export: OAuthStateExport,
    ) -> Result<OAuth2Exchange, OAuth2ServiceError> {
        let OAuthStateExport {
            provider,
            pkce_verifier,
            redirect,
        } = export;
        let runtime = self.provider_config(provider)?;
        let redirect_url = self
            .build_redirect_url(runtime, base_url)
            .map_err(OAuth2ServiceError::InvalidRedirectUrl)?;

        let client = runtime.client.clone().set_redirect_uri(
            RedirectUrl::new(redirect_url.to_string())
                .map_err(|err| OAuth2ServiceError::InvalidRedirectUrl(err.to_string()))?,
        );

        trace!(
            provider = %runtime.provider,
            redirect = %redirect_url,
            "Exchanging OAuth2 authorization code via restored state"
        );

        let pkce_verifier = PkceCodeVerifier::new(pkce_verifier);

        let token_response = client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(&self.http_client)
            .await
            .map_err(|err| OAuth2ServiceError::TokenRequestFailed(err.to_string()))?;

        Ok(OAuth2Exchange {
            provider,
            token_response,
            redirect,
        })
    }

    fn build_redirect_url(
        &self,
        runtime: &OAuth2ProviderRuntime,
        base_url: Option<&str>,
    ) -> Result<Url, String> {
        let raw_path = runtime.redirect_path.as_str();
        if raw_path.starts_with("http://") || raw_path.starts_with("https://") {
            return Url::parse(raw_path)
                .map_err(|err| format!("Failed to parse redirect URL: {err}"));
        }

        let Some(base) = self
            .settings
            .redirect_base_url
            .as_deref()
            .or(base_url)
            .filter(|base| !base.is_empty())
        else {
            return Err("Redirect base URL could not be determined".to_string());
        };

        let trimmed_path = if raw_path.starts_with('/') {
            raw_path.to_string()
        } else {
            format!("/{raw_path}")
        };

        let base_url =
            Url::parse(base).map_err(|err| format!("Invalid redirect base URL '{base}': {err}"))?;
        base_url
            .join(&trimmed_path)
            .map_err(|err| format!("Failed to combine base URL and path: {err}"))
    }
}

#[derive(Debug, Clone)]
pub struct AuthorizationRedirect {
    pub provider: OAuth2ProviderKind,
    pub authorization_url: Url,
    pub state: String,
}

#[derive(Debug)]
pub struct OAuth2Exchange {
    pub provider: OAuth2ProviderKind,
    pub token_response: OAuthTokenResponse,
    pub redirect: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthStateExport {
    pub provider: OAuth2ProviderKind,
    pub pkce_verifier: String,
    pub redirect: Option<String>,
}

#[derive(Default)]
struct OAuthStateStore {
    entries: Mutex<HashMap<String, OAuthStateValue>>,
}

impl OAuthStateStore {
    fn insert(&self, state: String, value: OAuthStateValue) {
        let mut entries = self.entries.lock();
        purge_expired_locked(&mut entries);
        entries.insert(state, value);
    }

    fn take(&self, state: &str) -> Option<OAuthStateValue> {
        let mut entries = self.entries.lock();
        purge_expired_locked(&mut entries);
        entries.remove(state)
    }

    fn export(&self, state: &str) -> Option<OAuthStateExport> {
        let mut entries = self.entries.lock();
        purge_expired_locked(&mut entries);
        entries.get(state).map(|value| OAuthStateExport {
            provider: value.provider,
            pkce_verifier: value.pkce_verifier.secret().to_string(),
            redirect: value.redirect.clone(),
        })
    }
}

fn purge_expired_locked(entries: &mut HashMap<String, OAuthStateValue>) {
    let now = Instant::now();
    entries.retain(|_, value| now.duration_since(value.created_at) < STATE_TTL);
}

struct OAuthStateValue {
    provider: OAuth2ProviderKind,
    pkce_verifier: PkceCodeVerifier,
    redirect: Option<String>,
    created_at: Instant,
}

#[derive(Clone)]
struct OAuth2ProviderRuntime {
    provider: OAuth2ProviderKind,
    client: ConfiguredOAuthClient,
    scopes: Vec<String>,
    redirect_path: String,
}

impl OAuth2ProviderRuntime {
    fn new_google(config: &OAuth2GoogleConfig, settings: &OAuth2Settings) -> Result<Self, String> {
        let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
            .map_err(|err| format!("Invalid Google authorization URL: {err}"))?;
        let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".to_string())
            .map_err(|err| format!("Invalid Google token URL: {err}"))?;

        let client = OAuthClient::new(ClientId::new(config.client_id.clone()))
            .set_client_secret(ClientSecret::new(config.client_secret.clone()))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url);

        let redirect_path = config
            .redirect_path
            .clone()
            .unwrap_or_else(|| settings.callback_path.clone());

        Ok(Self {
            provider: OAuth2ProviderKind::Google,
            client,
            scopes: normalize_scopes(&config.scopes),
            redirect_path,
        })
    }

    fn new_microsoft(
        config: &OAuth2MicrosoftConfig,
        settings: &OAuth2Settings,
    ) -> Result<Self, String> {
        let tenant = config.tenant_id.as_deref().unwrap_or("common");
        let auth_url = AuthUrl::new(format!(
            "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"
        ))
        .map_err(|err| format!("Invalid Microsoft authorization URL: {err}"))?;
        let token_url = TokenUrl::new(format!(
            "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"
        ))
        .map_err(|err| format!("Invalid Microsoft token URL: {err}"))?;

        let client = OAuthClient::new(ClientId::new(config.client_id.clone()))
            .set_client_secret(ClientSecret::new(config.client_secret.clone()))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url);

        let redirect_path = config
            .redirect_path
            .clone()
            .unwrap_or_else(|| settings.callback_path.clone());

        Ok(Self {
            provider: OAuth2ProviderKind::Microsoft,
            client,
            scopes: normalize_scopes(&config.scopes),
            redirect_path,
        })
    }
}

pub(crate) fn normalize_scopes(scopes: &[String]) -> Vec<String> {
    if scopes.is_empty() {
        return vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
        ];
    }
    let mut dedup = Vec::with_capacity(scopes.len());
    for scope in scopes {
        if scope.is_empty() {
            continue;
        }
        if !dedup.iter().any(|existing: &String| existing == scope) {
            dedup.push(scope.clone());
        }
    }
    if dedup.is_empty() {
        dedup.extend(
            ["openid", "profile", "email"]
                .into_iter()
                .map(str::to_string),
        );
    }
    dedup
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OidcTokenExtraFields {
    #[serde(rename = "id_token", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(
        flatten,
        default,
        skip_serializing_if = "OidcTokenExtraFields::map_is_empty"
    )]
    pub additional: HashMap<String, Value>,
}

impl OidcTokenExtraFields {
    fn map_is_empty(map: &HashMap<String, Value>) -> bool {
        map.is_empty()
    }
}

impl ExtraTokenFields for OidcTokenExtraFields {}

#[cfg(test)]
mod tests;
