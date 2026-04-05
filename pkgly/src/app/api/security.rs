use axum::{Json, extract::State, response::Response, routing::get};
use nr_core::user::permissions::HasPermissions;
use tracing::{error, instrument};
use utoipa::{OpenApi, ToSchema};

use crate::{
    app::{
        Pkgly,
        authentication::Authentication,
        authentication::oauth::normalize_scopes,
        config::{
            OAuth2CasbinConfig, OAuth2GoogleConfig, OAuth2GroupRoleMapping, OAuth2MicrosoftConfig,
            OAuth2Settings, OidcProviderConfig, SsoSettings, TokenSource,
        },
    },
    error::InternalError,
    utils::ResponseBuilder,
};
use http::HeaderName;

use serde::{Deserialize, Serialize};

#[derive(OpenApi)]
#[openapi(
    paths(
        get_sso_settings,
        update_sso_settings,
        get_oauth2_settings,
        update_oauth2_settings
    ),
    components(schemas(SsoSettings, OAuth2Settings))
)]
pub struct SecurityAPI;

pub fn security_routes() -> axum::Router<Pkgly> {
    axum::Router::new()
        .route("/sso", get(get_sso_settings).put(update_sso_settings))
        .route(
            "/oauth2",
            get(get_oauth2_settings).put(update_oauth2_settings),
        )
}

