use std::sync::{Arc, LazyLock};

use chrono::Utc;
use http::{
    StatusCode,
    header::{CONTENT_LENGTH, CONTENT_TYPE},
};
use nr_core::{
    database::entities::repository::DBRepository,
    repository::{
        Visibility,
        config::RepositoryConfigType,
        project::{ProxyArtifactKey, ProxyArtifactMeta, VersionData},
        proxy_url::ProxyURL,
    },
    storage::StoragePath,
};
use nr_storage::{DynStorage, FileContent, Storage, StorageFile};
use parking_lot::{RwLock, RwLockReadGuard};
use serde_json::Value;
use sqlx;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;

use super::{
    ComposerDistPath, ComposerRootIndex, PhpRepositoryError,
    configs::{PhpProxyConfig, PhpProxyRoute, PhpRepositoryConfigType},
};
use crate::{
    app::Pkgly,
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
        proxy::base_proxy::{evict_proxy_cache_entry, record_proxy_cache_hit},
        proxy_indexing::{DatabaseProxyIndexer, ProxyIndexing},
        utils::{RepositoryExt, can_read_repository_with_auth},
    },
    utils::ResponseBuilder,
};

const UPSTREAM_URL_FIELD: &str = "pkgly-upstream-url";

pub static DEFAULT_ROUTE: LazyLock<PhpProxyRoute> = LazyLock::new(|| PhpProxyRoute {
    url: ProxyURL::try_from(String::from("https://repo.packagist.org"))
        .expect("valid Packagist URL"),
    name: Some("Packagist".to_string()),
});

pub(super) fn normalize_routes(routes: Vec<PhpProxyRoute>) -> Vec<PhpProxyRoute> {
    if routes.is_empty() {
        vec![DEFAULT_ROUTE.clone()]
    } else {
        routes
    }
}

pub struct PhpProxyInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub routes: RwLock<Vec<PhpProxyRoute>>,
    pub client: reqwest::Client,
    pub active: bool,
    pub storage_name: String,
    pub indexer: Arc<dyn ProxyIndexing>,
}

#[derive(Debug, Clone)]
pub struct PhpProxy(pub Arc<PhpProxyInner>);

impl std::fmt::Debug for PhpProxyInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhpProxyInner")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("visibility", &self.visibility.read())
            .field("storage_name", &self.storage_name)
            .field("active", &self.active)
            .finish()
    }
}

