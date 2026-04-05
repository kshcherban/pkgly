use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use axum::body::to_bytes;
use http::{Method, Request, StatusCode, header::CONTENT_LENGTH, request::Parts};
use nr_core::database::prelude::{
    DynEncodeType, FilterExpr, QueryTool, SQLOrder, SelectQueryBuilder, TableQuery, TableType,
    WhereableTool,
};
use nr_core::storage::StoragePath;
use nr_core::{
    database::entities::{
        project::{DBProject, DBProjectColumn},
        repository::{
            DBRepository, DBRepositoryConfig, DBVirtualRepositoryMember, NewVirtualRepositoryMember,
        },
    },
    repository::{Visibility, config::RepositoryConfigType},
    user::permissions::RepositoryActions,
};
use nr_storage::DynStorage;
use parking_lot::RwLock;
use tracing::warn;
use uuid::Uuid;

use crate::{
    app::Pkgly,
    error::OtherInternalError,
    repository::{
        DynRepository, RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
        repo_http::{
            RepositoryAuthentication, RepositoryRequestBody,
            repo_tracing::{RepositoryMetricsMeter, RepositoryRequestTracing},
        },
        utils::can_read_repository_with_auth,
        r#virtual::{
            config::{VirtualRepositoryConfig, VirtualResolutionOrder},
            resolver::{
                ResolvedVirtualMember, VirtualMemberClient, VirtualResolutionCache, VirtualResolver,
            },
        },
    },
    utils::ResponseBuilder,
};

use super::{
    PythonRepository, PythonRepositoryConfig, PythonRepositoryConfigType, PythonRepositoryError,
    utils::{html_escape, normalize_package_name},
};

mod simple_index;
use simple_index::{build_simple_index_html, parse_simple_index_links};

#[cfg(test)]
mod tests;

const MAX_SIMPLE_INDEX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct PythonVirtualRepository(pub(crate) Arc<PythonVirtualInner>);

#[derive(Debug)]
pub struct PythonVirtualInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub active: bool,
    pub resolution_order: RwLock<VirtualResolutionOrder>,
    pub cache: RwLock<VirtualResolutionCache>,
    pub members: RwLock<Vec<ResolvedVirtualMember>>,
    pub publish_to: RwLock<Option<Uuid>>,
    pub simple_cache: RwLock<SimpleIndexCache>,
}

#[derive(Debug, Clone)]
pub struct SimpleIndexCache {
    ttl: Duration,
    entries: ahash::HashMap<String, SimpleIndexCacheEntry>,
}

#[derive(Debug, Clone)]
struct SimpleIndexCacheEntry {
    stored_at: Instant,
    body: Vec<u8>,
}

impl SimpleIndexCache {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: ahash::HashMap::default(),
        }
    }

    fn update_ttl(&mut self, ttl: Duration) {
        self.ttl = ttl;
    }

    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        let now = Instant::now();
        if let Some(entry) = self.entries.get(key) {
            if now.duration_since(entry.stored_at) <= self.ttl {
                return Some(entry.body.clone());
            }
        }
        self.entries.remove(key);
        None
    }

    fn put(&mut self, key: impl Into<String>, body: Vec<u8>) {
        self.entries.insert(
            key.into(),
            SimpleIndexCacheEntry {
                stored_at: Instant::now(),
                body,
            },
        );
    }
}

impl PythonVirtualRepository {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: VirtualRepositoryConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let ttl = Duration::from_secs(config.cache_ttl_seconds.max(1).min(86_400 * 24));
        let cache = VirtualResolutionCache::new(ttl);
        let repository_id = repository.id;

        let instance = Self(Arc::new(PythonVirtualInner {
            id: repository_id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            storage,
            site,
            active: repository.active,
            resolution_order: RwLock::new(config.resolution_order.clone()),
            cache: RwLock::new(cache),
            members: RwLock::new(Vec::new()),
            publish_to: RwLock::new(config.publish_to),
            simple_cache: RwLock::new(SimpleIndexCache::new(ttl)),
        }));

