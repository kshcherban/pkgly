use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use http::{Method, StatusCode, request::Parts};
use http_body_util::BodyExt;
use nr_core::{
    database::entities::repository::{
        DBRepository, DBRepositoryConfig, DBVirtualRepositoryMember, NewVirtualRepositoryMember,
    },
    repository::{Visibility, config::RepositoryConfigType},
    storage::StoragePath,
    user::permissions::RepositoryActions,
};
use nr_storage::{DynStorage, Storage};
use parking_lot::RwLock;
use uuid::Uuid;

use super::{
    NugetError, NugetRepository, NugetRepositoryConfig, NugetRepositoryConfigType,
    utils::{
        REPOSITORY_TYPE_ID, RegistrationLeaf, base_repository_path, build_registration_index,
        build_registration_leaf, clone_parts, collect_registration_leaves, external_repository_base,
        json_response, parse_flatcontainer_index_versions, service_index, warn_nested_virtual,
    },
};
use crate::{
    app::Pkgly,
    repository::{
        DynRepository, RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest, repo_http::{RepositoryAuthentication, RepositoryRequestBody, repo_tracing::RepositoryRequestTracing},
        r#virtual::{config::VirtualRepositoryConfig, resolver::{ResolvedVirtualMember, VirtualMemberClient, VirtualResolutionCache, VirtualResolver}},
        utils::can_read_repository_with_auth,
    },
};

#[derive(Debug, Clone)]
pub struct NugetVirtualRepository(pub Arc<NugetVirtualInner>);

#[derive(Debug)]
pub struct NugetVirtualInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub active: bool,
    pub resolution_order: RwLock<crate::repository::r#virtual::config::VirtualResolutionOrder>,
    pub cache: RwLock<VirtualResolutionCache>,
    pub members: RwLock<Vec<ResolvedVirtualMember>>,
    pub publish_to: RwLock<Option<Uuid>>,
}

