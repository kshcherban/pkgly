use std::{fmt::Debug, path::PathBuf, sync::Arc, time::Duration};

use ahash::{HashMap, HashMapExt, RandomState};
use anyhow::{Context, anyhow};
use axum::extract::State;
use dashmap::DashMap;
use derive_more::{AsRef, derive::Deref};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use nr_core::{
    database::{
        DatabaseConfig,
        entities::{
            repository::{DBRepository, DBRepositoryConfig},
            settings::ApplicationSettings,
            storage::{DBStorage, StorageDBType},
            user::user_utils,
        },
    },
    repository::config::RepositoryConfigType,
};
use nr_storage::{DynStorage, STORAGE_FACTORIES, Storage, StorageConfig, StorageFactory};
use opentelemetry::{
    InstrumentationScope, global,
    metrics::{Counter, Histogram, Meter, MeterProvider, UpDownCounter},
};

use crate::{
    repository::{
        DynRepository, RepositoryAuthConfig, RepositoryAuthConfigType, RepositoryType,
        StagingConfig,
        cargo::{CargoRepositoryConfigType, CargoRepositoryType},
        deb::{DebRepositoryConfigType, DebRepositoryType},
        docker::{DockerPushRulesConfigType, DockerRegistryConfigType, DockerRepositoryType},
        go::{GoRepositoryConfigType, GoRepositoryType},
        helm::{HelmRepositoryConfigType, HelmRepositoryType},
        maven::{MavenPushRulesConfigType, MavenRepositoryConfigType, MavenRepositoryType},
        npm::{NPMRegistryConfigType, NpmRegistryType},
        php::{PhpRepositoryConfigType, PhpRepositoryType},
        python::{PythonRepositoryConfigType, PythonRepositoryType},
        repo_tracing::RepositoryMetricsMeter,
        ruby::{RubyRepositoryConfigType, RubyRepositoryType},
    },
    utils::ip_addr::HasForwardedHeader,
};

#[cfg(feature = "frontend")]
use crate::app::frontend::HostedFrontend;

use super::{
    BlobUploadStateHandle, FinalizedUpload, UploadState,
    authentication::{
        jwks::{JwksManager, ReqwestJwksFetcher},
        oauth::{OAuth2Rbac, OAuth2Service},
        session::{SessionManager, SessionManagerConfig},
    },
    config::{Mode, OAuth2Settings, SecuritySettings, SiteSetting, SsoSettings},
    email::EmailSetting,
    email_service::{EmailAccess, EmailService},
    state::{Instance, InstanceOAuth2Settings, InstanceSsoSettings, RepositoryStorageName},
};
use current_semver::current_semver;
use http::{HeaderName, Uri};

#[derive(Debug, Default)]
pub struct InternalServices {
    pub session_cleaner: Option<JoinHandle<()>>,
    pub email: Option<EmailService>,
    pub background_scheduler: Option<JoinHandle<()>>,
}

pub struct PkglyInner {
    pub instance: Mutex<Instance>,
    pub storages: RwLock<HashMap<Uuid, DynStorage>>,
    pub repositories: RwLock<HashMap<Uuid, DynRepository>>,
    pub name_lookup_table: Mutex<HashMap<RepositoryStorageName, Uuid>>,
    pub general_security_settings: RwLock<SecuritySettings>,
    pub oauth2_service: RwLock<Option<Arc<OAuth2Service>>>,
    pub oauth2_rbac: RwLock<Option<Arc<OAuth2Rbac>>>,
    #[cfg(feature = "frontend")]
    pub frontend: HostedFrontend,
    pub staging_config: StagingConfig,
    services: Mutex<InternalServices>,
    pub(crate) blob_upload_states: DashMap<(Uuid, String), BlobUploadStateHandle, RandomState>,
    pub suggested_local_storage_path: PathBuf,
}

use parking_lot::{Mutex, RwLock};

macro_rules! take_service {
    ($(
        $fn_name:ident => $field:ident -> $type:ty
    ),*) => {
        $(
            pub fn $fn_name(&self) -> Option<$type> {
                let mut services = self.services.lock();
                services.$field.take()
            }
        )*
    }
}