        instance.sync_members(&config).await?;
        Ok(instance)
    }

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn id(&self) -> Uuid {
        self.0.id
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn resolution_order(&self) -> VirtualResolutionOrder {
        self.0.resolution_order.read().clone()
    }

    fn cache(&self) -> VirtualResolutionCache {
        self.0.cache.read().clone()
    }

    fn publish_target(&self) -> Option<Uuid> {
        *self.0.publish_to.read()
    }

    fn current_members(&self) -> Vec<ResolvedVirtualMember> {
        self.0.members.read().clone()
    }

    fn update_resolution_settings(&self, config: &VirtualRepositoryConfig) {
        *self.0.resolution_order.write() = config.resolution_order.clone();
        *self.0.cache.write() = {
            let mut cache = self.cache();
            cache.update_ttl(Duration::from_secs(config.cache_ttl_seconds.max(1)));
            cache
        };
        self.0
            .simple_cache
            .write()
            .update_ttl(Duration::from_secs(config.cache_ttl_seconds.max(1)));
    }

    fn build_resolver<C: VirtualMemberClient>(&self, client: C) -> VirtualResolver<C> {
        VirtualResolver::new(
            self.current_members(),
            self.resolution_order(),
            self.cache(),
            client,
        )
    }

    async fn refresh_config_from_db(
        &self,
    ) -> Result<VirtualRepositoryConfig, RepositoryFactoryError> {
        let config = DBRepositoryConfig::<PythonRepositoryConfig>::get_config(
            self.id(),
            PythonRepositoryConfigType::get_type_static(),
            self.site().as_ref(),
        )
        .await?
        .map(|cfg| cfg.value.0)
        .unwrap_or_default();

        match config {
            PythonRepositoryConfig::Virtual(config) => Ok(config),
            _ => Err(RepositoryFactoryError::InvalidConfig(
                PythonRepositoryConfigType::get_type_static(),
                "Repository is not a virtual python repo".to_string(),
            )),
        }
    }

    async fn sync_members(
        &self,
        config: &VirtualRepositoryConfig,
    ) -> Result<(), RepositoryFactoryError> {
        let db_members =
            DBVirtualRepositoryConfig::load_or_seed(self.id(), config, self.site().as_ref())
                .await?;

        let mut resolved = Vec::with_capacity(db_members.len());
        for member in db_members {
            let Some(repo) =
                DBRepository::get_by_id(member.member_repository_id, self.site().as_ref()).await?
            else {
                return Err(RepositoryFactoryError::InvalidConfig(
                    PythonRepositoryConfigType::get_type_static(),
                    format!("Virtual member {} missing", member.member_repository_id),
                ));
            };
            if !repo.repository_type.eq_ignore_ascii_case("python") {
                return Err(RepositoryFactoryError::InvalidConfig(
                    PythonRepositoryConfigType::get_type_static(),
                    format!("Virtual members must be Python repositories: {}", repo.name),
                ));
            }
            if repo.id == self.id() {
                return Err(RepositoryFactoryError::InvalidConfig(
                    PythonRepositoryConfigType::get_type_static(),
                    "Virtual repository cannot include itself".to_string(),
                ));
            }

            resolved.push(ResolvedVirtualMember {
                repository_id: repo.id,
                repository_name: repo.name.to_string(),
                priority: member.priority.max(0) as u32,
                enabled: member.enabled,
            });
        }

        let target = Self::select_publish_target(config, &resolved, self.site().as_ref()).await?;

        {
            let mut guard = self.0.members.write();
            *guard = resolved;
        }

        {
            let mut publish_guard = self.0.publish_to.write();
            *publish_guard = target;
        }

        Ok(())
    }

    async fn select_publish_target(
        config: &VirtualRepositoryConfig,
        members: &[ResolvedVirtualMember],
        database: &sqlx::PgPool,
    ) -> Result<Option<Uuid>, RepositoryFactoryError> {
        if let Some(target) = config.publish_to {
            validate_publish_target(target, members, database).await?;
            return Ok(Some(target));
        }

        let mut sorted: Vec<_> = members.iter().filter(|m| m.enabled).collect();
        sorted.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.repository_name.cmp(&b.repository_name))
        });

        for member in sorted {
            if is_hosted(member.repository_id, database).await? {
                return Ok(Some(member.repository_id));
            }
        }
        Ok(None)
    }

    async fn handle_read(
        &self,
        request: RepositoryRequest,
        method: Method,
    ) -> Result<RepoResponse, PythonRepositoryError> {
        let can_read = can_read_repository_with_auth(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &request.auth_config,
        )
        .await?;
        if !can_read {
            return Ok(RepoResponse::unauthorized());
        }

        let uri_path = request.parts.uri.path();
        if let Some(simple_request) = SimpleRequest::try_from_request(&request.path, uri_path) {
            if simple_request.redirect_needed {
                return Ok(RepoResponse::Other(redirect_to_trailing_slash(uri_path)));
            }

            match simple_request.kind {
                SimpleRequestKind::Root => {
                    let response = self.simple_root(method).await?;
                    return Ok(RepoResponse::Other(response));
                }
                SimpleRequestKind::Package { package_component } => {
                    let ctx = MemberRequestContext {
                        parts: clone_parts(&request.parts),
                        path: request.path.clone(),
                        authentication: request.authentication.clone(),
                        trace_parent: request.trace.span.clone(),
                    };
                    let response = self.simple_package(ctx, &package_component, method).await?;
                    return Ok(RepoResponse::Other(response));
                }
            }
        }

        let cache_key = request.path.to_string();
        let client = LiveMemberClient::new(self.clone(), &request);
        let resolver = self.build_resolver(client);
        match resolver.resolve(&cache_key, &request.path, method).await? {
            Some(hit) => Ok(hit.response),
            None => Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Resource not found",
            )),
        }
    }

    async fn simple_root(
        &self,
        method: Method,
    ) -> Result<axum::response::Response, PythonRepositoryError> {
        let cache_key = "simple:root";
        if matches!(method, Method::GET | Method::HEAD) {
            let cached = { self.0.simple_cache.write().get(cache_key) };
            if let Some(body) = cached {
                return Ok(response_html(method, body));
            }
        }

        let mut packages = BTreeSet::new();
        for member in self.current_members().into_iter().filter(|m| m.enabled) {
            if !is_hosted(member.repository_id, self.site().as_ref()).await? {
                continue;
            }
            let projects =
                list_projects_for_repository(member.repository_id, self.site().as_ref()).await?;
            for project in projects {
                packages.insert(project.key);
            }
        }

        let body = build_simple_root_html(&packages).into_bytes();
        self.0.simple_cache.write().put(cache_key, body.clone());
        Ok(response_html(method, body))
    }

    async fn simple_package(
        &self,
        request: MemberRequestContext,
        package_component: &str,
        method: Method,
    ) -> Result<axum::response::Response, PythonRepositoryError> {
        let normalized = normalize_package_name(package_component);
        let cache_key = format!("simple:pkg:{normalized}");

        if matches!(method, Method::GET | Method::HEAD) {
            let cached = { self.0.simple_cache.write().get(&cache_key) };
            if let Some(body) = cached {
                return Ok(response_html(method, body));
            }
        }

        let members = sorted_enabled_members(&self.current_members());
        let mut merged = Vec::new();
        let mut auth_failure: Option<axum::response::Response> = None;

        for member in members {
            let Some(DynRepository::Python(py_repo)) =
                self.site().get_repository(member.repository_id)
            else {
                continue;
            };
            if matches!(py_repo, PythonRepository::Virtual(_)) {
                warn!(repository = %member.repository_id, "Ignoring nested virtual member to prevent recursion");
                continue;
            }

            let auth_config = self
                .site()
                .get_repository_auth_config(member.repository_id)
                .await
                .map_err(PythonRepositoryError::from)?;

            let trace = RepositoryRequestTracing::new(
                &DynRepository::Python(py_repo.clone()),
                &request.trace_parent,
                self.site().repository_metrics.clone(),
            );

            let member_request = RepositoryRequest {
                parts: clone_parts(&request.parts),
                body: RepositoryRequestBody::empty(),
                path: request.path.clone(),
                authentication: request
                    .authentication
                    .clone()
                    .wrap_for_virtual_reads(self.id()),
                auth_config,
                trace,
            };

            let member_response = Box::pin(py_repo.handle_get(member_request)).await?;

            let RepoResponse::Other(resp) = member_response else {
                continue;
            };

            let status = resp.status();
            if status == StatusCode::NOT_FOUND {
                continue;
            }
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                if auth_failure.is_none() {
                    auth_failure = Some(resp);
                }
                continue;
            }
            if status != StatusCode::OK {
                continue;
            }

            let bytes = to_bytes(resp.into_body(), MAX_SIMPLE_INDEX_BYTES)
                .await
                .map_err(|err| {
                    PythonRepositoryError::Other(Box::new(OtherInternalError::new(err)))
                })?;
            let body_str = std::str::from_utf8(&bytes).map_err(|err| {
                PythonRepositoryError::Other(Box::new(OtherInternalError::new(err)))
            })?;

            let links = parse_simple_index_links(body_str);
            if links.is_empty() {
                continue;
            }
            merged.push((member.priority, member.repository_name.clone(), links));
        }

        if merged.is_empty() {
            if let Some(failure) = auth_failure {
                return Ok(failure);
            }
            let message = format!(
                "<!DOCTYPE html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Package not found</title>\n  </head>\n  <body>\n    <p>Package {} not found.</p>\n  </body>\n</html>\n",
                html_escape(package_component)
            );
            let body = message.into_bytes();
            return Ok(response_html(method, body));
        }

        let body = build_simple_index_html(package_component, merged).into_bytes();
        self.0.simple_cache.write().put(cache_key, body.clone());
        Ok(response_html(method, body))
    }

    async fn forward_publish(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, PythonRepositoryError> {
        let Some(target) = self.publish_target() else {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::FORBIDDEN,
                "No publish target configured for virtual repository",
            ));
        };

        let Some(DynRepository::Python(member_repo)) = self.site().get_repository(target) else {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Publish target unavailable",
            ));
        };

        let has_write = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, target, self.site().as_ref())
            .await?
            .is_some();
        if !has_write {
            return Ok(RepoResponse::unauthorized());
        }

        let trace = RepositoryRequestTracing::new(
            &DynRepository::Python(member_repo.clone()),
            &request.trace.span,
            self.site().repository_metrics.clone(),
        );
        let auth_config = self
            .site()
            .get_repository_auth_config(target)
            .await
            .map_err(PythonRepositoryError::from)?;

        let member_request = RepositoryRequest {
            parts: clone_parts(&request.parts),
            body: request.body,
            path: request.path,
            authentication: request.authentication,
            auth_config,
            trace,
        };

        match member_repo {
            PythonRepository::Hosted(hosted) => {
                let method = member_request.parts.method.clone();
                let response = match method {
                    Method::PUT => hosted.handle_put(member_request).await?,
                    Method::POST => hosted.handle_post(member_request).await?,
                    _ => hosted.handle_post(member_request).await?,
                };
                Ok(response)
            }
            _ => Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_REQUEST,
                "Publish target must be a hosted repository",
            )),
        }
    }
}

