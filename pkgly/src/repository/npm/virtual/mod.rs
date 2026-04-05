use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use http::{Method, Request, StatusCode, request::Parts};
use nr_core::{
    database::entities::repository::{
        DBRepository, DBRepositoryConfig, DBVirtualRepositoryMember, NewVirtualRepositoryMember,
    },
    repository::{Visibility, config::RepositoryConfigType},
    storage::StoragePath,
    user::permissions::RepositoryActions,
};
use nr_storage::DynStorage;
use parking_lot::RwLock;
use tracing::warn;
use uuid::Uuid;

use super::types::request::GetPath;
use crate::repository::npm::{login::is_npm_login_path, utils::NpmRegistryExt};
use crate::repository::r#virtual::resolver::{
    ResolvedVirtualMember, VirtualMemberClient, VirtualResolutionCache, VirtualResolver,
};
use crate::{
    app::Pkgly,
    repository::{
        DynRepository, RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
        npm::{NPMRegistryConfig, NPMRegistryConfigType, NPMRegistryError},
        repo_http::{
            RepositoryAuthentication, RepositoryRequestBody,
            repo_tracing::{RepositoryMetricsMeter, RepositoryRequestTracing},
        },
        utils::can_read_repository,
    },
};

mod config;
pub use config::*;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct NpmVirtualRepository(pub(crate) Arc<NpmVirtualInner>);

#[derive(Debug)]
pub struct NpmVirtualInner {
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
}

impl NpmVirtualRepository {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: NpmVirtualConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let ttl = Duration::from_secs(config.cache_ttl_seconds.max(1).min(86_400 * 24));
        let cache = VirtualResolutionCache::new(ttl);
        let repository_id = repository.id;

        let instance = Self(Arc::new(NpmVirtualInner {
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
        }));

        instance.sync_members(&config).await?;
        Ok(instance)
    }

    async fn sync_members(&self, config: &NpmVirtualConfig) -> Result<(), RepositoryFactoryError> {
        let db_members =
            DBVirtualRepositoryConfig::load_or_seed(self.id(), config, self.site().as_ref())
                .await?;
        let mut resolved = Vec::with_capacity(db_members.len());
        for member in db_members {
            let Some(repo) =
                DBRepository::get_by_id(member.member_repository_id, self.site().as_ref()).await?
            else {
                return Err(RepositoryFactoryError::InvalidConfig(
                    NPMRegistryConfigType::get_type_static(),
                    format!("Virtual member {} missing", member.member_repository_id),
                ));
            };
            if !repo.repository_type.eq_ignore_ascii_case("npm") {
                return Err(RepositoryFactoryError::InvalidConfig(
                    NPMRegistryConfigType::get_type_static(),
                    format!("Virtual members must be NPM repositories: {}", repo.name),
                ));
            }
            if repo.id == self.id() {
                return Err(RepositoryFactoryError::InvalidConfig(
                    NPMRegistryConfigType::get_type_static(),
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
        config: &NpmVirtualConfig,
        members: &[ResolvedVirtualMember],
        database: &sqlx::PgPool,
    ) -> Result<Option<Uuid>, RepositoryFactoryError> {
        if let Some(target) = config.publish_to {
            validate_publish_target(target, members, database).await?;
            return Ok(Some(target));
        }

        for member in members.iter().filter(|m| m.enabled) {
            if is_hosted(member.repository_id, database).await? {
                return Ok(Some(member.repository_id));
            }
        }
        Ok(None)
    }

    fn resolution_order(&self) -> VirtualResolutionOrder {
        self.0.resolution_order.read().clone()
    }

    fn cache(&self) -> VirtualResolutionCache {
        self.0.cache.read().clone()
    }

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn publish_target(&self) -> Option<Uuid> {
        *self.0.publish_to.read()
    }

    fn current_members(&self) -> Vec<ResolvedVirtualMember> {
        self.0.members.read().clone()
    }

    async fn refresh_config_from_db(&self) -> Result<NpmVirtualConfig, RepositoryFactoryError> {
        let config = DBRepositoryConfig::<NPMRegistryConfig>::get_config(
            self.id(),
            NPMRegistryConfigType::get_type_static(),
            self.site().as_ref(),
        )
        .await?
        .map(|cfg| cfg.value.0)
        .unwrap_or_default();

        match config {
            NPMRegistryConfig::Virtual(config) => Ok(config),
            _ => Err(RepositoryFactoryError::InvalidConfig(
                NPMRegistryConfigType::get_type_static(),
                "Repository is not a virtual npm repo".to_string(),
            )),
        }
    }

    fn update_resolution_settings(&self, config: &NpmVirtualConfig) {
        *self.0.resolution_order.write() = config.resolution_order.clone();
        *self.0.cache.write() = {
            let mut cache = self.cache();
            cache.update_ttl(Duration::from_secs(config.cache_ttl_seconds.max(1)));
            cache
        };
    }

    fn build_resolver<C: VirtualMemberClient>(&self, client: C) -> VirtualResolver<C> {
        VirtualResolver::new(
            self.current_members(),
            self.resolution_order(),
            self.cache(),
            client,
        )
    }
}

impl Repository for NpmVirtualRepository {
    type Error = NPMRegistryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "npm"
    }

    fn full_type(&self) -> &'static str {
        "npm/virtual"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            NPMRegistryConfigType::get_type_static(),
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
}

impl NpmVirtualRepository {
    async fn handle_read(
        &self,
        request: RepositoryRequest,
        method: Method,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let can_read = can_read_repository(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
        )
        .await?;
        if !can_read {
            return Ok(RepoResponse::unauthorized());
        }

        let Some(cache_key) = derive_cache_key(&request.path)? else {
            return Ok(RepoResponse::basic_text_response(StatusCode::OK, "{}"));
        };

        let client = LiveMemberClient::new(self.clone(), &request);
        let resolver = self.build_resolver(client);
        match resolver.resolve(&cache_key, &request.path, method).await? {
            Some(hit) => Ok(hit.response),
            None => Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Package not found",
            )),
        }
    }

    async fn forward_publish(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let path_as_string = request.path.to_string();

        if is_npm_login_path(&request.path) {
            if path_as_string.starts_with(r#"-/user/org.couchdb.user:"#) {
                return super::login::couch_db::perform_login(self, request).await;
            }

            if path_as_string.eq("-/v1/login") {
                return super::login::web_login::perform_login(self, request).await;
            }
        }

        let Some(target) = self.publish_target() else {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::FORBIDDEN,
                "No publish target configured for virtual repository",
            ));
        };

        let Some(DynRepository::NPM(member_repo)) = self.site().get_repository(target) else {
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
            &DynRepository::NPM(member_repo.clone()),
            &request.trace.span,
            self.site().repository_metrics.clone(),
        );
        let auth_config = self
            .site()
            .get_repository_auth_config(target)
            .await
            .map_err(NPMRegistryError::from)?;
        let parts = clone_parts(&request.parts);
        let member_request = RepositoryRequest {
            parts,
            body: request.body,
            path: request.path,
            authentication: request.authentication,
            auth_config,
            trace,
        };

        match member_repo {
            super::NPMRegistry::Hosted(hosted) => hosted.handle_put(member_request).await,
            _ => Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_REQUEST,
                "Publish target must be a hosted repository",
            )),
        }
    }
}

