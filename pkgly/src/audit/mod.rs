use opentelemetry::trace::TraceContextExt as _;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

#[derive(Debug, Clone, Default)]
pub struct AuditActor {
    pub username: String,
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct AuditMetadata {
    pub actor: AuditActor,
    pub action: Option<String>,
    pub resource_kind: Option<String>,
    pub resource_id: Option<String>,
    pub resource_name: Option<String>,
    pub repository_id: Option<String>,
    pub storage_id: Option<String>,
    pub target_user_id: Option<i32>,
    pub token_id: Option<i32>,
    pub path: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    Success,
    Denied,
}

impl AuditOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Denied => "denied",
        }
    }
}

pub fn should_emit_audit(status_code: i64) -> Option<AuditOutcome> {
    match status_code {
        200..=299 => Some(AuditOutcome::Success),
        401 | 403 => Some(AuditOutcome::Denied),
        _ => None,
    }
}

pub fn classify_api_action(http_method: &str, http_route: &str) -> Option<&'static str> {
    match (http_method, http_route) {
        ("POST", "/api/install") => Some("system.install"),
        ("GET", "/api/user/me") => Some("auth.me"),
        ("GET", "/api/user/me/permissions") => Some("auth.permissions.get"),
        ("GET", "/api/user/whoami") => Some("auth.whoami"),
        ("POST", "/api/user/login") => Some("auth.login"),
        ("GET", "/api/user/sso/login") => Some("auth.sso.login"),
        ("GET", "/api/user/oauth2/providers") => Some("auth.oauth.provider.list"),
        ("GET", "/api/user/oauth2/login/{provider}") => Some("auth.oauth.login"),
        ("GET", "/api/user/oauth2/callback") => Some("auth.oauth.callback"),
        ("GET", "/api/user/sessions") => Some("auth.session.list"),
        ("POST", "/api/user/logout") => Some("auth.logout"),
        ("POST", "/api/user/change-password") => Some("auth.password_change"),
        ("POST", "/api/user/password-reset/request") => Some("auth.password_reset.request"),
        ("GET", "/api/user/password-reset/check/{token}") => Some("auth.password_reset.check"),
        ("POST", "/api/user/password-reset/{token}") => Some("auth.password_reset.perform"),
        ("POST", "/api/user/token/create") => Some("auth.token.create"),
        ("GET", "/api/user/token/list") => Some("auth.token.list"),
        ("GET", "/api/user/token/get/{id}") => Some("auth.token.get"),
        ("DELETE", "/api/user/token/delete/{id}") => Some("auth.token.delete"),
        ("GET", "/api/user-management/list") => Some("user.list"),
        ("GET", "/api/user-management/get/{user_id}") => Some("user.get"),
        ("GET", "/api/user-management/get/{user_id}/permissions") => Some("user.permissions.get"),
        ("POST", "/api/user-management/create") => Some("user.create"),
        ("POST", "/api/user-management/is-taken") => Some("user.is_taken"),
        ("PUT", "/api/user-management/update/{user_id}") => Some("user.update"),
        ("PUT", "/api/user-management/update/{user_id}/permissions") => {
            Some("user.permissions.update")
        }
        ("PUT", "/api/user-management/update/{user_id}/password") => Some("user.password.update"),
        ("PUT", "/api/user-management/update/{user_id}/status") => Some("user.status.update"),
        ("DELETE", "/api/user-management/delete/{user_id}") => Some("user.delete"),
        ("GET", "/api/storage/list") => Some("storage.list"),
        ("POST", "/api/storage/new/{storage_type}") => Some("storage.create"),
        ("GET", "/api/storage/{id}") => Some("storage.get"),
        ("PUT", "/api/storage/{id}") => Some("storage.update"),
        ("POST", "/api/storage/local/path-helper") => Some("storage.path_helper"),
        ("GET", "/api/storage/s3/regions") => Some("storage.s3.region.list"),
        ("GET", "/api/repository/list") => Some("repository.list"),
        ("GET", "/api/repository/find-id/{storage_name}/{repository_name}") => {
            Some("repository.find_id")
        }
        ("GET", "/api/repository/{repository_id}") => Some("repository.get"),
        ("GET", "/api/repository/{repository_id}/names") => Some("repository.names.get"),
        ("GET", "/api/repository/types") => Some("repository.types.list"),
        ("GET", "/api/repository/config/{key}/schema") => Some("repository.config.schema"),
        ("POST", "/api/repository/config/{key}/validate") => Some("repository.config.validate"),
        ("GET", "/api/repository/config/{key}/default") => Some("repository.config.default"),
        ("GET", "/api/repository/config/{key}/description") => {
            Some("repository.config.description")
        }
        ("GET", "/api/repository/browse/{repository_id}") => Some("repository.browse"),
        ("GET", "/api/repository/browse/{repository_id}/") => Some("repository.browse"),
        ("GET", "/api/repository/browse/{repository_id}/{*path}") => Some("repository.browse"),
        ("GET", "/api/repository/browse-ws/{repository_id}") => Some("repository.browse_ws"),
        ("GET", "/api/repository/{repository_id}/packages") => Some("repository.package.list"),
        ("DELETE", "/api/repository/{repository_id}/packages") => Some("repository.package.delete"),
        ("GET", "/api/repository/{repository_id}/configs") => Some("repository.config.list"),
        ("POST", "/api/repository/new/{repository_type}") => Some("repository.create"),
        ("PUT", "/api/repository/{repository_id}/config/{key}") => Some("repository.config.update"),
        ("GET", "/api/repository/{repository_id}/config/{key}") => Some("repository.config.get"),
        ("POST", "/api/repository/{repository_id}/deb/refresh") => Some("repository.deb.refresh"),
        ("GET", "/api/repository/{repository_id}/deb/refresh/status") => {
            Some("repository.deb.refresh_status")
        }
        ("DELETE", "/api/repository/{repository_id}") => Some("repository.delete"),
        ("GET", "/api/repository/{repository_id}/virtual/members") => {
            Some("repository.virtual.members.list")
        }
        ("POST", "/api/repository/{repository_id}/virtual/members") => {
            Some("repository.virtual.members.update")
        }
        ("PUT", "/api/repository/{repository_id}/virtual/resolution-order") => {
            Some("repository.virtual.resolution_order.update")
        }
        ("GET", "/api/search/packages") => Some("package.search"),
        ("GET", "/api/security/sso") => Some("security.sso.get"),
        ("PUT", "/api/security/sso") => Some("security.sso.update"),
        ("GET", "/api/security/oauth2") => Some("security.oauth2.get"),
        ("PUT", "/api/security/oauth2") => Some("security.oauth2.update"),
        ("GET", "/api/system/webhooks") => Some("system.webhook.list"),
        ("POST", "/api/system/webhooks") => Some("system.webhook.create"),
        ("GET", "/api/system/webhooks/{id}") => Some("system.webhook.get"),
        ("PUT", "/api/system/webhooks/{id}") => Some("system.webhook.update"),
        ("DELETE", "/api/system/webhooks/{id}") => Some("system.webhook.delete"),
        ("GET", "/api/project/{project_id}") => Some("project.get"),
        ("GET", "/api/project/{project_id}/versions") => Some("project.versions.list"),
        ("GET", "/api/project/by-key/{repository_id}/{project_key}") => Some("project.get"),
        _ => None,
    }
}