impl Repository for PythonVirtualRepository {
    type Error = PythonRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "python"
    }

    fn full_type(&self) -> &'static str {
        "python/virtual"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            PythonRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn name(&self) -> String {
        self.0.name.clone()
    }

    fn id(&self) -> Uuid {
        self.0.id
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }

    fn is_active(&self) -> bool {
        self.0.active
    }

    fn site(&self) -> Pkgly {
        self.site()
    }

    async fn reload(&self) -> Result<(), RepositoryFactoryError> {
        let config = self.refresh_config_from_db().await?;
        self.update_resolution_settings(&config);
        self.sync_members(&config).await
    }

    fn handle_get(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_read(request, Method::GET).await }
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_read(request, Method::HEAD).await }
    }

    fn handle_put(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.forward_publish(request).await }
    }

    fn handle_post(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.forward_publish(request).await }
    }
}

#[derive(Debug, Clone)]
struct MemberRequestContext {
    parts: Parts,
    path: StoragePath,
    authentication: RepositoryAuthentication,
    trace_parent: tracing::Span,
}

#[derive(Debug)]
struct LiveMemberClient {
    site: Pkgly,
    authentication: RepositoryAuthentication,
    parts: Parts,
    trace_parent: tracing::Span,
    metrics: RepositoryMetricsMeter,
}

