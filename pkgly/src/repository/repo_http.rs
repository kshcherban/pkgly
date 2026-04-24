use std::error::Error;

use axum::{
    Router,
    body::Body,
    extract::{Path, Request, State},
    response::{IntoResponse, Response},
    routing::any,
};

use crate::{
    app::{
        Pkgly, RepositoryStorageName, authentication::AuthenticationError,
        responses::RepositoryNotFound,
    },
    error::IllegalStateError,
    repository::{
        Repository, RepositoryAuthConfig,
        docker::auth::{
            build_docker_bearer_challenge, build_registry_bearer_challenge,
            docker_repository_scope, docker_unauthorized_body,
        },
    },
    utils::{
        bad_request::BadRequestErrors,
        header::date_time::date_time_for_header,
        request_logging::{access_log::AccessLogContext, request_span::RequestSpan},
    },
};
pub mod repo_tracing;

use axum_extra::routing::RouterExt;
use bytes::Bytes;
use derive_more::From;
use futures::StreamExt;
use http::{
    HeaderValue, Method, StatusCode,
    header::{CONTENT_LENGTH, CONTENT_LOCATION, CONTENT_TYPE, ETAG, LAST_MODIFIED, USER_AGENT},
    request::Parts,
};
use http_body_util::BodyExt;
use nr_core::storage::{InvalidStoragePath, StoragePath};
use nr_storage::{
    FileFileType, FileType, Storage, StorageFile, StorageFileMeta, StorageFileReader,
};
use serde::Deserialize;
use tracing::{Instrument as _, Level, Span, debug, debug_span, error, event, info, instrument};
mod header;
mod repo_auth;
pub use header::*;
pub use repo_auth::*;

use super::{DynRepository, RepositoryHandlerError, repo_tracing::RepositoryRequestTracing};

use crate::utils::ResponseBuilder;

const DOCKER_API_VERSION: &str = "registry/2.0";
const DOCKER_JSON_CONTENT_TYPE: &str = "application/json";

fn docker_v2_ok_response() -> Response {
    ResponseBuilder::ok()
        .header("Docker-Distribution-API-Version", DOCKER_API_VERSION)
        .header(CONTENT_TYPE, DOCKER_JSON_CONTENT_TYPE)
        .body("{}")
}

fn docker_v2_unauthorized_response(challenge: &str, body: &str) -> Response {
    ResponseBuilder::unauthorized()
        .header("WWW-Authenticate", challenge)
        .header("Docker-Distribution-API-Version", DOCKER_API_VERSION)
        .header(CONTENT_TYPE, DOCKER_JSON_CONTENT_TYPE)
        .body(body.to_string())
}

fn classify_repo_audit_action(
    method: &Method,
    path: &StoragePath,
    response: Option<&RepoResponse>,
) -> &'static str {
    match method {
        &Method::POST | &Method::PUT | &Method::PATCH => "package.upload",
        &Method::DELETE => "package.delete",
        &Method::HEAD => "package.read_metadata",
        &Method::GET => match response {
            Some(RepoResponse::FileResponse(file)) => match file.as_ref() {
                StorageFile::Directory { .. } => "package.list",
                StorageFile::File { .. } => "package.download",
            },
            Some(RepoResponse::FileMetaResponse(meta)) => match meta.as_ref().file_type() {
                FileType::Directory { .. } => "package.list",
                FileType::File(_) => "package.read_metadata",
            },
            Some(RepoResponse::Other(_)) | None => {
                let raw_path = path.to_string();
                if raw_path.is_empty() || raw_path.ends_with('/') {
                    "package.list"
                } else {
                    "package.download"
                }
            }
        },
        _ => "package.read",
    }
}

#[cfg(test)]
mod tests;

pub fn repository_router() -> axum::Router<Pkgly> {
    Router::new()
        .route("/{storage}/{repository}/{*path}", any(handle_repo_request))
        .route_with_tsr("/{storage}/{repository}", any(handle_repo_request))
}