#[derive(Debug, Serialize, ToSchema)]
struct OAuth2ProviderSettingsResponse {
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<String>,
    scopes: Vec<String>,
    client_secret_configured: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct OAuth2SettingsResponse {
    enabled: bool,
    login_path: String,
    callback_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_base_url: Option<String>,
    auto_create_users: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    google: Option<OAuth2ProviderSettingsResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    microsoft: Option<OAuth2ProviderSettingsResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    casbin: Option<OAuth2CasbinConfig>,
    #[serde(default)]
    group_role_mappings: Vec<OAuth2GroupRoleMapping>,
    #[serde(default)]
    available_roles: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct OAuth2ProviderSettingsRequest {
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    redirect_path: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct OAuth2MicrosoftSettingsRequest {
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    redirect_path: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct OAuth2SettingsRequest {
    enabled: bool,
    login_path: String,
    callback_path: String,
    #[serde(default)]
    redirect_base_url: Option<String>,
    auto_create_users: bool,
    #[serde(default)]
    google: Option<OAuth2ProviderSettingsRequest>,
    #[serde(default)]
    microsoft: Option<OAuth2MicrosoftSettingsRequest>,
    #[serde(default)]
    casbin: Option<OAuth2CasbinConfig>,
    #[serde(default)]
    group_role_mappings: Vec<OAuth2GroupRoleMapping>,
}

fn sanitize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|val| {
        let trimmed = val.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn sanitize_relative_path(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn sanitize_scopes(scopes: Vec<String>) -> Vec<String> {
    let cleaned: Vec<String> = scopes
        .into_iter()
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect();
    normalize_scopes(&cleaned)
}

fn collect_available_roles(policy: &str) -> Vec<String> {
    let mut roles = Vec::new();
    for line in policy.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut segments = trimmed.split(',').map(|segment| segment.trim());
        let Some(rule_type) = segments.next() else {
            continue;
        };
        if rule_type != "p" {
            continue;
        }
        if let Some(role) = segments.next() {
            if !role.is_empty() {
                roles.push(role.to_string());
            }
        }
    }
    roles.sort();
    roles.dedup();
    roles
}

fn sanitize_casbin_config(mut cfg: OAuth2CasbinConfig) -> OAuth2CasbinConfig {
    let defaults = OAuth2CasbinConfig::default();
    let model = {
        let trimmed = cfg.model.trim();
        if trimmed.is_empty() {
            defaults.model.clone()
        } else {
            trimmed.to_string()
        }
    };
    let policy = {
        let trimmed = cfg.policy.trim();
        if trimmed.is_empty() {
            defaults.policy.clone()
        } else {
            trimmed.to_string()
        }
    };
    cfg.model = model;
    cfg.policy = policy;
    cfg
}

fn build_oauth2_response(settings: &OAuth2Settings) -> OAuth2SettingsResponse {
    let google = settings
        .google
        .as_ref()
        .map(|cfg| OAuth2ProviderSettingsResponse {
            client_id: cfg.client_id.clone(),
            redirect_path: cfg.redirect_path.clone(),
            tenant_id: None,
            scopes: cfg.scopes.clone(),
            client_secret_configured: !cfg.client_secret.is_empty(),
        });
    let microsoft = settings
        .microsoft
        .as_ref()
        .map(|cfg| OAuth2ProviderSettingsResponse {
            client_id: cfg.client_id.clone(),
            redirect_path: cfg.redirect_path.clone(),
            tenant_id: cfg.tenant_id.clone(),
            scopes: cfg.scopes.clone(),
            client_secret_configured: !cfg.client_secret.is_empty(),
        });

    OAuth2SettingsResponse {
        enabled: settings.enabled,
        login_path: settings.login_path.clone(),
        callback_path: settings.callback_path.clone(),
        redirect_base_url: settings.redirect_base_url.clone(),
        auto_create_users: settings.auto_create_users,
        google,
        microsoft,
        casbin: settings.casbin.clone(),
        group_role_mappings: settings.group_role_mappings.clone(),
        available_roles: settings
            .casbin
            .as_ref()
            .map(|cfg| collect_available_roles(&cfg.policy))
            .unwrap_or_default(),
    }
}

fn merge_oauth2_settings(
    current: Option<&OAuth2Settings>,
    request: OAuth2SettingsRequest,
) -> Result<OAuth2Settings, String> {
    let login_path = sanitize_relative_path(&request.login_path, "/api/user/oauth2/login");
    let callback_path = sanitize_relative_path(&request.callback_path, "/api/user/oauth2/callback");

    let google =
        merge_google_settings(current.and_then(|cfg| cfg.google.as_ref()), request.google)?;
    let microsoft = merge_microsoft_settings(
        current.and_then(|cfg| cfg.microsoft.as_ref()),
        request.microsoft,
    )?;

    let mut casbin = match request.casbin {
        Some(cfg) => Some(sanitize_casbin_config(cfg)),
        None => current.and_then(|cfg| cfg.casbin.clone()),
    };
    if request.enabled && casbin.is_none() {
        casbin = Some(OAuth2CasbinConfig::default());
    }

    let group_role_mappings = request
        .group_role_mappings
        .into_iter()
        .filter_map(|mapping| {
            let OAuth2GroupRoleMapping {
                provider,
                group,
                mut roles,
            } = mapping;
            let group_trimmed = group.trim();
            if group_trimmed.is_empty() {
                return None;
            }
            roles = roles
                .into_iter()
                .map(|role| role.trim().to_string())
                .filter(|role| !role.is_empty())
                .collect();
            if roles.is_empty() {
                return None;
            }
            roles.sort();
            roles.dedup();
            Some(OAuth2GroupRoleMapping {
                provider,
                group: group_trimmed.to_string(),
                roles,
            })
        })
        .collect();

    Ok(OAuth2Settings {
        enabled: request.enabled,
        login_path,
        callback_path,
        redirect_base_url: sanitize_optional(request.redirect_base_url),
        auto_create_users: request.auto_create_users,
        google,
        microsoft,
        casbin,
        group_role_mappings,
    })
}

fn sanitize_header_name(header: &str) -> Result<String, String> {
    let trimmed = header.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    HeaderName::from_bytes(trimmed.as_bytes())
        .map(|_| trimmed.to_string())
        .map_err(|_| format!("Invalid header name: {trimmed}"))
}

fn sanitize_provider(mut provider: OidcProviderConfig) -> Result<OidcProviderConfig, String> {
    provider.name = provider.name.trim().to_string();
    provider.issuer = provider.issuer.trim().trim_end_matches('/').to_string();
    provider.audience = provider.audience.trim().to_string();
    provider.jwks_url = sanitize_optional(provider.jwks_url.take());
    provider.subject_claim = sanitize_optional(provider.subject_claim.take());
    provider.email_claim = sanitize_optional(provider.email_claim.take());
    provider.display_name_claim = sanitize_optional(provider.display_name_claim.take());
    provider.role_claims = provider
        .role_claims
        .into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();

    provider.token_source = match provider.token_source {
        TokenSource::Header { name, prefix } => TokenSource::Header {
            name: sanitize_header_name(&name)?,
            prefix: prefix.and_then(|value| sanitize_optional(Some(value))),
        },
        TokenSource::Cookie { name } => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err("Cookie name cannot be empty".into());
            }
            TokenSource::Cookie {
                name: trimmed.to_string(),
            }
        }
    };

    if provider.issuer.is_empty() {
        return Err("Provider issuer is required".into());
    }
    if provider.audience.is_empty() {
        return Err("Provider audience is required".into());
    }
    if provider.name.is_empty() {
        return Err("Provider name is required".into());
    }

    Ok(provider)
}

fn sanitize_sso_settings(mut settings: SsoSettings) -> Result<SsoSettings, String> {
    settings.login_path = sanitize_relative_path(&settings.login_path, "/api/user/sso/login");
    settings.login_button_text = {
        let trimmed = settings.login_button_text.trim();
        if trimmed.is_empty() {
            "Sign in with SSO".to_string()
        } else {
            trimmed.to_string()
        }
    };

    settings.provider_login_url = sanitize_optional(settings.provider_login_url.take());
    settings.provider_redirect_param = sanitize_optional(settings.provider_redirect_param.take());

    let mut providers = Vec::new();
    for provider in settings.providers.into_iter() {
        let sanitized = sanitize_provider(provider)?;
        providers.push(sanitized);
    }
    settings.providers = providers;
    settings.role_claims = settings
        .role_claims
        .into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    Ok(settings)
}

fn merge_google_settings(
    current: Option<&OAuth2GoogleConfig>,
    request: Option<OAuth2ProviderSettingsRequest>,
) -> Result<Option<OAuth2GoogleConfig>, String> {
    let Some(req) = request else {
        return Ok(None);
    };

    let client_id = req.client_id.trim();
    if client_id.is_empty() {
        return Err("Google client ID is required".into());
    }

    let secret = match req.client_secret {
        Some(secret) => {
            let trimmed = secret.trim();
            if trimmed.is_empty() {
                return Err("Google client secret cannot be empty".into());
            }
            trimmed.to_string()
        }
        None => current
            .map(|cfg| cfg.client_secret.clone())
            .ok_or_else(|| "Provide a Google client secret".to_string())?,
    };

    Ok(Some(OAuth2GoogleConfig {
        client_id: client_id.to_string(),
        client_secret: secret,
        scopes: sanitize_scopes(req.scopes),
        redirect_path: sanitize_optional(req.redirect_path),
    }))
}

fn merge_microsoft_settings(
    current: Option<&OAuth2MicrosoftConfig>,
    request: Option<OAuth2MicrosoftSettingsRequest>,
) -> Result<Option<OAuth2MicrosoftConfig>, String> {
    let Some(req) = request else {
        return Ok(None);
    };

    let client_id = req.client_id.trim();
    if client_id.is_empty() {
        return Err("Microsoft client ID is required".into());
    }

    let secret = match req.client_secret {
        Some(secret) => {
            let trimmed = secret.trim();
            if trimmed.is_empty() {
                return Err("Microsoft client secret cannot be empty".into());
            }
            trimmed.to_string()
        }
        None => current
            .map(|cfg| cfg.client_secret.clone())
            .ok_or_else(|| "Provide a Microsoft client secret".to_string())?,
    };

    Ok(Some(OAuth2MicrosoftConfig {
        client_id: client_id.to_string(),
        client_secret: secret,
        tenant_id: sanitize_optional(req.tenant_id),
        scopes: sanitize_scopes(req.scopes),
        redirect_path: sanitize_optional(req.redirect_path),
    }))
}

#[utoipa::path(
    get,
    path = "/sso",
    tag = "security",
    responses((status = 200, description = "Current SSO configuration", body = SsoSettings)),
    security(("session" = []))
)]
#[instrument(skip(auth, site), fields(project_module = "Security"))]
pub async fn get_sso_settings(
    auth: Authentication,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    let security = site.security_settings();
    let settings = security.sso.unwrap_or_else(SsoSettings::default);
    Ok(ResponseBuilder::ok().json(&settings))
}

#[utoipa::path(
    put,
    path = "/sso",
    tag = "security",
    request_body = SsoSettings,
    responses((status = 204, description = "SSO configuration updated")),
    security(("session" = []))
)]
#[instrument(skip(auth, site, settings), fields(project_module = "Security"))]
pub async fn update_sso_settings(
    auth: Authentication,
    State(site): State<Pkgly>,
    Json(settings): Json<SsoSettings>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    let sanitized = match sanitize_sso_settings(settings) {
        Ok(settings) => settings,
        Err(message) => return Ok(ResponseBuilder::bad_request().body(message)),
    };

    if let Err(err) = site.update_sso_settings(Some(sanitized)).await {
        error!(%err, "Failed to update SSO configuration");
        return Ok(
            ResponseBuilder::internal_server_error().body("Failed to update SSO configuration")
        );
    }

    Ok(ResponseBuilder::no_content().empty())
}