impl LiveMemberClient {
    fn new(repository: PythonVirtualRepository, request: &RepositoryRequest) -> Self {
        Self {
            site: repository.site(),
            authentication: request
                .authentication
                .clone()
                .wrap_for_virtual_reads(repository.id()),
            parts: clone_parts(&request.parts),
            trace_parent: request.trace.span.clone(),
            metrics: repository.site().repository_metrics.clone(),
        }
    }
}

#[async_trait]
impl VirtualMemberClient for LiveMemberClient {
    type Error = PythonRepositoryError;

    async fn fetch(
        &self,
        member: &ResolvedVirtualMember,
        path: &StoragePath,
        method: Method,
    ) -> Result<Option<RepoResponse>, Self::Error> {
        let Some(repository) = self.site.get_repository(member.repository_id) else {
            warn!(repository = %member.repository_id, "Skipping missing member repository");
            return Ok(None);
        };

        let DynRepository::Python(py_repo) = repository else {
            return Ok(None);
        };

        if matches!(py_repo, PythonRepository::Virtual(_)) {
            warn!(repository = %member.repository_id, "Ignoring nested virtual member to prevent recursion");
            return Ok(None);
        }

        let auth_config = self
            .site
            .get_repository_auth_config(member.repository_id)
            .await
            .map_err(PythonRepositoryError::from)?;

        let trace = RepositoryRequestTracing::new(
            &DynRepository::Python(py_repo.clone()),
            &self.trace_parent,
            self.metrics.clone(),
        );

        let request = RepositoryRequest {
            parts: clone_parts(&self.parts),
            body: RepositoryRequestBody::empty(),
            path: path.clone(),
            authentication: self.authentication.clone(),
            auth_config,
            trace,
        };

        let response = match method {
            Method::GET => Box::pin(py_repo.handle_get(request)).await?,
            Method::HEAD => Box::pin(py_repo.handle_head(request)).await?,
            _ => RepoResponse::unsupported_method_response(method, "python"),
        };

        match response {
            RepoResponse::Other(resp) if resp.status() == StatusCode::NOT_FOUND => Ok(None),
            other => Ok(Some(other)),
        }
    }
}