impl NugetVirtualRepository {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: VirtualRepositoryConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let ttl = Duration::from_secs(config.cache_ttl_seconds.max(1).min(86_400 * 24));
        let cache = VirtualResolutionCache::new(ttl);
        let instance = Self(Arc::new(NugetVirtualInner {
            id: repository.id,
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

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }

    fn current_members(&self) -> Vec<ResolvedVirtualMember> {
        self.0.members.read().clone()
    }

    fn resolution_order(&self) -> crate::repository::r#virtual::config::VirtualResolutionOrder {
        self.0.resolution_order.read().clone()
    }

    fn cache(&self) -> VirtualResolutionCache {
        self.0.cache.read().clone()
    }

    fn publish_target(&self) -> Option<Uuid> {
        *self.0.publish_to.read()
    }

    fn base_path(&self) -> String {
        let storage = self.storage().storage_config().storage_config.storage_name.clone();
        base_repository_path(&storage, &self.0.name)
    }

    async fn sync_members(&self, config: &VirtualRepositoryConfig) -> Result<(), RepositoryFactoryError> {
        let db_members = DBVirtualRepositoryConfig::load_or_seed(self.id(), config, self.site().as_ref()).await?;
        let mut resolved = Vec::with_capacity(db_members.len());
        for member in db_members {
            let Some(repo) = DBRepository::get_by_id(member.member_repository_id, self.site().as_ref()).await? else {
                return Err(RepositoryFactoryError::InvalidConfig(
                    NugetRepositoryConfigType::get_type_static(),
                    format!("Virtual member {} missing", member.member_repository_id),
                ));
            };
            if !repo.repository_type.eq_ignore_ascii_case(REPOSITORY_TYPE_ID) {
                return Err(RepositoryFactoryError::InvalidConfig(
                    NugetRepositoryConfigType::get_type_static(),
                    format!("Virtual members must be NuGet repositories: {}", repo.name),
                ));
            }
            if repo.id == self.id() {
                return Err(RepositoryFactoryError::InvalidConfig(
                    NugetRepositoryConfigType::get_type_static(),
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
        *self.0.members.write() = resolved;
        *self.0.publish_to.write() = target;
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
        let mut sorted: Vec<_> = members.iter().filter(|member| member.enabled).collect();
        sorted.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.repository_name.cmp(&b.repository_name)));
        for member in sorted {
            if is_hosted(member.repository_id, database).await? {
                return Ok(Some(member.repository_id));
            }
        }
        Ok(None)
    }

    async fn refresh_config_from_db(&self) -> Result<VirtualRepositoryConfig, RepositoryFactoryError> {
        let config = DBRepositoryConfig::<NugetRepositoryConfig>::get_config(
            self.id(),
            NugetRepositoryConfigType::get_type_static(),
            self.site().as_ref(),
        )
        .await?
        .map(|cfg| cfg.value.0)
        .unwrap_or_default();
        match config {
            NugetRepositoryConfig::Virtual(config) => Ok(config),
            _ => Err(RepositoryFactoryError::InvalidConfig(
                NugetRepositoryConfigType::get_type_static(),
                "Repository is not a virtual NuGet repo".to_string(),
            )),
        }
    }

    fn update_resolution_settings(&self, config: &VirtualRepositoryConfig) {
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

    async fn handle_read(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NugetError> {
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

        let path = request.path.to_string();
        if path.is_empty() || path == "v3" || path == "v3/" || path == "v3/index.json" || path == "index.json" {
            let base = external_repository_base(&self.site(), Some(&request.parts), &self.base_path());
            let body = service_index(&base, self.publish_target().is_some());
            return Ok(RepoResponse::Other(json_response(&request.parts.method, &body)));
        }

        let parts: Vec<_> = path.split('/').collect();
        let request_parts = clone_parts(&request.parts);
        let request_auth = request.authentication.clone();
        let trace_parent = request.trace.span.clone();
        if parts.len() >= 4 && parts[0] == "v3" && parts[1] == "flatcontainer" && parts[3] == "index.json" {
            return self
                .handle_flatcontainer_versions(
                    request.path.clone(),
                    request_parts.clone(),
                    request_auth.clone(),
                    trace_parent.clone(),
                    parts[2],
                )
                .await;
        }
        if parts.len() == 4 && parts[0] == "v3" && parts[1] == "registration" && parts[3] == "index.json" {
            return self
                .handle_registration_index(
                    request.path.clone(),
                    request_parts.clone(),
                    request_auth.clone(),
                    trace_parent.clone(),
                    parts[2],
                )
                .await;
        }
        if parts.len() == 4 && parts[0] == "v3" && parts[1] == "registration" && parts[3].ends_with(".json") {
            let version = parts[3].trim_end_matches(".json");
            return self
                .handle_registration_leaf(
                    request_parts.clone(),
                    request_auth.clone(),
                    trace_parent.clone(),
                    parts[2],
                    version,
                )
                .await;
        }

        let cache_key = derive_cache_key(&request.path);
        let resolver = self.build_resolver(LiveMemberClient::new(self.clone(), &request));
        match resolver.resolve(&cache_key, &request.path, request.parts.method.clone()).await? {
            Some(hit) => Ok(hit.response),
            None => Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Package not found")),
        }
    }

    async fn handle_flatcontainer_versions(
        &self,
        path: StoragePath,
        parts: Parts,
        authentication: RepositoryAuthentication,
        trace_parent: tracing::Span,
        package_id: &str,
    ) -> Result<RepoResponse, NugetError> {
        let mut versions = std::collections::BTreeSet::new();
        for member in self.current_members().into_iter().filter(|member| member.enabled) {
            let Some(response) = fetch_member_response(
                &self.site(),
                member.repository_id,
                path.clone(),
                parts.clone(),
                authentication.clone(),
                trace_parent.clone(),
                parts.method.clone(),
                self.id(),
            )
            .await?
            else {
                continue;
            };
            let RepoResponse::Other(response) = response else {
                continue;
            };
            if response.status() == StatusCode::NOT_FOUND {
                continue;
            }
            let bytes = response.into_body().collect().await?.to_bytes();
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            for version in parse_flatcontainer_index_versions(&value) {
                versions.insert(version);
            }
        }
        if versions.is_empty() {
            return Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, format!("Package {package_id} not found")));
        }
        let body = serde_json::json!({
            "versions": versions.into_iter().collect::<Vec<_>>()
        });
        Ok(RepoResponse::Other(json_response(&parts.method, &body)))
    }

    async fn handle_registration_index(
        &self,
        path: StoragePath,
        parts: Parts,
        authentication: RepositoryAuthentication,
        trace_parent: tracing::Span,
        package_id: &str,
    ) -> Result<RepoResponse, NugetError> {
        let mut leaves = Vec::<RegistrationLeaf>::new();
        for member in self.current_members().into_iter().filter(|member| member.enabled) {
            let Some(response) = fetch_member_response(
                &self.site(),
                member.repository_id,
                path.clone(),
                parts.clone(),
                authentication.clone(),
                trace_parent.clone(),
                parts.method.clone(),
                self.id(),
            )
            .await?
            else {
                continue;
            };
            let RepoResponse::Other(response) = response else {
                continue;
            };
            if response.status() == StatusCode::NOT_FOUND {
                continue;
            }
            let bytes = response.into_body().collect().await?.to_bytes();
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            leaves.extend(collect_registration_leaves(&value));
        }
        leaves.sort_by(|a, b| a.lower_version.cmp(&b.lower_version));
        leaves.dedup_by(|a, b| a.lower_version == b.lower_version);
        if leaves.is_empty() {
            return Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, format!("Package {package_id} not found")));
        }
        let base = external_repository_base(&self.site(), Some(&parts), &self.base_path());
        let body = build_registration_index(&base, package_id, &leaves);
        Ok(RepoResponse::Other(json_response(&parts.method, &body)))
    }

    async fn handle_registration_leaf(
        &self,
        parts: Parts,
        authentication: RepositoryAuthentication,
        trace_parent: tracing::Span,
        package_id: &str,
        version: &str,
    ) -> Result<RepoResponse, NugetError> {
        let leaf_path = StoragePath::from(format!(
            "v3/registration/{}/{}.json",
            package_id.to_ascii_lowercase(),
            version.to_ascii_lowercase()
        ));
        for member in self.current_members().into_iter().filter(|member| member.enabled) {
            let Some(response) = fetch_member_response(
                &self.site(),
                member.repository_id,
                leaf_path.clone(),
                parts.clone(),
                authentication.clone(),
                trace_parent.clone(),
                parts.method.clone(),
                self.id(),
            )
            .await?
            else {
                continue;
            };
            let RepoResponse::Other(response) = response else {
                continue;
            };
            if response.status() == StatusCode::NOT_FOUND {
                continue;
            }
            let bytes = response.into_body().collect().await?.to_bytes();
            let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
            if !value.is_object() {
                continue;
            }
            let base = external_repository_base(&self.site(), Some(&parts), &self.base_path());
            let leaf = collect_registration_leaves(&serde_json::json!({ "items": [value.clone()] }))
                .into_iter()
                .find(|leaf| leaf.lower_version == version.to_ascii_lowercase());
            if let Some(leaf) = leaf {
                value = build_registration_leaf(&base, package_id, &leaf);
            }
            return Ok(RepoResponse::Other(json_response(&parts.method, &value)));
        }
        Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Package not found"))
    }

    async fn forward_publish(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NugetError> {
        let Some(target) = self.publish_target() else {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::FORBIDDEN,
                "No publish target configured for virtual repository",
            ));
        };
        let Some(DynRepository::Nuget(member_repo)) = self.site().get_repository(target) else {
            return Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Publish target unavailable"));
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
            &DynRepository::Nuget(member_repo.clone()),
            &request.trace.span,
            self.site().repository_metrics.clone(),
        );
        let auth_config = self.site().get_repository_auth_config(target).await?;
        let member_request = RepositoryRequest {
            parts: clone_parts(&request.parts),
            body: request.body,
            path: request.path,
            authentication: request.authentication,
            auth_config,
            trace,
        };
        match member_repo {
            NugetRepository::Hosted(hosted) => hosted.handle_put(member_request).await,
            _ => Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_REQUEST,
                "Publish target must be a hosted repository",
            )),
        }
    }
}