impl PhpProxy {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: PhpProxyConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let client = reqwest::Client::builder()
            .user_agent("Pkgly PHP Proxy")
            .build()
            .map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    PhpRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;
        let storage_name = storage.storage_config().storage_config.storage_name.clone();
        let indexer: Arc<dyn ProxyIndexing> =
            Arc::new(DatabaseProxyIndexer::new(site.clone(), repository.id));
        Ok(Self(Arc::new(PhpProxyInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            storage,
            site,
            routes: RwLock::new(normalize_routes(config.routes)),
            client,
            active: repository.active,
            storage_name,
            indexer,
        })))
    }

    fn id(&self) -> Uuid {
        self.0.id
    }

    fn name(&self) -> &str {
        &self.0.name
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }

    fn routes(&self) -> RwLockReadGuard<'_, Vec<PhpProxyRoute>> {
        self.0.routes.read()
    }

    fn client(&self) -> &reqwest::Client {
        &self.0.client
    }

    fn indexer(&self) -> &Arc<dyn ProxyIndexing> {
        &self.0.indexer
    }

    fn storage_name(&self) -> &str {
        &self.0.storage_name
    }

    fn base_repository_path(&self) -> String {
        format!("/repositories/{}/{}", self.storage_name(), self.name())
    }

    /// Compute the external Composer base URL (scheme + host + repository path)
    /// used when rewriting dist URLs for clients like Composer.
    fn external_base_url(&self, host: Option<&str>) -> String {
        let site = self.site();
        let instance = site.inner.instance.lock();
        let scheme = if instance.is_https { "https" } else { "http" };

        if let Some(host) = host {
            return format!("{scheme}://{host}{}", self.base_repository_path());
        }

        if !instance.app_url.is_empty() {
            return format!(
                "{}{}",
                instance.app_url.trim_end_matches('/'),
                self.base_repository_path()
            );
        }

        format!("{scheme}://localhost:6742{}", self.base_repository_path())
    }

    fn metadata_path(&self, vendor: &str, package: &str, is_dev: bool) -> StoragePath {
        let suffix = if is_dev { "~dev" } else { "" };
        StoragePath::from(format!(
            "p2/{}/{}{}.json",
            vendor.to_ascii_lowercase(),
            package.to_ascii_lowercase(),
            suffix
        ))
    }

    fn dist_storage_path(&self, dist: &ComposerDistPath) -> StoragePath {
        StoragePath::from(format!(
            "dist/{}/{}/{}/{}",
            dist.vendor.to_ascii_lowercase(),
            dist.package.to_ascii_lowercase(),
            dist.version,
            dist.filename
        ))
    }

    pub async fn handle_external_eviction(
        &self,
        path: &StoragePath,
    ) -> Result<(), PhpRepositoryError> {
        let key = proxy_key_from_cache_path(path);
        evict_proxy_cache_entry(self.indexer().as_ref(), key).await?;
        Ok(())
    }

    async fn handle_root_index(&self) -> Result<RepoResponse, PhpRepositoryError> {
        let index = ComposerRootIndex::new(self.storage_name(), self.name());
        Ok(RepoResponse::Other(ResponseBuilder::ok().json(&index)))
    }

    async fn handle_metadata_request(
        &self,
        request: RepositoryRequest,
        vendor: String,
        package: String,
        is_dev: bool,
    ) -> Result<RepoResponse, PhpRepositoryError> {
        if !can_read_repository_with_auth(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &request.auth_config,
        )
        .await?
        {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::UNAUTHORIZED,
                "Missing permission to read repository",
            ));
        }

        let host = request
            .parts
            .headers
            .get(http::header::HOST)
            .and_then(|h| h.to_str().ok());
        let base_url = self.external_base_url(host);

        let metadata_path = self.metadata_path(&vendor, &package, is_dev);
        if let Some(file) = self.storage().open_file(self.id(), &metadata_path).await? {
            return Ok(file.into());
        }

        if let Some(response) = self
            .fetch_and_cache_metadata(&metadata_path, request.parts.uri.query(), &base_url)
            .await?
        {
            return Ok(response);
        }

        Ok(RepoResponse::basic_text_response(
            StatusCode::NOT_FOUND,
            "Metadata not found",
        ))
    }

    async fn fetch_and_cache_metadata(
        &self,
        metadata_path: &StoragePath,
        query: Option<&str>,
        base_url: &str,
    ) -> Result<Option<RepoResponse>, PhpRepositoryError> {
        let routes = { self.routes().clone() };
        for route in routes.iter() {
            let Some(url) = build_url(&route.url, metadata_path.clone(), query) else {
                continue;
            };
            let response =
                match crate::utils::upstream::send(self.client(), self.client().get(url.clone()))
                    .await
                {
                    Ok(resp) => resp,
                    Err(err) => {
                        warn!(?err, %url, "PHP proxy metadata fetch failed");
                        continue;
                    }
                };
            if response.status() == StatusCode::NOT_FOUND {
                continue;
            }
            if !response.status().is_success() {
                warn!(
                    status = %response.status(),
                    %url,
                    "PHP proxy metadata upstream returned error"
                );
                continue;
            }
            let bytes = response.bytes().await?;
            let (rewritten, metas) = rewrite_proxy_metadata(&bytes, base_url)?;

            // Always overwrite cached metadata atomically to ensure upstream URL hints
            // (pkgly-upstream-url) are present for dist downloads.
            let tmp_path = StoragePath::from(format!("{}.tmp", metadata_path));
            self.storage()
                .save_file(
                    self.id(),
                    FileContent::Bytes(rewritten.clone().into()),
                    &tmp_path,
                )
                .await?;
            let moved = self
                .storage()
                .move_file(self.id(), &tmp_path, metadata_path)
                .await?;
            if !moved {
                return Err(PhpRepositoryError::InvalidComposer(
                    "failed to finalize proxy metadata file".into(),
                ));
            }

            for meta in metas {
                record_proxy_cache_hit(self.indexer().as_ref(), Some(meta)).await?;
            }
            let builder = ResponseBuilder::ok().header(CONTENT_TYPE, "application/json");
            return Ok(Some(RepoResponse::Other(builder.body(rewritten))));
        }
        Ok(None)
    }

    async fn handle_dist_request(
        &self,
        request: RepositoryRequest,
        dist: ComposerDistPath,
    ) -> Result<RepoResponse, PhpRepositoryError> {
        if !can_read_repository_with_auth(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &request.auth_config,
        )
        .await?
        {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::UNAUTHORIZED,
                "Missing permission to read repository",
            ));
        }

        let storage_path = self.dist_storage_path(&dist);
        if let Some(file) = self.storage().open_file(self.id(), &storage_path).await? {
            return Ok(file.into());
        }

        if let Some(response) = self.download_and_cache_dist(&storage_path).await? {
            return Ok(response);
        }

        Ok(RepoResponse::basic_text_response(
            StatusCode::NOT_FOUND,
            "File not found",
        ))
    }

    async fn download_and_cache_dist(
        &self,
        cache_path: &StoragePath,
    ) -> Result<Option<RepoResponse>, PhpRepositoryError> {
        let Some(meta) = self.fetch_proxy_meta(cache_path).await? else {
            return Ok(None);
        };
        let Some(upstream_url) = meta.upstream_url.clone() else {
            return Ok(None);
        };
        let mut response = match self.fetch_upstream_dist(&upstream_url).await {
            Ok(resp) => resp,
            Err(err) => {
                if let Some(alt) = github_zipball_to_codeload(&upstream_url) {
                    match self.fetch_upstream_dist(&alt).await {
                        Ok(resp) => resp,
                        Err(err2) => {
                            warn!(?err2, %alt, "PHP proxy dist fetch failed (codeload fallback)");
                            return Ok(None);
                        }
                    }
                } else {
                    warn!(?err, %upstream_url, "PHP proxy dist fetch failed");
                    return Ok(None);
                }
            }
        };
        if !response.status().is_success() {
            if let Some(alt) = github_zipball_to_codeload(&upstream_url) {
                match self.fetch_upstream_dist(&alt).await {
                    Ok(alt_resp) => {
                        if !alt_resp.status().is_success() {
                            debug!(
                                status = %alt_resp.status(),
                                upstream = %alt_resp.url(),
                                "Upstream codeload returned non-success for PHP dist"
                            );
                            return Ok(None);
                        }
                        response = alt_resp;
                    }
                    Err(err) => {
                        warn!(
                            ?err,
                            %alt,
                            "PHP proxy dist fetch failed (codeload fallback after HTTP error)"
                        );
                        return Ok(None);
                    }
                }
            } else {
                debug!(
                    status = %response.status(),
                    upstream = %response.url(),
                    "Upstream returned non-success for PHP dist"
                );
                return Ok(None);
            }
        }
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await?;
        self.storage()
            .save_file(self.id(), FileContent::Bytes(bytes.clone()), cache_path)
            .await?;

        let recorded_meta = Self::build_recorded_meta(meta, cache_path, bytes.len() as u64);
        record_proxy_cache_hit(self.indexer().as_ref(), Some(recorded_meta)).await?;

        Ok(Some(Self::build_dist_response(status, &headers, bytes)))
    }

    async fn fetch_proxy_meta(
        &self,
        cache_path: &StoragePath,
    ) -> Result<Option<ProxyArtifactMeta>, PhpRepositoryError> {
        let mut meta = self.resolve_upstream_meta(cache_path).await?;
        if meta.is_none() {
            meta = self.resolve_upstream_meta_by_version(cache_path).await?;
        }
        if meta.is_none() {
            meta = proxy_meta_from_cache_path(cache_path, None);
        }
        if meta
            .as_ref()
            .and_then(|m| m.upstream_url.as_ref())
            .is_some()
        {
            return Ok(meta);
        }

        let dist = ComposerDistPath::try_from(cache_path)?;
        let metadata_path =
            self.metadata_path(&dist.vendor, &dist.package, is_dev_version(&dist.version));
        if let Some(recovered) = self
            .recover_meta_from_metadata(&metadata_path, cache_path)
            .await?
        {
            return Ok(Some(recovered));
        }

        // Metadata may be stale or missing upstream URL; refresh from upstream once,
        // then try recovery again.
        if self
            .fetch_and_cache_metadata(&metadata_path, None, &self.external_base_url(None))
            .await?
            .is_some()
        {
            if let Some(recovered) = self
                .recover_meta_from_metadata(&metadata_path, cache_path)
                .await?
            {
                return Ok(Some(recovered));
            }
        }

        Ok(meta)
    }

    async fn recover_meta_from_metadata(
        &self,
        metadata_path: &StoragePath,
        cache_path: &StoragePath,
    ) -> Result<Option<ProxyArtifactMeta>, PhpRepositoryError> {
        if let Some(StorageFile::File { content, meta }) =
            self.storage().open_file(self.id(), metadata_path).await?
        {
            let size_hint: usize = meta.file_type.file_size.try_into().unwrap_or(16_384);
            let bytes = content
                .read_to_vec(size_hint)
                .await
                .map_err(|err| PhpRepositoryError::InvalidComposer(err.to_string()))?;
            if let Ok(doc) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(recovered) = proxy_meta_from_metadata_doc(&doc, cache_path) {
                    return Ok(Some(recovered));
                }
            }
        }
        Ok(None)
    }

    async fn resolve_upstream_meta_by_version(
        &self,
        cache_path: &StoragePath,
    ) -> Result<Option<ProxyArtifactMeta>, PhpRepositoryError> {
        let dist = ComposerDistPath::try_from(cache_path)?;
        let package = format!("{}/{}", dist.vendor, dist.package).to_ascii_lowercase();
        let record = sqlx::query_scalar::<_, Option<serde_json::Value>>(
            r#"
                SELECT pv.extra
                FROM project_versions pv
                INNER JOIN projects p ON pv.project_id = p.id
                WHERE p.repository_id = $1
                  AND p.project_key = $2
                  AND pv.version = $3
                LIMIT 1
                "#,
        )
        .bind(self.id())
        .bind(&package)
        .bind(&dist.version)
        .fetch_one(&self.site().database)
        .await?;

        let Some(extra) = record else {
            return Ok(None);
        };
        let data: VersionData = serde_json::from_value(extra)?;
        Ok(data.proxy_artifact())
    }

    fn build_recorded_meta(
        meta: ProxyArtifactMeta,
        cache_path: &StoragePath,
        size: u64,
    ) -> ProxyArtifactMeta {
        let mut builder = ProxyArtifactMeta::builder(
            meta.package_name,
            meta.package_key,
            canonical_cache_path(cache_path),
        )
        .fetched_at(Utc::now());
        if let Some(version) = meta.version {
            builder = builder.version(version);
        }
        if let Some(digest) = meta.upstream_digest {
            builder = builder.upstream_digest(digest);
        }
        builder = builder.size(size);
        if let Some(url) = meta.upstream_url {
            builder = builder.upstream_url(url);
        }
        builder.build()
    }

    fn build_dist_response(
        status: StatusCode,
        headers: &http::HeaderMap,
        body: bytes::Bytes,
    ) -> RepoResponse {
        let mut builder = ResponseBuilder::default().status(status);
        if let Some(content_type) = headers.get(CONTENT_TYPE) {
            builder = builder.header(CONTENT_TYPE, content_type.clone());
        }
        if let Some(content_length) = headers.get(CONTENT_LENGTH) {
            builder = builder.header(CONTENT_LENGTH, content_length.clone());
        }
        RepoResponse::Other(builder.body(body))
    }

    async fn fetch_upstream_dist(&self, url: &str) -> Result<reqwest::Response, reqwest::Error> {
        crate::utils::upstream::send(
            self.client(),
            self.client()
                .get(url)
                .header(http::header::ACCEPT, "application/octet-stream"),
        )
        .await
    }

    async fn resolve_upstream_meta(
        &self,
        cache_path: &StoragePath,
    ) -> Result<Option<ProxyArtifactMeta>, PhpRepositoryError> {
        let normalized_path = canonical_cache_path(cache_path);
        let record = sqlx::query_scalar::<_, Option<serde_json::Value>>(
            r#"
                SELECT pv.extra
                FROM project_versions pv
                INNER JOIN projects p ON pv.project_id = p.id
                WHERE p.repository_id = $1
                  AND LOWER(pv.path) = $2
                LIMIT 1
                "#,
        )
        .bind(self.id())
        .bind(&normalized_path)
        .fetch_one(&self.site().database)
        .await?;

        let Some(extra) = record else {
            return Ok(None);
        };
        let data: VersionData = serde_json::from_value(extra)?;
        Ok(data.proxy_artifact())
    }

    async fn handle_head_metadata(
        &self,
        request: RepositoryRequest,
        vendor: String,
        package: String,
        is_dev: bool,
    ) -> Result<RepoResponse, PhpRepositoryError> {
        if !can_read_repository_with_auth(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &request.auth_config,
        )
        .await?
        {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::UNAUTHORIZED,
                "Missing permission to read repository",
            ));
        }
        let host = request
            .parts
            .headers
            .get(http::header::HOST)
            .and_then(|h| h.to_str().ok());
        let base_url = self.external_base_url(host);
        let metadata_path = self.metadata_path(&vendor, &package, is_dev);
        if let Some(StorageFile::File { meta, .. }) =
            self.storage().open_file(self.id(), &metadata_path).await?
        {
            let mut builder = ResponseBuilder::ok();
            builder = builder.header(CONTENT_LENGTH, meta.file_type.file_size.to_string());
            if let Some(mime) = meta.file_type.mime_type {
                builder = builder.header(CONTENT_TYPE, mime.to_string());
            }
            return Ok(RepoResponse::Other(builder.empty()));
        }
        if let Some(response) = self
            .fetch_and_cache_metadata(&metadata_path, request.parts.uri.query(), &base_url)
            .await?
        {
            return Ok(match response {
                RepoResponse::Other(resp) => {
                    let mut builder = ResponseBuilder::default().status(resp.status());
                    if let Some(len) = resp
                        .headers()
                        .get(CONTENT_LENGTH)
                        .and_then(|val| val.to_str().ok())
                    {
                        builder = builder.header(CONTENT_LENGTH, len);
                    }
                    RepoResponse::Other(builder.header(CONTENT_TYPE, "application/json").empty())
                }
                _ => response,
            });
        }
        Ok(RepoResponse::basic_text_response(
            StatusCode::NOT_FOUND,
            "Metadata not found",
        ))
    }

    async fn handle_head_dist(
        &self,
        request: RepositoryRequest,
        dist: ComposerDistPath,
    ) -> Result<RepoResponse, PhpRepositoryError> {
        if !can_read_repository_with_auth(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &request.auth_config,
        )
        .await?
        {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::UNAUTHORIZED,
                "Missing permission to read repository",
            ));
        }

        let storage_path = self.dist_storage_path(&dist);
        if let Some(StorageFile::File { meta, .. }) =
            self.storage().open_file(self.id(), &storage_path).await?
        {
            let mut builder = ResponseBuilder::ok();
            builder = builder.header(CONTENT_LENGTH, meta.file_type.file_size.to_string());
            if let Some(mime) = meta.file_type.mime_type {
                builder = builder.header(CONTENT_TYPE, mime.to_string());
            }
            return Ok(RepoResponse::Other(builder.empty()));
        }

        if let Some(meta) = self.resolve_upstream_meta(&storage_path).await? {
            if let Some(url) = meta.upstream_url {
                let response = match crate::utils::upstream::send(
                    self.client(),
                    self.client().head(url.clone()),
                )
                .await
                {
                    Ok(resp) => resp,
                    Err(err) => {
                        warn!(?err, %url, "PHP proxy HEAD upstream failed");
                        return Ok(RepoResponse::basic_text_response(
                            StatusCode::NOT_FOUND,
                            "File not found",
                        ));
                    }
                };
                let mut builder = ResponseBuilder::default().status(response.status());
                if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
                    builder = builder.header(CONTENT_TYPE, content_type.clone());
                }
                if let Some(content_length) = response.headers().get(CONTENT_LENGTH) {
                    builder = builder.header(CONTENT_LENGTH, content_length.clone());
                }
                return Ok(RepoResponse::Other(builder.empty()));
            }
        }

        Ok(RepoResponse::basic_text_response(
            StatusCode::NOT_FOUND,
            "File not found",
        ))
    }
}