fn clone_parts(parts: &Parts) -> Parts {
    let mut builder = Request::builder()
        .method(parts.method.clone())
        .uri(parts.uri.clone())
        .version(parts.version);
    if let Some(headers) = builder.headers_mut() {
        *headers = parts.headers.clone();
    }
    builder
        .body(())
        .expect("cloning Parts into new request should be infallible")
        .into_parts()
        .0
}

fn response_html(method: Method, body: Vec<u8>) -> axum::response::Response {
    if method == Method::HEAD {
        return ResponseBuilder::ok()
            .header(http::header::CONTENT_TYPE, mime::TEXT_HTML.to_string())
            .header(CONTENT_LENGTH, body.len().to_string())
            .empty();
    }
    ResponseBuilder::ok()
        .header(http::header::CONTENT_TYPE, mime::TEXT_HTML.to_string())
        .body(body)
}

fn build_simple_root_html(packages: &BTreeSet<String>) -> String {
    let mut body = String::from(
        "<!DOCTYPE html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Simple index</title>\n  </head>\n  <body>\n",
    );

    if packages.is_empty() {
        body.push_str("    <p>No packages available.</p>\n");
    } else {
        for package in packages {
            body.push_str("    <a href=\"");
            body.push_str(&html_escape(package));
            body.push_str("/\">");
            body.push_str(&html_escape(package));
            body.push_str("</a><br/>\n");
        }
    }

    body.push_str("  </body>\n</html>\n");
    body
}

async fn list_projects_for_repository(
    repository_id: Uuid,
    database: &sqlx::PgPool,
) -> Result<Vec<DBProject>, PythonRepositoryError> {
    let projects = SelectQueryBuilder::with_columns(DBProject::table_name(), DBProject::columns())
        .filter(DBProjectColumn::RepositoryId.equals(repository_id.value()))
        .order_by(DBProjectColumn::Key, SQLOrder::Ascending)
        .query_as()
        .fetch_all(database)
        .await?;
    Ok(projects)
}

