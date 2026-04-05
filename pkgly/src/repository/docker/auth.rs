use ahash::{HashMap, HashMapExt};
use std::fmt;

use axum::{
    Json,
    extract::{Extension, Query, State},
    response::{IntoResponse, Response},
};
use chrono::{Duration, FixedOffset, Utc};
use http::{HeaderMap, StatusCode};
use nr_core::{
    database::{DateTime, entities::user::auth_token::NewRepositoryToken},
    user::permissions::RepositoryActions,
};
use serde::{
    Serialize,
    de::{Deserializer, IgnoredAny, MapAccess, Visitor},
};
use tracing::{debug, error, instrument, warn};
use uuid::Uuid;

use crate::repository::repo_http::RepositoryAuthentication;
use crate::{
    app::{Pkgly, RepositoryStorageName, authentication::AuthenticationRaw},
    repository::{DynRepository, Repository},
    utils::ResponseBuilder,
};

const DEFAULT_TOKEN_LIFETIME: i64 = 15 * 60;

#[derive(Debug, Default)]
pub struct DockerTokenQuery {
    pub service: Option<String>,
    pub scope: Vec<String>,
    pub account: Option<String>,
    pub client_id: Option<String>,
}

impl<'de> serde::Deserialize<'de> for DockerTokenQuery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct QueryVisitor;

        impl<'de> Visitor<'de> for QueryVisitor {
            type Value = DockerTokenQuery;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("Docker token query parameters")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut service: Option<String> = None;
                let mut scope: Vec<String> = Vec::new();
                let mut account: Option<String> = None;
                let mut client_id: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "service" => {
                            service = Some(map.next_value()?);
                        }
                        "scope" => {
                            let value: String = map.next_value()?;
                            scope.extend(split_scope_values(&value));
                        }
                        "account" => {
                            account = Some(map.next_value()?);
                        }
                        "client_id" => {
                            client_id = Some(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<IgnoredAny>()?;
                        }
                    }
                }

                Ok(DockerTokenQuery {
                    service,
                    scope,
                    account,
                    client_id,
                })
            }
        }

        deserializer.deserialize_map(QueryVisitor)
    }
}