#[derive(Debug, Clone)]
struct LiveMemberClient {
    site: Pkgly,
    authentication: RepositoryAuthentication,
    parts: Parts,
    trace_parent: tracing::Span,
    metrics: RepositoryMetricsMeter,
}

impl LiveMemberClient {
    fn new(repository: NpmVirtualRepository, request: &RepositoryRequest) -> Self {
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
    type Error = NPMRegistryError;

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

        let DynRepository::NPM(npm_repo) = repository else {
            return Ok(None);
        };

        if matches!(npm_repo, super::NPMRegistry::Virtual(_)) {
            warn!(repository = %member.repository_id, "Ignoring nested virtual member to prevent recursion");
            return Ok(None);
        }

        let auth_config = self
            .site
            .get_repository_auth_config(member.repository_id)
            .await
            .map_err(NPMRegistryError::from)?;

        let trace = RepositoryRequestTracing::new(
            &DynRepository::NPM(npm_repo.clone()),
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
            Method::GET => npm_repo.handle_get(request).await?,
            Method::HEAD => npm_repo.handle_head(request).await?,
            _ => RepoResponse::unsupported_method_response(method, "npm"),
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

fn derive_cache_key(path: &StoragePath) -> Result<Option<String>, NPMRegistryError> {
    match GetPath::try_from(path.clone()) {
        Ok(GetPath::GetPackageInfo { name }) => Ok(Some(name)),
        Ok(GetPath::VersionInfo { name, version }) | Ok(GetPath::GetTar { name, version, .. }) => {
            Ok(Some(format!("{name}@{version}")))
        }
        Ok(GetPath::RegistryBase) => Ok(None),
        _ => Err(NPMRegistryError::InvalidGetRequest),
    }
}

struct DBVirtualRepositoryConfig;

impl DBVirtualRepositoryConfig {
    async fn load_or_seed(
        repository_id: Uuid,
        config: &NpmVirtualConfig,
        database: &sqlx::PgPool,
    ) -> Result<
        Vec<nr_core::database::entities::repository::DBVirtualRepositoryMember>,
        RepositoryFactoryError,
    > {
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
            NPMRegistryConfigType::get_type_static(),
            format!("Publish target {target} not in member set"),
        ));
    };
    if !member.enabled {
        return Err(RepositoryFactoryError::InvalidConfig(
            NPMRegistryConfigType::get_type_static(),
            "Publish target must reference an enabled member".to_string(),
        ));
    }
    if !is_hosted(target, database).await? {
        return Err(RepositoryFactoryError::InvalidConfig(
            NPMRegistryConfigType::get_type_static(),
            "Publish target must reference a hosted repository".to_string(),
        ));
    }
    Ok(())
}

async fn is_hosted(target: Uuid, database: &sqlx::PgPool) -> Result<bool, RepositoryFactoryError> {
    let config = DBRepositoryConfig::<NPMRegistryConfig>::get_config(
        target,
        NPMRegistryConfigType::get_type_static(),
        database,
    )
    .await?
    .map(|cfg| cfg.value.0)
    .unwrap_or_default();
    Ok(matches!(config, NPMRegistryConfig::Hosted))
}

impl NpmRegistryExt for NpmVirtualRepository {}