fn redirect_to_trailing_slash(uri_path: &str) -> axum::response::Response {
    let location = if uri_path.is_empty() {
        String::from("/")
    } else if uri_path.ends_with('/') {
        uri_path.to_string()
    } else {
        format!("{}/", uri_path)
    };

    ResponseBuilder::default()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(http::header::LOCATION, location)
        .empty()
}

#[derive(Debug, Clone)]
struct SimpleRequest {
    kind: SimpleRequestKind,
    redirect_needed: bool,
}

#[derive(Debug, Clone)]
enum SimpleRequestKind {
    Root,
    Package { package_component: String },
}

impl SimpleRequest {
    fn try_from_request(path: &StoragePath, uri_path: &str) -> Option<Self> {
        let mut components: Vec<String> = path.clone().into_iter().map(String::from).collect();
        if components.first().map(|c| c.as_str()) != Some("simple") {
            return None;
        }
        components.remove(0);

        let trailing_slash = uri_path.ends_with('/');
        let is_directory =
            path.is_directory() || trailing_slash || components.is_empty() || components.len() == 1;
        if !is_directory {
            return None;
        }
        let redirect_needed = is_directory && !trailing_slash;

        let kind = match components.len() {
            0 => SimpleRequestKind::Root,
            1 => SimpleRequestKind::Package {
                package_component: components[0].clone(),
            },
            _ => return None,
        };

        Some(Self {
            kind,
            redirect_needed,
        })
    }
}

fn sorted_enabled_members(members: &[ResolvedVirtualMember]) -> Vec<ResolvedVirtualMember> {
    let mut members: Vec<_> = members.iter().cloned().filter(|m| m.enabled).collect();
    members.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.repository_name.cmp(&b.repository_name))
    });
    members
}

struct DBVirtualRepositoryConfig;

impl DBVirtualRepositoryConfig {
    async fn load_or_seed(
        repository_id: Uuid,
        config: &VirtualRepositoryConfig,
        database: &sqlx::PgPool,
    ) -> Result<Vec<DBVirtualRepositoryMember>, RepositoryFactoryError> {
        let existing = DBVirtualRepositoryMember::list_for_virtual(repository_id, database).await?;
        if existing.is_empty() && !config.member_repositories.is_empty() {
            let seed: Vec<NewVirtualRepositoryMember> = config
                .member_repositories
                .iter()
                .map(|member| NewVirtualRepositoryMember {
                    member_repository_id: member.repository_id,
                    priority: member.priority as i32,
                    enabled: member.enabled,
                })
                .collect();
            return Ok(
                DBVirtualRepositoryMember::replace_all(repository_id, &seed, database).await?,
            );
        }
        Ok(existing)
    }
}

async fn validate_publish_target(
    target: Uuid,
    members: &[ResolvedVirtualMember],
    database: &sqlx::PgPool,
) -> Result<(), RepositoryFactoryError> {
    let Some(member) = members.iter().find(|m| m.repository_id == target) else {
        return Err(RepositoryFactoryError::InvalidConfig(
            PythonRepositoryConfigType::get_type_static(),
            format!("Publish target {target} not in member set"),
        ));
    };
    if !member.enabled {
        return Err(RepositoryFactoryError::InvalidConfig(
            PythonRepositoryConfigType::get_type_static(),
            "Publish target must reference an enabled member".to_string(),
        ));
    }
    if !is_hosted(target, database).await? {
        return Err(RepositoryFactoryError::InvalidConfig(
            PythonRepositoryConfigType::get_type_static(),
            "Publish target must reference a hosted repository".to_string(),
        ));
    }
    Ok(())
}

async fn is_hosted(target: Uuid, database: &sqlx::PgPool) -> Result<bool, sqlx::Error> {
    let config = DBRepositoryConfig::<PythonRepositoryConfig>::get_config(
        target,
        PythonRepositoryConfigType::get_type_static(),
        database,
    )
    .await?
    .map(|cfg| cfg.value.0)
    .unwrap_or_default();
    Ok(matches!(config, PythonRepositoryConfig::Hosted))
}