#[derive(Debug, Serialize)]
pub struct DockerTokenResponse {
    pub token: String,
    #[serde(rename = "access_token")]
    pub access_token: String,
    pub expires_in: i64,
    pub issued_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

fn split_scope_values(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
        .collect()
}

pub fn docker_repository_scope(storage: &str, repository: &str, rest: &str) -> String {
    let trimmed = rest.trim_matches('/');
    if trimmed.is_empty() {
        return format!("{storage}/{repository}");
    }

    // Identify where the repository-specific path ends and Docker reserved
    // segments (blobs, manifests, tags) begin.
    let mut end = trimmed.len();
    for marker in ["/blobs/", "/manifests/", "/tags/", "/_catalog"] {
        if let Some(idx) = trimmed.find(marker) {
            if idx < end {
                end = idx;
            }
        }
    }

    let repo_suffix = trimmed[..end].trim_end_matches('/');
    if repo_suffix.is_empty() {
        format!("{storage}/{repository}")
    } else {
        format!("{storage}/{repository}/{}", repo_suffix)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DockerTokenError {
    #[error("authentication required")]
    Authentication,
    #[error("forbidden")]
    Forbidden,
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    #[error("repository not found: {0}")]
    RepositoryNotFound(String),
    #[error("internal error")]
    Internal,
}

impl IntoResponse for DockerTokenError {
    fn into_response(self) -> Response {
        let status = match self {
            DockerTokenError::Authentication => StatusCode::UNAUTHORIZED,
            DockerTokenError::Forbidden => StatusCode::FORBIDDEN,
            DockerTokenError::InvalidScope(_) | DockerTokenError::RepositoryNotFound(_) => {
                StatusCode::BAD_REQUEST
            }
            DockerTokenError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = match &self {
            DockerTokenError::Authentication => {
                serde_json::json!({"errors":[{"code":"UNAUTHORIZED","message":"authentication required"}]})
            }
            DockerTokenError::Forbidden => {
                serde_json::json!({"errors":[{"code":"DENIED","message":"requested access to the resource is denied"}]})
            }
            DockerTokenError::InvalidScope(scope) => serde_json::json!({
                "errors":[{"code":"UNSUPPORTED","message":format!("invalid scope: {scope}")}]
            }),
            DockerTokenError::RepositoryNotFound(repo) => serde_json::json!({
                "errors":[{"code":"NAME_UNKNOWN","message":format!("repository not found: {repo}")}]
            }),
            DockerTokenError::Internal => serde_json::json!({
                "errors":[{"code":"INTERNAL","message":"internal server error"}]
            }),
        };
        let mut builder = ResponseBuilder::default()
            .status(status)
            .header(http::header::CONTENT_TYPE, "application/json");

        if matches!(self, DockerTokenError::Authentication) {
            builder = builder.header(
                http::header::WWW_AUTHENTICATE,
                "Basic realm=\"Pkgly Docker Token\"",
            );
        }

        builder.json(&message)
    }
}

#[derive(Debug, Clone)]
struct ParsedScope {
    raw: String,
    storage: String,
    repository: String,
    actions: Vec<RepositoryActions>,
}

#[instrument(skip(site, query, auth_raw))]
pub async fn handle_docker_token(
    State(site): State<Pkgly>,
    Query(query): Query<DockerTokenQuery>,
    auth_raw: Option<Extension<AuthenticationRaw>>,
) -> Result<Response, DockerTokenError> {
    let raw = auth_raw
        .map(|Extension(inner)| inner)
        .unwrap_or(AuthenticationRaw::NoIdentification);

    let authentication = RepositoryAuthentication::from_raw(raw.clone(), &site)
        .await
        .map_err(|err| {
            debug!("Failed to authenticate docker token request: {}", err);
            DockerTokenError::Authentication
        })?;

    let Some(user) = authentication.get_user().cloned() else {
        return Err(DockerTokenError::Authentication);
    };

    if matches!(
        authentication,
        RepositoryAuthentication::NoIdentification | RepositoryAuthentication::Other(_, _)
    ) {
        return Err(DockerTokenError::Authentication);
    }

    let scopes = parse_scopes(&query.scope)?;

    // Validate requested scopes
    let mut repository_requests: HashMap<Uuid, Vec<RepositoryActions>> = HashMap::new();

    for scope in scopes.iter() {
        let repo_name =
            RepositoryStorageName::from((scope.storage.clone(), scope.repository.clone()));

        let Some(repository) = site
            .get_repository_from_names(&repo_name)
            .await
            .map_err(|err| {
                error!("Failed to load repository for scope {:?}: {}", scope, err);
                DockerTokenError::Internal
            })?
        else {
            return Err(DockerTokenError::RepositoryNotFound(scope.raw.clone()));
        };

        let repository_id = match repository {
            DynRepository::Docker(ref repo) => repo.id(),
            DynRepository::Helm(ref repo) => repo.id(),
            other => {
                warn!(
                    repository_type = other.get_type(),
                    storage = scope.storage,
                    repository = scope.repository,
                    "Scope requested repository that does not support OCI flows"
                );
                return Err(DockerTokenError::InvalidScope(scope.raw.clone()));
            }
        };

        for action in scope.actions.iter() {
            let allowed = authentication
                .can_access_repository(*action, repository_id, &site.database)
                .await
                .map_err(|err| {
                    error!(
                        ?err,
                        repository_id = %repository_id,
                        "Failed to verify repository access for docker token"
                    );
                    DockerTokenError::Internal
                })?;
            if !allowed {
                debug!(
                    ?scope,
                    repository_id = %repository_id,
                    "Repository action not permitted for docker token"
                );
                return Err(DockerTokenError::Forbidden);
            }
            let entry = repository_requests.entry(repository_id).or_default();
            if !entry.contains(action) {
                entry.push(*action);
            }
        }
    }

    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(DEFAULT_TOKEN_LIFETIME);
    let Some(offset) = FixedOffset::east_opt(0) else {
        error!("failed to construct UTC offset for docker token expiry");
        return Err(DockerTokenError::Internal);
    };
    let expires_at_fixed: DateTime = expires_at.with_timezone(&offset);

    let repositories = repository_requests.into_iter().collect::<Vec<_>>();

    let source = query
        .client_id
        .clone()
        .unwrap_or_else(|| "docker_bearer".to_string());

    let repo_token = NewRepositoryToken {
        user_id: user.id,
        source,
        repositories,
        expires_at: Some(expires_at_fixed),
    };

    let (token_id, token) = repo_token.insert(&site.database).await.map_err(|err| {
        error!("Failed to create docker bearer token: {}", err);
        DockerTokenError::Internal
    })?;

    debug!(
        token_id,
        user_id = user.id,
        scopes = ?scopes,
        "Issued docker bearer token"
    );

    let response = DockerTokenResponse {
        token: token.clone(),
        access_token: token,
        expires_in: DEFAULT_TOKEN_LIFETIME,
        issued_at: issued_at.to_rfc3339(),
        scope: (!query.scope.is_empty()).then(|| query.scope.join(" ")),
    };

    Ok(Json(response).into_response())
}

fn parse_scopes(scopes: &[String]) -> Result<Vec<ParsedScope>, DockerTokenError> {
    let mut parsed = Vec::new();

    for scope in scopes
        .iter()
        .flat_map(|value| split_scope_values(value).into_iter())
    {
        if scope.is_empty() {
            continue;
        }
        let parts: Vec<&str> = scope.split(':').collect();
        if parts.len() != 3 {
            return Err(DockerTokenError::InvalidScope(scope.clone()));
        }
        if parts[0] != "repository" {
            warn!(?scope, "Unsupported scope type requested");
            continue;
        }
        let name = parts[1];
        let actions_part = parts[2];

        let mut segments: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
        if segments.first().copied() == Some("repositories") {
            segments.remove(0);
        }
        if segments.len() < 2 {
            return Err(DockerTokenError::InvalidScope(scope.clone()));
        }
        let storage = segments[0];
        let repository = segments[1];

        let mut actions: Vec<RepositoryActions> = Vec::new();
        for action in actions_part.split(',') {
            match action {
                "pull" => {
                    if !actions.contains(&RepositoryActions::Read) {
                        actions.push(RepositoryActions::Read);
                    }
                }
                "push" => {
                    if !actions.contains(&RepositoryActions::Write) {
                        actions.push(RepositoryActions::Write);
                    }
                }
                "*" => {
                    if !actions.contains(&RepositoryActions::Read) {
                        actions.push(RepositoryActions::Read);
                    }
                    if !actions.contains(&RepositoryActions::Write) {
                        actions.push(RepositoryActions::Write);
                    }
                }
                "" => {}
                other => {
                    warn!(action = other, "Unsupported docker scope action requested");
                }
            }
        }

        parsed.push(ParsedScope {
            raw: scope.clone(),
            storage: storage.to_string(),
            repository: repository.to_string(),
            actions,
        });
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests;

pub fn build_docker_bearer_challenge(
    site: &Pkgly,
    headers: Option<&HeaderMap>,
    repository_scope: &str,
    actions: &[&str],
) -> String {
    let (base_url, service) = resolve_registry_location(site, headers);
    let scope_actions = actions.join(",");
    let scope = format!("repository:{repository_scope}:{scope_actions}");
    format!("Bearer realm=\"{base_url}/v2/token\",service=\"{service}\",scope=\"{scope}\"")
}

pub fn build_registry_bearer_challenge(site: &Pkgly, headers: Option<&HeaderMap>) -> String {
    let (base_url, service) = resolve_registry_location(site, headers);
    format!("Bearer realm=\"{base_url}/v2/token\",service=\"{service}\"")
}

pub fn docker_unauthorized_body(repository_scope: &str, actions: &[&str]) -> String {
    let details: Vec<_> = actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "Type": "repository",
                "Class": "",
                "Name": repository_scope,
                "Action": action,
            })
        })
        .collect();

    serde_json::json!({
        "errors": [{
            "code": "UNAUTHORIZED",
            "message": "authentication required",
            "detail": details,
        }]
    })
    .to_string()
}

pub fn resolve_registry_location(
    site: &Pkgly,
    headers: Option<&HeaderMap>,
) -> (String, String) {
    let forwarded_proto = headers
        .and_then(|h| h.get("x-forwarded-proto"))
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string());
    let forwarded_host = headers
        .and_then(|h| h.get("x-forwarded-host"))
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string());
    let host_header = headers
        .and_then(|h| h.get(http::header::HOST))
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string());

    let instance = site.inner.instance.lock();
    let configured_url = instance.app_url.clone();
    let is_https = instance.is_https;
    drop(instance);

    let configured_scheme = (!configured_url.is_empty())
        .then(|| extract_scheme(&configured_url))
        .flatten();
    let configured_authority = (!configured_url.is_empty())
        .then(|| extract_authority(&configured_url))
        .flatten();

    let service = forwarded_host
        .clone()
        .or_else(|| host_header.clone())
        .or_else(|| configured_authority.clone())
        .unwrap_or_else(|| "localhost".to_string());

    let authority_for_base = forwarded_host
        .clone()
        .or_else(|| configured_authority.clone())
        .or_else(|| host_header.clone())
        .unwrap_or_else(|| service.clone());

    let scheme = forwarded_proto
        .or_else(|| configured_scheme.clone())
        .unwrap_or_else(|| {
            if is_https {
                "https".to_string()
            } else {
                "http".to_string()
            }
        });

    let base = format!("{scheme}://{authority_for_base}");
    (base, service)
}

fn extract_authority(url: &str) -> Option<String> {
    url.parse::<http::Uri>()
        .ok()
        .and_then(|uri| uri.authority().map(|auth| auth.as_str().to_string()))
}

fn extract_scheme(url: &str) -> Option<String> {
    url.parse::<http::Uri>()
        .ok()
        .and_then(|uri| uri.scheme_str().map(|s| s.to_string()))
}