impl Repository for NugetVirtualRepository {
    type Error = NugetError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn full_type(&self) -> &'static str {
        "nuget/virtual"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            NugetRepositoryConfigType::get_type_static(),
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
        self.visibility()
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
        async move { this.handle_read(request).await }
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_read(request).await }
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
struct LiveMemberClient {
    site: Pkgly,
    authentication: RepositoryAuthentication,
    parts: Parts,
    trace_parent: tracing::Span,
}

impl LiveMemberClient {
    fn new(repository: NugetVirtualRepository, request: &RepositoryRequest) -> Self {
        Self {
            site: repository.site(),
            authentication: request
                .authentication
                .clone()
                .wrap_for_virtual_reads(repository.id()),
            parts: clone_parts(&request.parts),
            trace_parent: request.trace.span.clone(),
        }
    }
}

#[async_trait]
impl VirtualMemberClient for LiveMemberClient {
    type Error = NugetError;

    async fn fetch(
        &self,
        member: &ResolvedVirtualMember,
        path: &StoragePath,
        method: Method,
    ) -> Result<Option<RepoResponse>, Self::Error> {
        let Some(repository) = self.site.get_repository(member.repository_id) else {
            return Ok(None);
        };
        let DynRepository::Nuget(repo) = repository else {
            return Ok(None);
        };
        let auth_config = self.site.get_repository_auth_config(member.repository_id).await?;
        let trace = RepositoryRequestTracing::new(
            &DynRepository::Nuget(repo.clone()),
            &self.trace_parent,
            self.site.repository_metrics.clone(),
        );
        let request = RepositoryRequest {
            parts: clone_parts(&self.parts),
            body: RepositoryRequestBody::empty(),
            path: path.clone(),
            authentication: self.authentication.clone(),
            auth_config,
            trace,
        };
        let response = match (repo, method) {
            (NugetRepository::Hosted(hosted), Method::GET) => hosted.handle_get(request).await?,
            (NugetRepository::Hosted(hosted), Method::HEAD) => hosted.handle_head(request).await?,
            (NugetRepository::Proxy(proxy), Method::GET) => proxy.handle_get(request).await?,
            (NugetRepository::Proxy(proxy), Method::HEAD) => proxy.handle_head(request).await?,
            (NugetRepository::Virtual(_), _) => {
                warn_nested_virtual(member.repository_id);
                return Ok(None);
            }
            (_, other) => RepoResponse::unsupported_method_response(other, REPOSITORY_TYPE_ID),
        };
        match response {
            RepoResponse::Other(response) if response.status() == StatusCode::NOT_FOUND => Ok(None),
            other => Ok(Some(other)),
        }
    }
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
            return Ok(DBVirtualRepositoryMember::replace_all(repository_id, &seed, database).await?);
        }
        Ok(existing)
    }
}