/// Handle Docker V2 base endpoint: /v2/ or /v2
/// This endpoint is used by Docker clients to check if the registry is available
/// and supports the V2 API. It should return 200 OK without requiring authentication.
pub async fn handle_docker_v2_base_public(
    State(site): State<Pkgly>,
    authentication: RepositoryAuthentication,
    request: Request,
) -> Response {
    info!("Docker V2 base endpoint handler invoked!");
    let headers = request.headers().clone();
    let is_authenticated = matches!(
        authentication,
        RepositoryAuthentication::AuthToken(..)
            | RepositoryAuthentication::Session(..)
            | RepositoryAuthentication::Basic(..)
    );

    if is_authenticated {
        return docker_v2_ok_response();
    }

    let challenge = build_registry_bearer_challenge(&site, Some(&headers));
    docker_v2_unauthorized_response(
        &challenge,
        r#"{"errors":[{"code":"UNAUTHORIZED","message":"authentication required"}]}"#,
    )
}

/// Handle Docker V2 catchall - routes to either base endpoint or path rewrite
/// Handles /v2/ (with trailing slash) and /v2/{storage}/{repository}/{*path}
#[instrument(skip(site, request))]
async fn handle_docker_v2_catchall(
    Path(path): Path<String>,
    State(site): State<Pkgly>,
    parent_span: Option<RequestSpan>,
    authentication: RepositoryAuthentication,
    request: Request,
) -> Result<Response, RepositoryHandlerError> {
    info!("Docker V2 catchall handler - path: {}", path);

    // If path is just "/" or empty, this is the base V2 endpoint
    if path.is_empty() || path == "/" {
        return Ok(docker_v2_ok_response());
    }

    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segments.is_empty() {
        return Ok(ResponseBuilder::not_found().body("Not Found"));
    }
    let mut index = 0usize;
    if segments.get(0) == Some(&"repositories") {
        index += 1;
    }
    if segments.len() <= index + 1 {
        return Ok(ResponseBuilder::not_found().body("Not Found"));
    }

    let storage = segments[index].to_string();
    let repository = segments[index + 1].to_string();
    let rest = if segments.len() > index + 2 {
        segments[index + 2..].join("/")
    } else {
        String::new()
    };

    debug!(
        storage = %storage,
        repository = %repository,
        rest = %rest,
        "Docker V2 catchall - parsed path"
    );

    let docker_scope = docker_repository_scope(&storage, &repository, &rest);

    // Check authentication for write operations BEFORE processing request
    // This ensures Docker gets a proper 401 challenge on first write attempt
    let method = request.method().clone();
    let is_write_operation = matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let is_blob_probe = method == Method::HEAD && rest.contains("/blobs/");
    let should_challenge_early = is_write_operation || is_blob_probe;

    if should_challenge_early
        && matches!(
            authentication,
            RepositoryAuthentication::NoIdentification | RepositoryAuthentication::Other(_, _)
        )
    {
        info!(
            method = %method,
            storage = %storage,
            repository = %repository,
            "Docker V2 write operation without authentication - returning 401 challenge"
        );
        let challenge = build_docker_bearer_challenge(
            &site,
            Some(request.headers()),
            &docker_scope,
            &["pull", "push"],
        );
        let body = docker_unauthorized_body(&docker_scope, &["pull", "push"]);
        return Ok(docker_v2_unauthorized_response(&challenge, &body));
    }

    // Build the repo request path with v2/ prefix
    let trimmed_rest = rest.trim_start_matches('/');
    let repo_path = if trimmed_rest.is_empty() {
        format!("v2/{}/{}", storage, repository)
    } else {
        format!("v2/{}/{}/{}", storage, repository, trimmed_rest)
    };

    let request_path = RepoRequestPath {
        storage: storage.clone(),
        repository: repository.clone(),
        path: Some(StoragePath::from(repo_path)),
        docker_scope: Some(docker_scope.clone()),
    };

    // Forward to the core handler logic
    let site_for_challenge = site.clone();
    let response =
        handle_repo_request_core(site, request_path, parent_span, authentication, request).await?;

    // Check if this is a 401 Unauthorized response
    // If so, ensure it has Docker-specific headers
    if response.status() == StatusCode::UNAUTHORIZED {
        let actions: &[&str] = if matches!(method, Method::GET | Method::HEAD) {
            &["pull"][..]
        } else {
            &["pull", "push"][..]
        };
        let challenge =
            build_docker_bearer_challenge(&site_for_challenge, None, &docker_scope, actions);
        let body = docker_unauthorized_body(&docker_scope, actions);
        let (parts, _) = response.into_parts();
        let mut builder = ResponseBuilder::unauthorized()
            .header("WWW-Authenticate", challenge)
            .header("Docker-Distribution-API-Version", DOCKER_API_VERSION)
            .header(CONTENT_TYPE, DOCKER_JSON_CONTENT_TYPE);

        for (key, value) in parts.headers.iter() {
            if key == "www-authenticate" || key == "content-length" || key == "content-type" {
                continue;
            }
            builder = builder.header(key, value.clone());
        }

        Ok(builder.body(body))
    } else {
        Ok(response)
    }
}