fn extract_safe_query(action: &str, url_path: &str, metadata: &AuditMetadata) -> String {
    if let Some(query) = &metadata.query {
        return query.clone();
    }
    if !matches!(action, "package.search" | "repository.package.list") {
        return String::new();
    }
    let Some((_, query_string)) = url_path.split_once('?') else {
        return String::new();
    };
    for (key, value) in url::form_urlencoded::parse(query_string.as_bytes()) {
        if matches!(key.as_ref(), "q" | "query") {
            return value.into_owned();
        }
    }
    String::new()
}

fn emit_audit_event(
    span: &Span,
    action: &str,
    outcome: AuditOutcome,
    metadata: &AuditMetadata,
    http_method: &str,
    http_route: &str,
    url_path: &str,
    status_code: i64,
) {
    let actor_username = if metadata.actor.username.is_empty() {
        "anonymous"
    } else {
        metadata.actor.username.as_str()
    };
    let trace_id = span.context().span().span_context().trace_id().to_string();
    let query = extract_safe_query(action, url_path, metadata);

    tracing::info!(
        target: "pkgly::audit",
        trace_id = %trace_id,
        action = %action,
        outcome = %outcome.as_str(),
        actor_username = %actor_username,
        actor_id = metadata.actor.user_id.unwrap_or_default(),
        actor_authenticated = metadata.actor.user_id.is_some(),
        resource_kind = %metadata.resource_kind.as_deref().unwrap_or(""),
        resource_id = %metadata.resource_id.as_deref().unwrap_or(""),
        resource_name = %metadata.resource_name.as_deref().unwrap_or(""),
        repository_id = %metadata.repository_id.as_deref().unwrap_or(""),
        storage_id = %metadata.storage_id.as_deref().unwrap_or(""),
        target_user_id = metadata.target_user_id.unwrap_or_default(),
        token_id = metadata.token_id.unwrap_or_default(),
        path = %metadata.path.as_deref().unwrap_or(""),
        query = %query,
        http_request_method = %http_method,
        http_route = %http_route,
        url_path = %url_path,
        http_response_status_code = status_code,
        "Audit event"
    );
}

pub fn emit_http_audit_log(
    span: &Span,
    http_method: &str,
    http_route: &str,
    url_path: &str,
    status_code: i64,
    metadata: &AuditMetadata,
) {
    let Some(outcome) = should_emit_audit(status_code) else {
        return;
    };
    let Some(action) = metadata
        .action
        .as_deref()
        .or_else(|| classify_api_action(http_method, http_route))
    else {
        return;
    };
    emit_audit_event(
        span,
        action,
        outcome,
        metadata,
        http_method,
        http_route,
        url_path,
        status_code,
    );
}

pub fn emit_named_audit_log(
    span: &Span,
    action: &str,
    outcome: AuditOutcome,
    metadata: &AuditMetadata,
) {
    emit_audit_event(span, action, outcome, metadata, "", "", "", 0);
}

#[cfg(test)]
mod tests;