impl RepositoryExt for PhpProxy {}

impl Repository for PhpProxy {
    type Error = PhpRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "php"
    }

    fn full_type(&self) -> &'static str {
        "php/proxy"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            PhpRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn name(&self) -> String {
        self.name().to_string()
    }

    fn id(&self) -> Uuid {
        self.id()
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

    fn handle_get(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            let path = parse_path(&request.path)?;
            match path {
                PhpProxyPath::RootIndex => this.handle_root_index().await,
                PhpProxyPath::Metadata {
                    vendor,
                    package,
                    is_dev,
                } => {
                    this.handle_metadata_request(request, vendor, package, is_dev)
                        .await
                }
                PhpProxyPath::Dist(dist) => this.handle_dist_request(request, dist).await,
                PhpProxyPath::Unknown => Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Not Found",
                )),
            }
        }
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            let path = parse_path(&request.path)?;
            match path {
                PhpProxyPath::RootIndex => Ok(RepoResponse::Other(ResponseBuilder::ok().empty())),
                PhpProxyPath::Metadata {
                    vendor,
                    package,
                    is_dev,
                } => {
                    this.handle_head_metadata(request, vendor, package, is_dev)
                        .await
                }
                PhpProxyPath::Dist(dist) => this.handle_head_dist(request, dist).await,
                PhpProxyPath::Unknown => Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Not Found",
                )),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PhpProxyPath {
    RootIndex,
    Metadata {
        vendor: String,
        package: String,
        is_dev: bool,
    },
    Dist(ComposerDistPath),
    Unknown,
}