/// Public handler for /v2/{*path} routes (used at app level)
/// Parses the path and routes to the catchall handler
pub async fn handle_docker_v2_any_path(
    Path(path): Path<String>,
    State(site): State<Pkgly>,
    parent_span: Option<RequestSpan>,
    authentication: RepositoryAuthentication,
    request: Request,
) -> Result<Response, RepositoryHandlerError> {
    handle_docker_v2_catchall(
        Path(path),
        State(site),
        parent_span,
        authentication,
        request,
    )
    .await
}

#[derive(Debug, From)]
pub struct RepositoryRequestBody(Body);
impl RepositoryRequestBody {
    pub fn empty() -> Self {
        RepositoryRequestBody(Body::empty())
    }

    #[cfg(test)]
    pub fn from_bytes(bytes: Bytes) -> Self {
        RepositoryRequestBody(Body::from(bytes))
    }

    #[instrument]
    pub async fn body_as_bytes(self) -> Result<Bytes, RepositoryHandlerError> {
        // I am not sure if this error is user fault or server fault. I am going to assume it is a user fault for now
        let body = self.0.collect().await.map_err(BadRequestErrors::from)?;
        let bytes = body.to_bytes();
        Ok(bytes)
    }
    #[cfg(not(debug_assertions))]
    #[instrument]
    pub async fn body_as_json<T: for<'a> Deserialize<'a>>(
        self,
    ) -> Result<T, RepositoryHandlerError> {
        let body = self.body_as_bytes().await?;
        serde_json::from_slice(&body).map_err(RepositoryHandlerError::from)
    }
    /// In Debug mode we convert to a string so we can debug it
    #[cfg(debug_assertions)]
    #[instrument]
    pub async fn body_as_json<T: for<'a> Deserialize<'a>>(
        self,
    ) -> Result<T, RepositoryHandlerError> {
        let body = self.body_as_string().await?;
        debug!(body.len = body.len(), "Body as JSON");
        Ok(serde_json::from_str(&body).map_err(BadRequestErrors::from)?)
    }
    #[instrument]
    pub async fn body_as_string(self) -> Result<String, RepositoryHandlerError> {
        let body = self.body_as_bytes().await?;
        let body = String::from_utf8(body.to_vec()).map_err(BadRequestErrors::from)?;
        Ok(body)
    }

    pub fn into_byte_stream(
        self,
    ) -> impl futures::Stream<Item = Result<Bytes, RepositoryHandlerError>> {
        self.0.into_data_stream().map(|result| {
            result
                .map_err(BadRequestErrors::from)
                .map_err(RepositoryHandlerError::from)
        })
    }
}

