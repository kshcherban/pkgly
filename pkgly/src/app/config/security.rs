use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const DEFAULT_CASBIN_MODEL: &str = include_str!("../../../resources/rbac/model.conf");
const DEFAULT_CASBIN_POLICY: &str = include_str!("../../../resources/rbac/policy.csv");

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SecuritySettings {
    pub allow_basic_without_tokens: bool,
    pub password_rules: Option<PasswordRules>,
    pub sso: Option<SsoSettings>,
    pub oauth2: Option<OAuth2Settings>,
}
impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            allow_basic_without_tokens: false,
            password_rules: Some(PasswordRules::default()),
            sso: None,
            oauth2: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct SsoSettings {
    /// Enable SSO support. Disabled configurations are ignored at runtime.
    pub enabled: bool,
    /// Path or URL the UI should direct to when initiating SSO.
    pub login_path: String,
    /// Text used for the SSO button in the UI.
    pub login_button_text: String,
    /// Optional external identity provider URL used to initiate the SSO flow.
    pub provider_login_url: Option<String>,
    /// Optional query parameter on the provider login URL that indicates where to redirect after authentication.
    pub provider_redirect_param: Option<String>,
    /// Automatically create a Pkgly account when the principal does not exist.
    pub auto_create_users: bool,
    /// Optional list of OIDC/JWT providers validated via JWKS.
    #[serde(default)]
    pub providers: Vec<OidcProviderConfig>,
    /// Optional list of JWT claim keys that contain role values to apply to Casbin.
    #[serde(default)]
    pub role_claims: Vec<String>,
}

impl Default for SsoSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            login_path: default_login_path(),
            login_button_text: default_login_button_text(),
            provider_login_url: None,
            provider_redirect_param: None,
            auto_create_users: false,
            providers: Vec::new(),
            role_claims: Vec::new(),
        }
    }
}

fn default_login_path() -> String {
    "/api/user/sso/login".to_string()
}