fn parse_path(path: &StoragePath) -> Result<PhpProxyPath, PhpRepositoryError> {
    let components: Vec<String> = path.clone().into_iter().map(|c| c.to_string()).collect();
    if components.is_empty() {
        return Ok(PhpProxyPath::RootIndex);
    }
    if components.len() == 1 && components[0].eq_ignore_ascii_case("packages.json") {
        return Ok(PhpProxyPath::RootIndex);
    }

    if components.get(0).map(|s| s.as_str()) == Some("p2") && components.len() >= 3 {
        let vendor = components[1].clone();
        let file = components.last().cloned().unwrap_or_default();
        let is_dev = file.ends_with("~dev.json");
        let trimmed = file.trim_end_matches("~dev.json").trim_end_matches(".json");
        let package = trimmed.to_string();
        return Ok(PhpProxyPath::Metadata {
            vendor,
            package,
            is_dev,
        });
    }

    if components.get(0).map(|s| s.as_str()) == Some("dist") || components.len() >= 3 {
        let dist = ComposerDistPath::try_from(path)?;
        return Ok(PhpProxyPath::Dist(dist));
    }

    Ok(PhpProxyPath::Unknown)
}

fn build_url(base: &ProxyURL, path: StoragePath, query: Option<&str>) -> Option<url::Url> {
    match base.add_storage_path(path) {
        Ok(mut url) => {
            url.set_query(query);
            Some(url)
        }
        Err(err) => {
            warn!(?err, "Invalid proxy URL");
            None
        }
    }
}