#[derive(Debug)]
pub struct RepositoryRequest {
    pub parts: Parts,
    /// The body can be consumed only once
    pub body: RepositoryRequestBody,
    pub path: StoragePath,
    pub authentication: RepositoryAuthentication,
    pub auth_config: RepositoryAuthConfig,
    pub trace: RepositoryRequestTracing,
}
impl RepositoryRequest {
    pub fn user_agent_as_string(&self) -> Result<Option<&str>, BadRequestErrors> {
        let Some(header_value) = self.parts.headers.get(USER_AGENT) else {
            return Ok(None);
        };
        header_value
            .to_str()
            .map(Some)
            .map_err(BadRequestErrors::from)
    }
}
impl AsRef<Parts> for RepositoryRequest {
    fn as_ref(&self) -> &Parts {
        &self.parts
    }
}
#[derive(Debug, From)]
pub enum RepositoryRequestError {
    InvalidPath(InvalidStoragePath),
    AuthorizationError(AuthenticationError),
    BadRequestErrors(BadRequestErrors),
}
impl IntoResponse for RepositoryRequestError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::InvalidPath(err) => {
                error!(?err, "Failed to parse path");
                ResponseBuilder::bad_request().body(err.to_string())
            }
            Self::AuthorizationError(err) => {
                error!(?err, "Failed to authenticate request");
                err.into_response()
            }
            Self::BadRequestErrors(err) => {
                error!(?err, "Bad Request Error");
                err.into_response()
            }
        }
    }
}

fn response_file(
    meta: StorageFileMeta<FileFileType>,
    content: StorageFileReader,
) -> Response<Body> {
    let last_modified = date_time_for_header(meta.modified());
    // TODO: Handle cache control headers
    let FileFileType {
        file_size,
        mime_type,
        file_hash,
    } = meta.file_type();
    let mut response = ResponseBuilder::ok()
        .header(CONTENT_LENGTH, file_size.to_string())
        .header(LAST_MODIFIED, last_modified);

    if let Some(etag) = &file_hash.sha2_256 {
        response = response.header(ETAG, etag);
    }
    if let Some(mime_type) = mime_type {
        response = response.header(CONTENT_TYPE, mime_type.to_string());
    }

    let Ok(file_size) = (*file_size).try_into() else {
        // So my guess. This software is running on a 32-bit system.
        // A. Why are you still on a 32-bit system?
        // B. How do you have a 4GB file hosted on a 32-bit system?
        // Either way. You are limited to the max usize for file sizes.
        // Now if this is a 64-bit system. Interesting. You have a file that is greater than 2^64 bytes.
        // Gigabit Internet won't help you now
        return IllegalStateError("File Size is greater than the systems max integer size")
            .into_response();
    };

    let body = Body::new(content.into_body(file_size));
    response.body(body)
}