async fn validate_publish_target(
    target: Uuid,
    members: &[ResolvedVirtualMember],
    database: &sqlx::PgPool,
) -> Result<(), RepositoryFactoryError> {
    let Some(member) = members.iter().find(|member| member.repository_id == target) else {
        return Err(RepositoryFactoryError::InvalidConfig(
            NugetRepositoryConfigType::get_type_static(),
            format!("Publish target {target} not in member set"),
        ));
    };
    if !member.enabled {
        return Err(RepositoryFactoryError::InvalidConfig(
            NugetRepositoryConfigType::get_type_static(),
            "Publish target must reference an enabled member".to_string(),
        ));
    }
    if !is_hosted(target, database).await? {
        return Err(RepositoryFactoryError::InvalidConfig(
            NugetRepositoryConfigType::get_type_static(),
            "Publish target must reference a hosted repository".to_string(),
        ));
    }
    Ok(())
}

async fn is_hosted(target: Uuid, database: &sqlx::PgPool) -> Result<bool, RepositoryFactoryError> {
    let config = DBRepositoryConfig::<NugetRepositoryConfig>::get_config(
        target,
        NugetRepositoryConfigType::get_type_static(),
        database,
    )
    .await?
    .map(|cfg| cfg.value.0)
    .unwrap_or_default();
    Ok(matches!(config, NugetRepositoryConfig::Hosted))
}

fn derive_cache_key(path: &StoragePath) -> String {
    let path = path.to_string();
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() >= 4 && parts[0] == "v3" && parts[1] == "flatcontainer" && parts[3] == "index.json" {
        return format!("pkg:{}", parts[2]);
    }
    if parts.len() >= 5 && parts[0] == "v3" && parts[1] == "flatcontainer" {
        return format!("pkg:{}@{}", parts[2], parts[3]);
    }
    if parts.len() >= 4 && parts[0] == "v3" && parts[1] == "registration" && parts[3] == "index.json" {
        return format!("reg:{}", parts[2]);
    }
    if parts.len() >= 4 && parts[0] == "v3" && parts[1] == "registration" {
        return format!("reg:{}@{}", parts[2], parts[3]);
    }
    path
}

async fn fetch_member_response(
    site: &Pkgly,
    member_id: Uuid,
    path: StoragePath,
    parts: Parts,
    authentication: RepositoryAuthentication,
    trace_parent: tracing::Span,
    method: Method,
    virtual_repo_id: Uuid,
) -> Result<Option<RepoResponse>, NugetError> {
    let Some(repository) = site.get_repository(member_id) else {
        return Ok(None);
    };
    let DynRepository::Nuget(repo) = repository else {
        return Ok(None);
    };
    let auth_config = site.get_repository_auth_config(member_id).await?;
    let trace = RepositoryRequestTracing::new(
        &DynRepository::Nuget(repo.clone()),
        &trace_parent,
        site.repository_metrics.clone(),
    );
    let member_request = RepositoryRequest {
        parts: {
            let mut parts = parts;
            parts.method = method.clone();
            parts
        },
        body: RepositoryRequestBody::empty(),
        path,
        authentication: authentication.wrap_for_virtual_reads(virtual_repo_id),
        auth_config,
        trace,
    };
    let response = match (repo, method) {
        (NugetRepository::Hosted(hosted), Method::GET) => hosted.handle_get(member_request).await?,
        (NugetRepository::Hosted(hosted), Method::HEAD) => hosted.handle_head(member_request).await?,
        (NugetRepository::Proxy(proxy), Method::GET) => proxy.handle_get(member_request).await?,
        (NugetRepository::Proxy(proxy), Method::HEAD) => proxy.handle_head(member_request).await?,
        (NugetRepository::Virtual(_), _) => {
            warn_nested_virtual(member_id);
            return Ok(None);
        }
        (_, other) => RepoResponse::unsupported_method_response(other, REPOSITORY_TYPE_ID),
    };
    Ok(Some(response))
}