impl PkglyInner {
    take_service! {
        take_session_cleaner => session_cleaner -> JoinHandle<()>,
        take_background_scheduler => background_scheduler -> JoinHandle<()>,
        take_email => email -> EmailService
    }

    /// Notifies services that have waiters that the application is shutting down
    pub fn notify_shutdown(&self) {
        let services = self.services.lock();
        if let Some(email) = services.email.as_ref() {
            email.notify_shutdown.notify_waiters();
        }
    }
}

impl Debug for Pkgly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pkgly")
            .field("instance", &self.inner.instance.lock())
            .field("active_storages", &self.inner.storages.read().len())
            .field("active_repositories", &self.inner.repositories.read().len())
            .field("database", &self.database)
            .finish()
    }
}

/// Request Metrics based on HTTP Server Semantic Conventions.
#[derive(Debug, Clone)]
pub struct AppMetrics {
    pub meter: Meter,
    pub request_size_bytes: Histogram<u64>,
    pub response_size_bytes: Histogram<u64>,
    pub request_duration: Histogram<f64>,
    pub active_sessions: UpDownCounter<i64>,
    pub active_requests: UpDownCounter<i64>,
    pub request_count: Counter<u64>,
}

impl Default for AppMetrics {
    fn default() -> Self {
        let meter = global::meter_with_scope(Self::scope());
        Self::from_meter(meter)
    }
}

impl AppMetrics {
    fn scope() -> InstrumentationScope {
        InstrumentationScope::builder("pkgly")
            .with_schema_url("https://github.com/open-telemetry/semantic-conventions/blob/v1.29.0/docs/http/http-metrics.md")
            .with_version(env!("CARGO_PKG_VERSION"))
            .build()
    }

    pub fn with_meter_provider(provider: &impl MeterProvider) -> Self {
        let meter = provider.meter_with_scope(Self::scope());
        Self::from_meter(meter)
    }

    fn from_meter(meter: Meter) -> Self {
        AppMetrics {
            active_sessions: meter
                .i64_up_down_counter("http.server.active_sessions")
                .with_description("The number of active sessions")
                .build(),
            request_size_bytes: meter
                .u64_histogram("http.server.request.body.size")
                .with_unit("By")
                .build(),
            response_size_bytes: meter
                .u64_histogram("http.server.response.body.size")
                .with_unit("By")
                .build(),
            request_duration: meter
                .f64_histogram("http.server.request.duration")
                .with_boundaries(vec![
                    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1f64, 2.5, 5f64, 7.5,
                    10f64,
                ])
                .with_unit("s")
                .build(),
            active_requests: meter
                .i64_up_down_counter("http.server.active_requests")
                .with_description("Number of in-flight HTTP requests")
                .build(),
            request_count: meter
                .u64_counter("http.server.requests")
                .with_description("Count of completed HTTP requests")
                .build(),
            meter,
        }
    }
}

#[derive(Clone, AsRef, Deref)]
pub struct Pkgly {
    #[deref(forward)]
    pub inner: Arc<PkglyInner>,
    pub database: PgPool,
    pub session_manager: Arc<SessionManager>,
    pub email_access: Arc<EmailAccess>,
    pub metrics: AppMetrics,
    pub repository_metrics: RepositoryMetricsMeter,
    pub auth_token_cache: Arc<
        moka::future::Cache<
            String,
            (
                nr_core::database::entities::user::auth_token::AuthToken,
                nr_core::database::entities::user::UserSafeData,
            ),
        >,
    >,
    pub jwks: Arc<JwksManager<ReqwestJwksFetcher>>,
}

static X_FORWARDED_FOR_HEADER: HeaderName = HeaderName::from_static("x-forwarded-for");

impl HasForwardedHeader for Pkgly {
    fn forwarded_header(&self) -> Option<&http::HeaderName> {
        Some(&X_FORWARDED_FOR_HEADER)
    }
}