#[derive(Debug, From)]
pub enum RepoResponse {
    FileResponse(Box<StorageFile>),
    FileMetaResponse(Box<StorageFileMeta<FileType>>),
    Other(axum::response::Response),
}
impl From<StorageFileMeta<FileType>> for RepoResponse {
    fn from(meta: StorageFileMeta<FileType>) -> Self {
        RepoResponse::FileMetaResponse(Box::new(meta))
    }
}
impl RepoResponse {
    /// Default Response Format
    pub fn into_response_default(self) -> Response {
        match self {
            Self::FileResponse(file) => match *file {
                StorageFile::Directory { .. } => ResponseBuilder::default()
                    .status(StatusCode::NOT_IMPLEMENTED)
                    .header(CONTENT_TYPE, mime::TEXT_HTML.to_string())
                    .body("Build HTML Page listing"),
                StorageFile::File { meta, content } => response_file(meta, content),
            },
            Self::FileMetaResponse(meta) => {
                let last_modified = date_time_for_header(meta.modified());
                let mut response = ResponseBuilder::ok().header(LAST_MODIFIED, last_modified);
                match meta.file_type() {
                    nr_storage::FileType::Directory { .. } => {
                        response = response.header(CONTENT_TYPE, mime::TEXT_HTML.to_string());
                    }
                    nr_storage::FileType::File(FileFileType {
                        file_hash,
                        file_size,
                        mime_type,
                    }) => {
                        if let Some(etag) = &file_hash.sha2_256 {
                            response = response.header(ETAG, etag);
                        }
                        if let Some(mime_type) = mime_type {
                            response = response.header(CONTENT_TYPE, mime_type.to_string());
                        }
                        response = response.header(CONTENT_LENGTH, file_size.to_string());
                    }
                }
                response.body(Body::empty())
            }
            Self::Other(response) => response,
        }
    }
    pub fn put_response(was_created: bool, location: impl AsRef<str>) -> Self {
        let status = if was_created {
            StatusCode::CREATED
        } else {
            StatusCode::NO_CONTENT
        };
        let header = match HeaderValue::from_str(location.as_ref()) {
            Ok(ok) => ok,
            Err(err) => {
                let location = location.as_ref();
                error!(?err, ?location, "Failed to create header for location");
                return Self::internal_error(err);
            }
        };

        ResponseBuilder::default()
            .status(status)
            .header(CONTENT_LOCATION, header)
            .empty()
            .into()
    }
    pub fn require_pkgly_deploy() -> Self {
        Self::basic_text_response(
            StatusCode::BAD_REQUEST,
            "This repository requires Pkgly Deploy to push",
        )
    }
    pub fn internal_error(error: impl Error) -> Self {
        error!(?error, "Internal Error");
        ResponseBuilder::internal_server_error()
            .body(format!("Internal Error: {}", error))
            .into()
    }
    pub fn basic_text_response(status: StatusCode, message: impl Into<String>) -> Self {
        ResponseBuilder::default()
            .status(status)
            .body(message.into())
            .into()
    }
    pub fn indexing_not_allowed() -> Self {
        Self::basic_text_response(
            StatusCode::FORBIDDEN,
            "Indexing is not allowed for this repository",
        )
    }
    pub fn www_authenticate(value: &str) -> Self {
        ResponseBuilder::unauthorized()
            .header("WWW-Authenticate", value)
            .body("Unauthorized")
            .into()
    }
    pub fn unauthorized() -> Self {
        ResponseBuilder::unauthorized().body("Unauthorized").into()
    }
    pub fn forbidden() -> Self {
        ResponseBuilder::forbidden()
            .body("You do not have permission to access this repository")
            .into()
    }
    pub fn require_auth_token() -> Self {
        ResponseBuilder::unauthorized()
            .body("Authentication Token is required for this repository.")
            .into()
    }
    pub fn disabled_repository() -> Self {
        Self::basic_text_response(StatusCode::FORBIDDEN, "Repository is disabled")
    }
    pub fn unsupported_method_response(
        method: ::http::Method,
        repository_type: &str,
    ) -> RepoResponse {
        ResponseBuilder::default()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(format!(
                "Method {} is not supported for repository type {}",
                method, repository_type
            ))
            .into()
    }
}
impl From<Result<Response, http::Error>> for RepoResponse {
    fn from(result: Result<Response, http::Error>) -> Self {
        match result {
            Ok(response) => RepoResponse::Other(response),
            Err(err) => {
                error!(?err, "Failed to create response");
                RepoResponse::internal_error(err)
            }
        }
    }
}
impl From<StorageFile> for RepoResponse {
    fn from(file: StorageFile) -> Self {
        RepoResponse::FileResponse(Box::new(file))
    }
}
impl From<Option<StorageFile>> for RepoResponse {
    fn from(file: Option<StorageFile>) -> Self {
        match file {
            Some(file) => RepoResponse::FileResponse(Box::new(file)),
            None => RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "File not found"),
        }
    }
}