#[cfg(test)]
mod tests;

#[utoipa::path(
    get,
    path = "/oauth2",
    tag = "security",
    responses((status = 200, description = "Current OAuth2 configuration", body = OAuth2Settings)),
    security(("session" = []))
)]
#[instrument(skip(auth, site), fields(project_module = "Security"))]
pub async fn get_oauth2_settings(
    auth: Authentication,
    State(site): State<Pkgly>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    let security = site.security_settings();
    let settings = security.oauth2.unwrap_or_else(OAuth2Settings::default);
    let response = build_oauth2_response(&settings);
    Ok(ResponseBuilder::ok().json(&response))
}

#[utoipa::path(
    put,
    path = "/oauth2",
    tag = "security",
    request_body = OAuth2Settings,
    responses((status = 204, description = "OAuth2 configuration updated")),
    security(("session" = []))
)]
#[instrument(skip(auth, site, settings), fields(project_module = "Security"))]
pub async fn update_oauth2_settings(
    auth: Authentication,
    State(site): State<Pkgly>,
    Json(settings): Json<OAuth2SettingsRequest>,
) -> Result<Response, InternalError> {
    if !auth.is_admin_or_system_manager() {
        return Ok(ResponseBuilder::forbidden().body("Administrator permissions required"));
    }

    let current = site.oauth2_settings_raw();
    let merged = match merge_oauth2_settings(current.as_ref(), settings) {
        Ok(settings) => settings,
        Err(err) => {
            error!(%err, "Invalid OAuth2 configuration submitted");
            return Ok(ResponseBuilder::bad_request().body(err));
        }
    };

    if let Err(err) = site.update_oauth2_settings(Some(merged)).await {
        error!(%err, "Failed to update OAuth2 configuration");
        return Ok(
            ResponseBuilder::internal_server_error().body("Failed to update OAuth2 configuration")
        );
    }

    Ok(ResponseBuilder::no_content().empty())
}