impl Pkgly {
    #[instrument]
    async fn load_database(database: DatabaseConfig) -> anyhow::Result<PgPool> {
        info!(
            user = %database.user,
            database = %database.database,
            host = %database.host,
            port = ?database.port,
            "Connecting to database"
        );
        let options = database.try_into()?;
        info!("Database connection established successfully (password masked in logs)");
        let database = PgPool::connect_with(options)
            .await
            .context("Could not connect to database")?;
        nr_core::database::migration::run_migrations(&database).await?;
        Ok(database)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mode: Mode,
        site: SiteSetting,
        security: SecuritySettings,
        session_manager: SessionManagerConfig,
        staging_config: StagingConfig,
        email_settings: Option<EmailSetting>,
        database: DatabaseConfig,
        suggested_local_storage_path: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let database = Self::load_database(database).await?;
        let stored_sso = ApplicationSettings::get::<SsoSettings>("security.sso", &database)
            .await
            .context("Failed to load stored SSO settings")?;
        let stored_oauth2 =
            ApplicationSettings::get::<OAuth2Settings>("security.oauth2", &database)
                .await
                .context("Failed to load stored OAuth2 settings")?;
        let mut security = security;
        if let Some(stored_sso) = stored_sso {
            security.sso = Some(stored_sso);
        }
        if let Some(stored_oauth2) = stored_oauth2 {
            security.oauth2 = Some(stored_oauth2);
        }

        let oauth2_service = match security.oauth2.clone() {
            Some(cfg) if cfg.enabled => match OAuth2Service::new(cfg.clone()) {
                Ok(Some(service)) => Some(Arc::new(service)),
                Ok(None) => None,
                Err(err) => {
                    warn!(%err, "Failed to initialize OAuth2 service");
                    None
                }
            },
            _ => None,
        };

        let oauth2_rbac = if let Some(cfg) = security.oauth2.clone() {
            if cfg.enabled {
                if let Some(casbin_cfg) = cfg.casbin.as_ref() {
                    match OAuth2Rbac::from_config(casbin_cfg).await {
                        Ok(rbac) => Some(Arc::new(rbac)),
                        Err(err) => {
                            warn!(%err, "Failed to initialize OAuth2 RBAC");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let is_installed = user_utils::does_user_exist(&database).await?;
        let mut instance = Instance {
            mode,
            version: current_semver!(),
            app_url: site.app_url.unwrap_or_default(),
            is_installed,
            name: site.name,
            description: site.description,
            is_https: site.is_https,
            password_rules: security.password_rules.clone(),
            sso: security
                .sso
                .as_ref()
                .filter(|cfg| cfg.enabled)
                .map(InstanceSsoSettings::from),
            oauth2: security
                .oauth2
                .as_ref()
                .filter(|cfg| cfg.enabled)
                .map(InstanceOAuth2Settings::from),
        };
        if oauth2_service.is_none() {
            instance.oauth2 = None;
        }
        let mut services = InternalServices::default();

        let (email_access, service) = EmailService::start(email_settings).await?;
        services.email = Some(service);
        let suggested_local_storage_path = if let Some(path) = suggested_local_storage_path {
            path
        } else {
            std::env::current_dir()?.join("storages")
        };
        let pkgly = PkglyInner {
            instance: Mutex::new(instance),
            storages: RwLock::new(HashMap::new()),
            repositories: RwLock::new(HashMap::new()),
            name_lookup_table: Mutex::new(HashMap::new()),
            general_security_settings: RwLock::new(security),
            oauth2_service: RwLock::new(oauth2_service),
            oauth2_rbac: RwLock::new(oauth2_rbac),
            staging_config,
            services: Mutex::new(services),
            blob_upload_states: DashMap::with_hasher(RandomState::default()),
            #[cfg(feature = "frontend")]
            frontend: HostedFrontend::new(site.frontend_path)?,
            suggested_local_storage_path,
        };

        let session_manager = Arc::new(SessionManager::new(session_manager, mode)?);

        // Initialize auth token cache with 5 minute TTL
        // Tokens expire in 15 minutes, so 5 minute cache is safe
        let auth_token_cache = Arc::new(
            moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(std::time::Duration::from_secs(300))
                .build(),
        );

        let jwks_fetcher = ReqwestJwksFetcher::new()
            .map_err(|err| anyhow!("Failed to initialize JWKS fetcher: {err}"))?;
        let jwks = Arc::new(JwksManager::new(jwks_fetcher, Duration::from_secs(3600)));

        let pkgly = Pkgly {
            inner: Arc::new(pkgly),
            session_manager,
            database,
            email_access: Arc::new(email_access),
            metrics: AppMetrics::default(),
            repository_metrics: RepositoryMetricsMeter::default(),
            auth_token_cache,
            jwks,
        };
        pkgly.load_storages().await?;
        pkgly.load_repositories().await?;
        pkgly.start_background_scheduler();
        Ok(pkgly)
    }

    /// Lock is held intentionally to prevent anything else touching the storages while they are being loaded.
    #[allow(clippy::await_holding_lock)]
    async fn load_storages(&self) -> anyhow::Result<()> {
        let mut storages = self.storages.write();
        storages.clear();

        let db_storages = DBStorage::get_all(&self.database).await?;
        let storage_configs = db_storages
            .into_iter()
            .map(StorageConfig::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        for storage_config in storage_configs {
            let id = storage_config.storage_config.storage_id;
            info!(?storage_config, "Loading storage");
            let Some(factory) =
                self.get_storage_factory(&storage_config.storage_config.storage_type)
            else {
                warn!(
                    "No storage factory found for {}",
                    storage_config.storage_config.storage_type
                );
                continue;
            };
            let storage = factory.create_storage(storage_config).await?;
            storages.insert(id, storage);
        }
        info!("Loaded {} storages", storages.len());
        Ok(())
    }

    /// Lock is held intentionally to prevent anything else touching the repositories while they are being loaded.
    #[allow(clippy::await_holding_lock)]
    async fn load_repositories(&self) -> anyhow::Result<()> {
        let mut repositories = self.repositories.write();
        repositories.clear();
        let db_repositories = DBRepository::get_all(&self.database).await?;
        for db_repository in db_repositories {
            let storage = self
                .get_storage(db_repository.storage_id)
                .context("Storage not found")?;
            let repository_type = self
                .get_repository_type(&db_repository.repository_type)
                .context("Repository type not found")?;
            let repository_id = db_repository.id;
            let repository = repository_type
                .load_repo(db_repository, storage, self.clone())
                .await?;
            repositories.insert(repository_id, repository);
        }
        info!("Loaded {} repositories", repositories.len());
        Ok(())
    }

    pub fn get_storage_factory(&self, storage_name: &str) -> Option<&'static dyn StorageFactory> {
        STORAGE_FACTORIES
            .iter()
            .find(|factory| factory.storage_name() == storage_name)
            .copied()
    }

    pub async fn close(self) {
        self.session_manager.shutdown();
        self.inner.notify_shutdown();

        let storages = {
            let mut storages = self.storages.write();
            std::mem::take(&mut *storages)
        };
        for (id, storage) in storages.into_iter() {
            info!(?id, "Unloading storage");
            storage.unload().await.unwrap_or_else(|err| {
                warn!(?id, "Failed to unload storage: {}", err);
            });
        }
        info!("Removing Logger");

        info!("Removing Email");
        let email = self.inner.take_email();
        info!("Email State has been taken");
        if let Some(email) = email {
            email.handle.abort();
        }
        let session_cleaner = self.inner.take_session_cleaner();
        if let Some(handle) = session_cleaner {
            handle.abort();
        }
        let background_scheduler = self.inner.take_background_scheduler();
        if let Some(handle) = background_scheduler {
            handle.abort();
        }
    }

    pub fn get_repository_config_type(
        &self,
        name: &str,
    ) -> Option<&'static dyn RepositoryConfigType> {
        REPOSITORY_CONFIG_TYPES
            .iter()
            .find(|config_type| config_type.get_type().eq_ignore_ascii_case(name))
            .copied()
    }

    pub fn security_settings(&self) -> SecuritySettings {
        self.inner.general_security_settings.read().clone()
    }

    pub fn sso_settings(&self) -> Option<SsoSettings> {
        self.inner
            .general_security_settings
            .read()
            .sso
            .clone()
            .filter(|cfg| cfg.enabled)
    }

    pub fn sso_settings_raw(&self) -> Option<SsoSettings> {
        self.inner.general_security_settings.read().sso.clone()
    }

    pub fn oauth2_settings(&self) -> Option<OAuth2Settings> {
        self.inner
            .general_security_settings
            .read()
            .oauth2
            .clone()
            .filter(|cfg| cfg.enabled)
    }

    pub fn oauth2_settings_raw(&self) -> Option<OAuth2Settings> {
        self.inner.general_security_settings.read().oauth2.clone()
    }

    pub fn oauth2_service(&self) -> Option<Arc<OAuth2Service>> {
        self.inner.oauth2_service.read().clone()
    }

    pub fn oauth2_rbac(&self) -> Option<Arc<OAuth2Rbac>> {
        self.inner.oauth2_rbac.read().clone()
    }

    pub async fn apply_oauth_roles(&self, subject: &str, roles: &[String]) -> anyhow::Result<()> {
        if let Some(rbac) = self.oauth2_rbac() {
            rbac.set_roles_for_user(subject, roles).await?;
        }
        Ok(())
    }

    pub async fn check_oauth_permission(
        &self,
        subject: &str,
        object: &str,
        action: &str,
    ) -> anyhow::Result<Option<bool>> {
        if let Some(rbac) = self.oauth2_rbac() {
            let decision = rbac.enforce(subject, object, action).await?;
            Ok(Some(decision))
        } else {
            Ok(None)
        }
    }

    pub async fn update_oauth2_settings(
        &self,
        settings: Option<OAuth2Settings>,
    ) -> anyhow::Result<()> {
        let mut new_service: Option<Arc<OAuth2Service>> = None;
        let mut new_rbac: Option<Arc<OAuth2Rbac>> = None;

        if let Some(cfg) = settings.clone() {
            if cfg.enabled {
                let service = OAuth2Service::new(cfg.clone())
                    .map_err(|err| anyhow!("Failed to initialize OAuth2 service: {err}"))?
                    .ok_or_else(|| {
                        anyhow!("OAuth2 configuration is missing provider credentials")
                    })?;
                new_service = Some(Arc::new(service));

                if let Some(casbin_cfg) = cfg.casbin.as_ref() {
                    let rbac = OAuth2Rbac::from_config(casbin_cfg)
                        .await
                        .map_err(|err| anyhow!("Failed to initialize OAuth2 RBAC: {err}"))?;
                    new_rbac = Some(Arc::new(rbac));
                }
            }
        }

        {
            let mut security = self.inner.general_security_settings.write();
            security.oauth2 = settings.clone();
        }
        {
            let mut instance = self.inner.instance.lock();
            instance.oauth2 = settings
                .as_ref()
                .filter(|cfg| cfg.enabled)
                .map(InstanceOAuth2Settings::from);
        }
        {
            let mut service_lock = self.inner.oauth2_service.write();
            *service_lock = new_service;
        }
        {
            let mut rbac_lock = self.inner.oauth2_rbac.write();
            *rbac_lock = new_rbac;
        }

        if let Some(settings) = settings {
            ApplicationSettings::upsert("security.oauth2", &settings, &self.database).await?;
        } else {
            ApplicationSettings::delete("security.oauth2", &self.database).await?;
        }

        Ok(())
    }

    pub fn get_repository(&self, id: Uuid) -> Option<DynRepository> {
        let repository = self.repositories.read();
        repository.get(&id).cloned()
    }

    pub fn add_storage(&self, id: Uuid, storage: DynStorage) {
        let mut storages = self.storages.write();
        storages.insert(id, storage);
    }

    pub fn replace_storage(&self, id: Uuid, storage: DynStorage) {
        let mut storages = self.storages.write();
        storages.insert(id, storage);
    }

    pub fn add_repository(&self, id: Uuid, repository: DynRepository) {
        let mut repositories = self.repositories.write();
        repositories.insert(id, repository);
    }

    pub fn loaded_repositories(&self) -> Vec<(Uuid, DynRepository)> {
        let repositories = self.repositories.read();
        repositories
            .iter()
            .map(|(id, repository)| (*id, repository.clone()))
            .collect()
    }

    pub async fn get_repository_auth_config(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositoryAuthConfig, sqlx::Error> {
        let config = DBRepositoryConfig::<RepositoryAuthConfig>::get_config(
            repository_id,
            RepositoryAuthConfigType::get_type_static(),
            &self.database,
        )
        .await?;
        Ok(config.map(|cfg| cfg.value.0).unwrap_or_default())
    }

    fn ensure_upload_state_handle(
        &self,
        repository: Uuid,
        upload_id: &str,
        sha256_only: bool,
    ) -> BlobUploadStateHandle {
        let entry = self
            .blob_upload_states
            .entry((repository, upload_id.to_owned()))
            .or_insert_with(|| {
                if sha256_only {
                    BlobUploadStateHandle::new(UploadState::new_sha256_only())
                } else {
                    BlobUploadStateHandle::new(UploadState::new())
                }
            });
        entry.value().clone()
    }

    pub fn get_upload_state_handle(
        &self,
        repository: Uuid,
        upload_id: &str,
    ) -> Option<BlobUploadStateHandle> {
        self.blob_upload_states
            .get(&(repository, upload_id.to_owned()))
            .map(|entry| entry.value().clone())
    }

    pub fn ensure_docker_blob_upload_state_handle(
        &self,
        repository: Uuid,
        upload_id: &str,
    ) -> BlobUploadStateHandle {
        self.ensure_upload_state_handle(repository, upload_id, true)
    }

    fn ensure_blob_upload_state_handle(
        &self,
        repository: Uuid,
        upload_id: &str,
    ) -> BlobUploadStateHandle {
        self.ensure_upload_state_handle(repository, upload_id, false)
    }

    pub fn update_upload_state_handle(&self, handle: &BlobUploadStateHandle, chunk: &[u8]) -> u64 {
        let mut guard = handle.lock();
        guard.update(chunk);
        guard.length
    }

    pub fn blob_upload_state_length(&self, handle: &BlobUploadStateHandle) -> u64 {
        handle.lock().length
    }

    pub fn begin_blob_upload_state(&self, repository: Uuid, upload_id: &str) {
        self.ensure_blob_upload_state_handle(repository, upload_id);
    }

    /// Begin blob upload state for Docker (SHA256 only)
    pub fn begin_docker_blob_upload_state(&self, repository: Uuid, upload_id: &str) {
        self.ensure_docker_blob_upload_state_handle(repository, upload_id);
    }

    pub fn update_blob_upload_state(&self, repository: Uuid, upload_id: &str, chunk: &[u8]) -> u64 {
        let state = self.ensure_blob_upload_state_handle(repository, upload_id);
        self.update_upload_state_handle(&state, chunk)
    }

    /// Update blob upload state for Docker (ensures SHA256-only hashing)
    pub fn update_docker_blob_upload_state(
        &self,
        repository: Uuid,
        upload_id: &str,
        chunk: &[u8],
    ) -> u64 {
        let state = self.ensure_docker_blob_upload_state_handle(repository, upload_id);
        self.update_upload_state_handle(&state, chunk)
    }

    pub fn current_blob_upload_length(&self, repository: Uuid, upload_id: &str) -> Option<u64> {
        self.get_upload_state_handle(repository, upload_id)
            .map(|handle| handle.lock().length)
    }

    pub fn finalize_blob_upload_state(
        &self,
        repository: Uuid,
        upload_id: &str,
    ) -> Option<FinalizedUpload> {
        let state = self
            .blob_upload_states
            .remove(&(repository, upload_id.to_owned()))
            .map(|(_, handle)| handle);
        state.map(|handle| match handle.try_into_state() {
            Ok(state) => state.finalize(),
            Err(handle) => {
                let mut guard = handle.lock();
                let state = guard.take();
                state.finalize()
            }
        })
    }

    pub fn abandon_blob_upload_state(&self, repository: Uuid, upload_id: &str) {
        self.blob_upload_states
            .remove(&(repository, upload_id.to_owned()));
    }

    pub fn update_app_url(&self, app_url: &Uri) {
        info!(?app_url, "Updating app url");
        // TODO: Update persisted application URL if needed.
    }

    pub async fn update_sso_settings(&self, settings: Option<SsoSettings>) -> anyhow::Result<()> {
        {
            let mut security = self.inner.general_security_settings.write();
            security.sso = settings.clone();
        }

        {
            let mut instance = self.inner.instance.lock();
            instance.sso = settings
                .as_ref()
                .filter(|cfg| cfg.enabled)
                .map(InstanceSsoSettings::from);
        }

        if let Some(settings) = settings {
            ApplicationSettings::upsert("security.sso", &settings, &self.database).await?;
        } else {
            ApplicationSettings::delete("security.sso", &self.database).await?;
        }

        Ok(())
    }

    /// Checks if a repository name and storage pair are found in the lookup table. If not queries the database.
    /// If found in the database, adds the pair to the lookup table.
    #[instrument(skip(name))]
    pub async fn get_repository_from_names(
        &self,
        name: &RepositoryStorageName,
    ) -> Result<Option<DynRepository>, sqlx::Error> {
        let id = {
            let lookup_table = self.inner.name_lookup_table.lock();
            lookup_table.get(name).cloned()
        };
        if let Some(id) = id {
            debug!(?id, ?name, "Found id in lookup table");
            let repository: Option<DynRepository> = self.get_repository(id);
            if repository.is_none() {
                warn!(?name, "Unregistered database id found in lookup table");
                {
                    let mut lookup_table = self.inner.name_lookup_table.lock();
                    lookup_table.remove(name);
                }
                return Ok(repository);
            }
            return Ok(repository);
        }
        debug!(
            ?name,
            "Name not found in lookup table. Attempting to query database"
        );
        let id = name.query_db(&self.database).await?;
        if let Some(id) = id {
            debug!(?id, ?name, "Found id in database");
            let repository: Option<DynRepository> = self.get_repository(id);
            if repository.is_none() {
                warn!(
                    ?name,
                    "Unregistered database id found. Repositories in database do not match loaded repositories"
                );
                // TODO: Reload Everything
                return Ok(repository);
            }
            let mut lookup_table = self.inner.name_lookup_table.lock();
            lookup_table.insert(name.clone(), id);

            return Ok(repository);
        }
        Ok(None)
    }

    pub fn get_storage(&self, id: Uuid) -> Option<DynStorage> {
        let storages = self.storages.read();
        storages.get(&id).cloned()
    }

    pub fn get_repository_type(&self, name: &str) -> Option<&'static dyn RepositoryType> {
        REPOSITORY_TYPES
            .iter()
            .find(|repo_type| repo_type.get_type().eq_ignore_ascii_case(name))
            .copied()
    }

    pub fn remove_repository(&self, id: Uuid) {
        {
            let mut repositories = self.repositories.write();
            repositories.remove(&id);
        }
        {
            let mut lookup_table = self.inner.name_lookup_table.lock();
            lookup_table.retain(|_, value| *value != id);
        }
    }

    fn set_session_cleaner(&self, cleaner: JoinHandle<()>) {
        let mut services = self.inner.services.lock();
        services.session_cleaner = Some(cleaner);
    }

    pub fn start_session_cleaner(&self) {
        let result = SessionManager::start_cleaner(self.clone());
        if let Some(handle) = result {
            self.set_session_cleaner(handle);
            info!("Session cleaner started");
        }
    }

    fn set_background_scheduler(&self, scheduler: JoinHandle<()>) {
        let mut services = self.inner.services.lock();
        services.background_scheduler = Some(scheduler);
    }

    fn start_background_scheduler(&self) {
        let handle = crate::app::scheduler::start_background_scheduler(self.clone());
        self.set_background_scheduler(handle);
        info!("Background scheduler started");
    }
}

pub type PkglyState = State<Pkgly>;

pub static REPOSITORY_CONFIG_TYPES: &[&dyn RepositoryConfigType] = &[
    &DockerRegistryConfigType,
    &DockerPushRulesConfigType,
    &GoRepositoryConfigType,
    &HelmRepositoryConfigType,
    &MavenRepositoryConfigType,
    &MavenPushRulesConfigType,
    &NPMRegistryConfigType,
    &CargoRepositoryConfigType,
    &PythonRepositoryConfigType,
    &PhpRepositoryConfigType,
    &DebRepositoryConfigType,
    &RubyRepositoryConfigType,
    &RepositoryAuthConfigType,
];

pub static REPOSITORY_TYPES: &[&dyn RepositoryType] = &[
    &DockerRepositoryType,
    &GoRepositoryType,
    &HelmRepositoryType,
    &CargoRepositoryType,
    &MavenRepositoryType,
    &NpmRegistryType,
    &PythonRepositoryType,
    &PhpRepositoryType,
    &DebRepositoryType,
    &RubyRepositoryType,
];