impl From<Option<StorageFileMeta<FileType>>> for RepoResponse {
    fn from(meta: Option<StorageFileMeta<FileType>>) -> Self {
        match meta {
            Some(meta) => RepoResponse::FileMetaResponse(Box::new(meta)),
            None => RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "File not found"),
        }
    }
}
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RepoRequestPath {
    storage: String,
    repository: String,
    #[serde(default)]
    path: Option<StoragePath>,
    #[serde(default)]
    docker_scope: Option<String>,
}

/// Core repository request handler logic (extracted for reuse)
async fn handle_repo_request_core(
    site: Pkgly,
    request_path: RepoRequestPath,
    parent_span: Option<RequestSpan>,
    authentication: RepositoryAuthentication,
    request: Request,
) -> Result<Response, RepositoryHandlerError> {
    let parent_span = parent_span.map(|span| span.0).unwrap_or(Span::current());
    let request_debug = debug_span!(
        target: "pkgly::repository::requests",
        parent: &parent_span,
        "Repository Request",
        request_path = ?request_path,
        authentication = ?authentication
    );
    async move {
        debug!(?request_path, "Repository Request Happening");
        let RepoRequestPath {
            storage,
            repository,
            path,
            docker_scope,
        } = request_path;
        let is_docker_request = docker_scope.is_some();
        let docker_scope = docker_scope.unwrap_or_else(|| format!("{}/{}", storage, repository));
        let method = request.method().clone();
        let names = RepositoryStorageName::from((storage, repository));
        let Some(repository) = site.get_repository_from_names(&names).await? else {
            if matches!(
                authentication,
                RepositoryAuthentication::NoIdentification | RepositoryAuthentication::Other(_, _)
            ) {
                if is_docker_request {
                    let actions: &[&str] = match method {
                        Method::GET | Method::HEAD => &["pull"],
                        _ => &["pull", "push"],
                    };
                    let challenge =
                        build_docker_bearer_challenge(&site, None, &docker_scope, actions);
                    let body = docker_unauthorized_body(&docker_scope, actions);
                    let response = docker_v2_unauthorized_response(&challenge, &body);
                    return Ok(response);
                } else {
                    return Ok(RepoResponse::www_authenticate("Basic realm=\"Pkgly\"")
                        .into_response_default());
                }
            }
            let not_found = RepositoryNotFound::from(names);
            return Ok(not_found.into_response());
        };
        if !repository.is_active() {
            return Ok(RepoResponse::disabled_repository().into_response_default());
        }
        let (parts, body) = request.into_parts();
        let audit_ctx = parts.extensions.get::<AccessLogContext>().cloned();
        let path = path.unwrap_or_default();
        if let Some(ctx) = &audit_ctx {
            ctx.set_repository_id(repository.id());
            ctx.set_storage_id(
                repository
                    .get_storage()
                    .storage_config()
                    .storage_config
                    .storage_id,
            );
            ctx.set_resource_kind("repository");
            ctx.set_resource_id(repository.id().to_string());
            ctx.set_resource_name(repository.name());
            ctx.set_audit_path(path.to_string());
            if let Some(user) = authentication.get_user() {
                ctx.set_user(user.username.as_ref().to_string());
                ctx.set_user_id(user.id);
            }
        }
        let trace = RepositoryRequestTracing::new(
            &repository,
            &parent_span,
            site.repository_metrics.clone(),
        );
        trace.path(&path);
        let auth_config = match site.get_repository_auth_config(repository.id()).await {
            Ok(config) => config,
            Err(err) => {
                error!(?err, "Failed to load repository auth config");
                return Ok(RepoResponse::internal_error(err).into_response_default());
            }
        };

        let request = RepositoryRequest {
            parts,
            body: RepositoryRequestBody(body),
            path: path.clone(),
            authentication,
            auth_config: auth_config.clone(),
            trace: trace.clone(),
        };

        // Authentication logic:
        // - If auth is disabled: allow reads (GET/HEAD) without auth, but require auth for writes
        // - If auth is enabled: require auth for all operations
        let is_authenticated = matches!(
            request.authentication,
            RepositoryAuthentication::AuthToken(..)
                | RepositoryAuthentication::Session(..)
                | RepositoryAuthentication::Basic(..)
        );

        let is_read_operation = matches!(method, Method::GET | Method::HEAD);
        let is_npm_login = matches!(repository, DynRepository::NPM(_))
            && crate::repository::npm::login::is_npm_login_path(&path);
        let is_npm_proxy_like = matches!(
            repository,
            DynRepository::NPM(crate::repository::npm::NPMRegistry::Proxy(_))
                | DynRepository::NPM(crate::repository::npm::NPMRegistry::Virtual(_))
        );

        let requires_auth = should_require_auth(
            &auth_config,
            is_read_operation,
            is_npm_login,
            is_npm_proxy_like,
        );

        if requires_auth && !is_authenticated {
            if let Some(ctx) = &audit_ctx {
                ctx.set_audit_action(classify_repo_audit_action(&method, &path, None));
            }
            if matches!(repository, DynRepository::Docker(_)) {
                let actions: &[&str] = if is_read_operation {
                    &["pull"][..]
                } else {
                    &["pull", "push"][..]
                };
                let challenge = build_docker_bearer_challenge(
                    &site,
                    Some(&request.parts.headers),
                    &docker_scope,
                    actions,
                );
                let body = docker_unauthorized_body(&docker_scope, actions);
                let response = docker_v2_unauthorized_response(&challenge, &body);
                return Ok(response);
            } else {
                return Ok(
                    RepoResponse::www_authenticate("Basic realm=\"Pkgly\"").into_response_default()
                );
            }
        }

        let response = match method {
            Method::GET => {
                repository
                    .handle_get(request)
                    .instrument(trace.span.clone())
                    .await
            }
            Method::POST => {
                repository
                    .handle_post(request)
                    .instrument(trace.span.clone())
                    .await
            }
            Method::PUT => {
                repository
                    .handle_put(request)
                    .instrument(trace.span.clone())
                    .await
            }
            Method::DELETE => {
                repository
                    .handle_delete(request)
                    .instrument(trace.span.clone())
                    .await
            }
            Method::PATCH => {
                repository
                    .handle_patch(request)
                    .instrument(trace.span.clone())
                    .await
            }
            Method::HEAD => {
                repository
                    .handle_head(request)
                    .instrument(trace.span.clone())
                    .await
            }
            _ => {
                repository
                    .handle_other(request)
                    .instrument(trace.span.clone())
                    .await
            }
        };

        match &response {
            Ok(ok) => {
                if let Some(ctx) = &audit_ctx {
                    ctx.set_audit_action(classify_repo_audit_action(&method, &path, Some(ok)));
                }
                trace.ok()
            }
            Err(err) => trace.error(err),
        }
        event!(Level::DEBUG, "Repository Request Completed");

        match response {
            Ok(response) => Ok(response.into_response_default()),
            Err(err) => {
                error!(?err, "Failed to handle request");
                Ok(err.into_response())
            }
        }
    }
    .instrument(request_debug)
    .await
}

fn should_require_auth(
    auth_config: &RepositoryAuthConfig,
    is_read_operation: bool,
    is_npm_login: bool,
    is_npm_proxy_like: bool,
) -> bool {
    if is_npm_login {
        // npm CLI login endpoints must be reachable without prior auth
        return false;
    }

    if is_read_operation && is_npm_proxy_like {
        // Allow anonymous reads for npm proxy/virtual repositories so tarball fetches
        // don't get blocked after metadata rewrite.
        return false;
    }

    if auth_config.enabled {
        true
    } else {
        !is_read_operation
    }
}

pub async fn handle_repo_request(
    State(site): State<Pkgly>,
    Path(request_path): Path<RepoRequestPath>,
    parent_span: Option<RequestSpan>,
    authentication: RepositoryAuthentication,
    request: Request,
) -> Result<Response, RepositoryHandlerError> {
    handle_repo_request_core(site, request_path, parent_span, authentication, request).await
}