fn default_login_button_text() -> String {
    "Sign in with SSO".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenSource {
    Header {
        /// Header name that carries the bearer token.
        name: String,
        /// Optional prefix to strip (e.g., "Bearer ").
        #[serde(default)]
        prefix: Option<String>,
    },
    Cookie {
        /// Cookie name that carries the token.
        name: String,
    },
}

impl Default for TokenSource {
    fn default() -> Self {
        TokenSource::Header {
            name: "Authorization".to_string(),
            prefix: Some("Bearer ".to_string()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OidcProviderConfig {
    /// Friendly identifier for the provider (e.g., "cloudflare", "okta").
    pub name: String,
    /// Expected issuer claim.
    pub issuer: String,
    /// Expected audience/client ID.
    pub audience: String,
    /// Optional explicit JWKS endpoint; when omitted discovery will be used.
    pub jwks_url: Option<String>,
    /// Where to read the token from.
    pub token_source: TokenSource,
    /// Optional claim to use for username; defaults to preferred_username/sub.
    pub subject_claim: Option<String>,
    /// Optional claim to use for email; defaults to `email`.
    pub email_claim: Option<String>,
    /// Optional claim to use for display name; defaults to `name`.
    pub display_name_claim: Option<String>,
    /// Claims that contain role values applied to Casbin.
    #[serde(default)]
    pub role_claims: Vec<String>,
}

impl Default for OidcProviderConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            issuer: String::new(),
            audience: String::new(),
            jwks_url: None,
            token_source: TokenSource::default(),
            subject_claim: None,
            email_claim: None,
            display_name_claim: None,
            role_claims: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2Settings {
    /// Enable OAuth2 login support.
    pub enabled: bool,
    /// Public route to initiate the OAuth2 login flow.
    pub login_path: String,
    /// Public callback route that identity providers redirect to.
    pub callback_path: String,
    /// Optional base URL override for redirect URLs. When empty, the application attempts to infer the base URL.
    pub redirect_base_url: Option<String>,
    /// Automatically provision users that do not already exist.
    pub auto_create_users: bool,
    /// Google OAuth2/OpenID Connect configuration.
    pub google: Option<OAuth2GoogleConfig>,
    /// Microsoft Entra ID (Azure AD) OAuth2/OpenID Connect configuration.
    pub microsoft: Option<OAuth2MicrosoftConfig>,
    /// Optional Casbin configuration for RBAC policy enforcement.
    pub casbin: Option<OAuth2CasbinConfig>,
    /// Optional mapping between provider groups/roles and Pkgly Casbin roles.
    pub group_role_mappings: Vec<OAuth2GroupRoleMapping>,
}

impl Default for OAuth2Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            login_path: "/api/user/oauth2/login".to_string(),
            callback_path: "/api/user/oauth2/callback".to_string(),
            redirect_base_url: None,
            auto_create_users: false,
            google: None,
            microsoft: None,
            casbin: Some(OAuth2CasbinConfig::default()),
            group_role_mappings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2ProviderKind {
    Google,
    Microsoft,
}

impl fmt::Display for OAuth2ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            OAuth2ProviderKind::Google => "google",
            OAuth2ProviderKind::Microsoft => "microsoft",
        };
        write!(f, "{value}")
    }
}

impl FromStr for OAuth2ProviderKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "google" => Ok(OAuth2ProviderKind::Google),
            "microsoft" | "azure" | "azure_ad" | "entra" | "entra_id" => {
                Ok(OAuth2ProviderKind::Microsoft)
            }
            other => Err(format!("Unsupported OAuth2 provider '{other}'")),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2GoogleConfig {
    /// OAuth2 client identifier issued by Google.
    pub client_id: String,
    /// OAuth2 client secret issued by Google.
    pub client_secret: String,
    /// Additional scopes requested during authorization.
    pub scopes: Vec<String>,
    /// Optional explicit redirect path override.
    pub redirect_path: Option<String>,
}

impl Default for OAuth2GoogleConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            redirect_path: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2MicrosoftConfig {
    /// OAuth2 client identifier for the Entra ID application.
    pub client_id: String,
    /// OAuth2 client secret for the Entra ID application.
    pub client_secret: String,
    /// Tenant identifier (defaults to `common` when omitted).
    pub tenant_id: Option<String>,
    /// Additional scopes requested during authorization.
    pub scopes: Vec<String>,
    /// Optional explicit redirect path override.
    pub redirect_path: Option<String>,
}

impl Default for OAuth2MicrosoftConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            tenant_id: None,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            redirect_path: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2CasbinConfig {
    /// Casbin model configuration (INI format).
    pub model: String,
    /// Casbin policy rules (CSV format).
    pub policy: String,
}

impl Default for OAuth2CasbinConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_CASBIN_MODEL.trim().to_string(),
            policy: DEFAULT_CASBIN_POLICY.trim().to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct OAuth2GroupRoleMapping {
    /// Identity provider that emits the group/role claim.
    pub provider: OAuth2ProviderKind,
    /// Group or role identifier received from the provider.
    pub group: String,
    /// Pkgly Casbin roles applied when the group is present.
    pub roles: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct PasswordRules {
    pub min_length: usize,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_number: bool,
    pub require_symbol: bool,
}
impl PasswordRules {
    pub fn validate(&self, password: &str) -> bool {
        if password.len() < self.min_length {
            return false;
        }
        if self.require_uppercase && !password.chars().any(|c| c.is_uppercase()) {
            return false;
        }
        if self.require_lowercase && !password.chars().any(|c| c.is_lowercase()) {
            return false;
        }
        if self.require_number && !password.chars().any(|c| c.is_numeric()) {
            return false;
        }
        if self.require_symbol && !password.chars().any(|c| c.is_ascii_punctuation()) {
            return false;
        }
        true
    }
}
impl Default for PasswordRules {
    fn default() -> Self {
        Self {
            min_length: 8,
            require_uppercase: true,
            require_lowercase: true,
            require_number: true,
            require_symbol: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TlsConfig {
    pub private_key: PathBuf,
    pub certificate_chain: PathBuf,
}