fn extract_file_name(dist_url: &str) -> Option<String> {
    if let Ok(url) = Url::parse(dist_url) {
        return url
            .path_segments()
            .and_then(|segments| segments.last().map(str::to_string));
    }
    dist_url
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn cache_path_for_version(
    package: &str,
    version: &str,
    dist_url: &str,
) -> Result<String, PhpRepositoryError> {
    let (vendor, package_name) = package
        .split_once('/')
        .ok_or_else(|| PhpRepositoryError::InvalidComposer("package name missing vendor".into()))?;
    let file_name =
        extract_file_name(dist_url).unwrap_or_else(|| format!("{package_name}-{version}.zip"));
    Ok(format!(
        "dist/{}/{}/{}/{}",
        vendor.to_ascii_lowercase(),
        package_name.to_ascii_lowercase(),
        version,
        file_name
    ))
}

pub(super) fn rewrite_proxy_metadata(
    body: &[u8],
    base_path: &str,
) -> Result<(Vec<u8>, Vec<ProxyArtifactMeta>), PhpRepositoryError> {
    let mut value: Value = serde_json::from_slice(body)?;
    let Some(packages) = value.get_mut("packages").and_then(Value::as_object_mut) else {
        return Ok((body.to_vec(), Vec::new()));
    };

    let mut metas = Vec::new();
    for (package_name, versions) in packages.iter_mut() {
        let Some(entries) = versions.as_array_mut() else {
            continue;
        };
        for version in entries.iter_mut() {
            let Some(obj) = version.as_object_mut() else {
                continue;
            };
            let version_str = obj
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let Some(dist) = obj.get_mut("dist").and_then(Value::as_object_mut) else {
                continue;
            };
            let upstream_url = dist
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !upstream_url.is_empty() {
                dist.insert(
                    UPSTREAM_URL_FIELD.into(),
                    Value::String(upstream_url.clone()),
                );
            }
            let cache_path = cache_path_for_version(package_name, &version_str, &upstream_url)?;
            let dist_url = format!(
                "{}/{}",
                base_path.trim_end_matches('/'),
                cache_path.as_str()
            );
            dist.insert("url".into(), Value::String(dist_url));

            let mut builder = ProxyArtifactMeta::builder(
                package_name.to_ascii_lowercase(),
                package_name.to_ascii_lowercase(),
                canonical_cache_path_str(&cache_path),
            )
            .version(version_str.clone())
            .upstream_url(upstream_url.clone());

            if let Some(shasum) = dist.get("shasum").and_then(Value::as_str) {
                builder = builder.upstream_digest(shasum.to_string());
            }
            metas.push(builder.build());
        }
    }

    Ok((serde_json::to_vec(&value)?, metas))
}

fn canonical_cache_path(path: &StoragePath) -> String {
    path.to_string().trim_matches('/').to_lowercase()
}

fn canonical_cache_path_str(path: &str) -> String {
    path.trim_matches('/').to_lowercase()
}

fn is_dev_version(version: &str) -> bool {
    version.to_ascii_lowercase().contains("dev")
}

pub(super) fn proxy_meta_from_metadata_doc(
    doc: &Value,
    cache_path: &StoragePath,
) -> Option<ProxyArtifactMeta> {
    let dist = ComposerDistPath::try_from(cache_path).ok()?;
    let package = format!(
        "{}/{}",
        dist.vendor.to_ascii_lowercase(),
        dist.package.to_ascii_lowercase()
    );
    let packages = doc.get("packages")?.as_object()?;
    let versions = packages.get(&package)?.as_array()?;
    let version_entry = versions
        .iter()
        .find(|entry| {
            entry
                .get("version")
                .and_then(Value::as_str)
                .map(|v| v == dist.version)
                .unwrap_or(false)
        })
        .or_else(|| {
            // Fallback: match on filename when version strings differ (e.g., normalized vs prefixed)
            versions.iter().find(|entry| {
                entry
                    .get("dist")
                    .and_then(Value::as_object)
                    .and_then(|d| d.get("url"))
                    .and_then(Value::as_str)
                    .map(|url| url.ends_with(&dist.filename))
                    .unwrap_or(false)
            })
        })?;
    let version_obj = version_entry.as_object()?;
    let dist_obj = version_obj.get("dist")?.as_object()?;
    let upstream_url = dist_obj.get(UPSTREAM_URL_FIELD)?.as_str()?;

    let mut builder = ProxyArtifactMeta::builder(
        package.clone(),
        package.clone(),
        canonical_cache_path(cache_path),
    )
    .version(dist.version.clone())
    .upstream_url(upstream_url.to_string());

    if let Some(shasum) = dist_obj.get("shasum").and_then(Value::as_str) {
        builder = builder.upstream_digest(shasum.to_string());
    }

    Some(builder.build())
}

fn proxy_meta_from_cache_path(
    path: &StoragePath,
    upstream_url: Option<&Url>,
) -> Option<ProxyArtifactMeta> {
    let dist = ComposerDistPath::try_from(path).ok()?;
    let package = format!("{}/{}", dist.vendor, dist.package).to_ascii_lowercase();
    let mut builder = ProxyArtifactMeta::builder(&package, &package, canonical_cache_path(path))
        .version(dist.version);
    if let Some(url) = upstream_url {
        builder = builder.upstream_url(url.to_string());
    }
    Some(builder.build())
}

fn proxy_key_from_cache_path(path: &StoragePath) -> Option<ProxyArtifactKey> {
    let dist = ComposerDistPath::try_from(path).ok()?;
    Some(ProxyArtifactKey {
        package_key: format!("{}/{}", dist.vendor, dist.package).to_ascii_lowercase(),
        version: Some(dist.version),
        cache_path: Some(canonical_cache_path(path)),
    })
}

fn github_zipball_to_codeload(original: &str) -> Option<String> {
    let url = Url::parse(original).ok()?;
    if url.host_str()? != "api.github.com" {
        return None;
    }
    let mut segments = url.path_segments()?;
    if segments.next()? != "repos" {
        return None;
    }
    let org = segments.next()?;
    let repo = segments.next()?;
    if segments.next()? != "zipball" {
        return None;
    }
    let reference = segments.next().unwrap_or_default();
    if reference.is_empty() {
        return None;
    }
    Some(format!(
        "https://codeload.github.com/{org}/{repo}/legacy.zip/{reference}",
        org = org,
        repo = repo,
        reference = reference
    ))
}

#[cfg(test)]
mod tests;
