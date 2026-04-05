use std::sync::{
    Arc,
    atomic::{self, AtomicBool},
};

use derive_more::derive::Deref;
use nr_core::{
    database::entities::repository::DBRepository,
    repository::{
        Visibility,
        config::{RepositoryConfigType, get_repository_config_or_default},
    },
};
use nr_storage::DynStorage;
use parking_lot::RwLock;
use reqwest::Url;
use tracing::{debug, error, instrument};
use uuid::Uuid;

use super::{
    DockerError, DockerPushRules, DockerPushRulesConfigType, REPOSITORY_TYPE_ID, RepoResponse,
    RepositoryRequest,
};
use crate::{
    app::Pkgly,
    repository::docker::DockerRegistryConfigType,
    repository::{Repository, RepositoryAuthConfigType, RepositoryFactoryError},
};

#[derive(derive_more::Debug)]
pub struct DockerHostedInner {
    pub id: Uuid,
    pub name: String,
    pub active: AtomicBool,
    pub visibility: RwLock<Visibility>,
    pub push_rules: RwLock<DockerPushRules>,
    /// Whether Docker manifest pushes should write catalog (projects/project_versions) entries.
    ///
    /// Some non-Docker repositories (e.g. Helm in OCI mode) reuse Docker V2 endpoints for storage,
    /// but manage their own catalog entries. In those cases, writing Docker manifest entries would
    /// conflict with the repository's catalog and can violate unique path constraints.
    pub catalog_indexing_enabled: bool,
    #[debug(skip)]
    pub storage: DynStorage,
    #[debug(skip)]
    pub site: Pkgly,
    #[debug(skip)]
    pub proxy: Option<ProxySettings>,
}

#[derive(Debug, Clone, Deref)]
pub struct DockerHosted(Arc<DockerHostedInner>);

#[derive(Debug, Clone)]
pub struct ProxySettings {
    pub upstream: Url,
    pub client: reqwest::Client,
}

impl DockerHosted {
    pub(crate) fn catalog_indexing_enabled_for_repository_type(repository_type: &str) -> bool {
        repository_type == super::REPOSITORY_TYPE_ID
    }

    pub(crate) fn catalog_indexing_enabled(&self) -> bool {
        self.0.catalog_indexing_enabled
    }

    pub async fn load(
        repository: DBRepository,
        storage: DynStorage,
        site: Pkgly,
    ) -> Result<Self, RepositoryFactoryError> {
        let push_rules_db = get_repository_config_or_default::<
            DockerPushRulesConfigType,
            DockerPushRules,
        >(repository.id, site.as_ref())
        .await?;
        debug!("Loaded Docker Push Rules Config: {:?}", push_rules_db);

        let active = AtomicBool::new(repository.active);
        let catalog_indexing_enabled =
            Self::catalog_indexing_enabled_for_repository_type(&repository.repository_type);

        let inner = DockerHostedInner {
            id: repository.id,
            name: repository.name.into(),
            active,
            visibility: RwLock::new(repository.visibility),
            push_rules: RwLock::new(push_rules_db.value.0),
            catalog_indexing_enabled,
            storage,
            site,
            proxy: None,
        };

        Ok(Self(Arc::new(inner)))
    }

    pub async fn load_proxy(
        repository: DBRepository,
        storage: DynStorage,
        site: Pkgly,
        upstream_url: &str,
    ) -> Result<Self, RepositoryFactoryError> {
        let mut hosted = Self::load(repository, storage, site).await?;
        let upstream = Url::parse(upstream_url).map_err(|err| {
            RepositoryFactoryError::InvalidConfig(super::REPOSITORY_TYPE_ID, err.to_string())
        })?;
        let client = reqwest::Client::new();
        Arc::get_mut(&mut hosted.0)
            .expect("no other references during construction")
            .proxy = Some(ProxySettings { upstream, client });
        Ok(hosted)
    }
}

impl Repository for DockerHosted {
    type Error = DockerError;

    #[inline(always)]
    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    #[inline(always)]
    fn get_storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    #[inline(always)]
    fn visibility(&self) -> Visibility {
        *self.visibility.read()
    }

    #[inline(always)]
    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn full_type(&self) -> &'static str {
        "docker/hosted"
    }

    #[inline(always)]
    fn name(&self) -> String {
        self.0.name.clone()
    }

    #[inline(always)]
    fn id(&self) -> Uuid {
        self.0.id
    }

    #[inline(always)]
    fn is_active(&self) -> bool {
        self.active.load(atomic::Ordering::Relaxed)
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            DockerRegistryConfigType::get_type_static(),
            DockerPushRulesConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    #[instrument(fields(repository_type = "docker/hosted"))]
    async fn reload(&self) -> Result<(), RepositoryFactoryError> {
        let Some(is_active) = DBRepository::get_active_by_id(self.id, self.site.as_ref()).await?
        else {
            error!("Failed to get repository");
            self.0.active.store(false, atomic::Ordering::Relaxed);
            return Ok(());
        };
        self.0.active.store(is_active, atomic::Ordering::Relaxed);

        let push_rules_db = get_repository_config_or_default::<
            DockerPushRulesConfigType,
            DockerPushRules,
        >(self.id, self.site.as_ref())
        .await?;

        {
            let mut push_rules = self.push_rules.write();
            *push_rules = push_rules_db.value.0;
        }

        Ok(())
    }

    async fn handle_get(&self, request: RepositoryRequest) -> Result<RepoResponse, Self::Error> {
        super::handlers::handle_get(self.clone(), request).await
    }

    async fn handle_put(&self, request: RepositoryRequest) -> Result<RepoResponse, Self::Error> {
        super::handlers::handle_put(self.clone(), request).await
    }

    async fn handle_post(&self, request: RepositoryRequest) -> Result<RepoResponse, Self::Error> {
        super::handlers::handle_post(self.clone(), request).await
    }

    async fn handle_patch(&self, request: RepositoryRequest) -> Result<RepoResponse, Self::Error> {
        super::handlers::handle_patch(self.clone(), request).await
    }

    async fn handle_delete(&self, request: RepositoryRequest) -> Result<RepoResponse, Self::Error> {
        super::handlers::handle_delete(self.clone(), request).await
    }

    async fn handle_head(&self, request: RepositoryRequest) -> Result<RepoResponse, Self::Error> {
        super::handlers::handle_head(self.clone(), request).await
    }
}

impl DockerHosted {
    pub fn is_proxy(&self) -> bool {
        self.0.proxy.is_some()
    }

    pub fn upstream(&self) -> Option<&ProxySettings> {
        self.0.proxy.as_ref()
    }
}
