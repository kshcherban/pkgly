use chrono::Utc;
use http::{
    StatusCode,
    header::{CONTENT_LENGTH, CONTENT_TYPE},
};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nr_core::{
    database::entities::repository::DBRepository,
    repository::{
        Visibility,
        config::RepositoryConfigType,
        project::{ProxyArtifactKey, ProxyArtifactMeta},
        proxy_url::ProxyURL,
    },
    storage::StoragePath,
};
use nr_storage::{DynStorage, FileContent, Storage};
use parking_lot::RwLock;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;

use super::{
    configs::{GoProxyConfig, GoProxyRoute, GoRepositoryConfigType},
    utils::{GoModuleRequest, GoRequestType},
};

use crate::{
    app::Pkgly,
    repository::{
        RepoResponse, Repository, RepositoryFactoryError, RepositoryHandlerError,
        RepositoryRequest,
        proxy::base_proxy::{evict_proxy_cache_entry, record_proxy_cache_hit},
        proxy_indexing::{DatabaseProxyIndexer, ProxyIndexing, ProxyIndexingError},
        utils::can_read_repository_with_auth,
    },
    repository::{RepositoryAuthConfigType, go::GoRepositoryError},
};

// Default Go proxy route
static DEFAULT_GO_PROXY_ROUTE: LazyLock<GoProxyRoute> = LazyLock::new(|| GoProxyRoute {
    url: ProxyURL::try_from(String::from("https://proxy.golang.org"))
        .unwrap_or_else(|_| panic!("valid Go proxy default route")),
    name: Some("Go Official Proxy".to_string()),
    priority: Some(0),
});

const DEFAULT_SUMDB_BASE_URL: &str = "https://sum.golang.org";

fn normalize_routes(routes: Vec<GoProxyRoute>) -> Vec<GoProxyRoute> {
    if routes.is_empty() {
        vec![DEFAULT_GO_PROXY_ROUTE.clone()]
    } else {
        let mut sorted_routes = routes;
        sorted_routes.sort_by_key(|route| -route.priority()); // Higher priority first
        sorted_routes
    }
}

pub struct GoProxyInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub routes: RwLock<Vec<GoProxyRoute>>,
    pub client: reqwest::Client,
    pub active: bool,
    pub storage_name: String,
    pub cache_ttl: u64,
    pub indexer: Arc<dyn ProxyIndexing>,
}

#[derive(Debug, Clone)]
pub struct GoProxy(pub Arc<GoProxyInner>);

impl std::fmt::Debug for GoProxyInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoProxyInner")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("visibility", &self.visibility.read())
            .field("active", &self.active)
            .field("cache_ttl", &self.cache_ttl)
            .finish()
    }
}

impl GoProxy {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: GoProxyConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let client = reqwest::Client::builder()
            .user_agent("Pkgly Go Proxy/1.0")
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    GoRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;

        let storage_name = storage.storage_config().storage_config.storage_name.clone();
        let cache_ttl = config.go_module_cache_ttl.unwrap_or(3600); // Default 1 hour

        let indexer: Arc<dyn ProxyIndexing> =
            Arc::new(DatabaseProxyIndexer::new(site.clone(), repository.id));
        Ok(Self(Arc::new(GoProxyInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            storage,
            site: site.clone(),
            routes: RwLock::new(normalize_routes(config.routes)),
            client,
            active: repository.active,
            storage_name,
            cache_ttl,
            indexer,
        })))
    }

    fn id(&self) -> Uuid {
        self.0.id
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

    fn is_active(&self) -> bool {
        self.0.active
    }

    fn routes(&self) -> Vec<GoProxyRoute> {
        self.0.routes.read().clone()
    }

    fn indexer(&self) -> &Arc<dyn ProxyIndexing> {
        &self.0.indexer
    }

    pub async fn handle_external_eviction(
        &self,
        path: &StoragePath,
    ) -> Result<(), GoRepositoryError> {
        evict_go_proxy_cache_entry(self.indexer().as_ref(), path).await?;
        Ok(())
    }

    /// Build proxy URL for the request
    fn build_proxy_url(
        &self,
        base_url: &str,
        request: &GoModuleRequest,
    ) -> Result<String, crate::repository::RepositoryHandlerError> {
        let module_path = request.module_path.as_str();

        let url_path = match &request.request_type {
            GoRequestType::ListVersions => {
                format!("{}/@v/list", module_path)
            }
            GoRequestType::VersionInfo => {
                let version = request.version.as_ref().ok_or_else(|| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(
                            "Version required for version info request".to_string(),
                        ),
                    ))
                })?;
                format!("{}/@v/{}.info", module_path, version.as_str())
            }
            GoRequestType::GoMod => {
                let version = request.version.as_ref().ok_or_else(|| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(
                            "Version required for go.mod request".to_string(),
                        ),
                    ))
                })?;
                format!("{}/@v/{}.mod", module_path, version.as_str())
            }
            GoRequestType::ModuleZip => {
                let version = request.version.as_ref().ok_or_else(|| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(
                            "Version required for module zip request".to_string(),
                        ),
                    ))
                })?;
                format!("{}/@v/{}.zip", module_path, version.as_str())
            }
            GoRequestType::Latest => {
                format!("{}/@latest", module_path)
            }
            GoRequestType::GoModWithoutVersion => {
                // Deprecated endpoint - try to get latest go.mod
                format!("{}/@latest/go.mod", module_path)
            }
            GoRequestType::SumdbSupported
            | GoRequestType::SumdbLookup
            | GoRequestType::SumdbTile => {
                // Sumdb requests don't use the normal proxy URL pattern
                return Err(crate::repository::RepositoryHandlerError::Other(Box::new(
                    crate::utils::bad_request::BadRequestErrors::Other(
                        "Sumdb requests should be handled separately".to_string(),
                    ),
                )));
            }
        };

        // Ensure base_url ends with a slash
        let base_url = if base_url.ends_with('/') {
            base_url
        } else {
            &format!("{}/", base_url)
        };

        let full_url = format!("{}{}", base_url, url_path);

        // Validate URL
        Url::parse(&full_url).map_err(|e| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Invalid proxy URL: {}",
                    e
                )),
            ))
        })?;

        Ok(full_url)
    }

    /// Check if cached content is still valid based on TTL
    fn is_cache_valid(&self, cache_time: u64) -> bool {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let cache_age = current_time.saturating_sub(cache_time);
        cache_age < self.0.cache_ttl
    }

    /// Get cached content if available and not expired
    async fn get_cached_content(
        &self,
        cache_key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, crate::repository::RepositoryHandlerError> {
        let cache_path = StoragePath::from(format!("go-proxy-cache/{}", cache_key));

        match self.get_storage().open_file(self.id(), &cache_path).await {
            Ok(Some(nr_storage::StorageFile::File { meta, content })) => {
                let size_hint = usize::try_from(meta.file_type.file_size).unwrap_or(0);
                match content.read_to_vec(size_hint).await {
                    Ok(bytes) => {
                        // Try to parse metadata (first 8 bytes contain timestamp)
                        if bytes.len() >= 8 {
                            let timestamp_bytes = &bytes[0..8];
                            let cache_time = u64::from_be_bytes([
                                timestamp_bytes[0],
                                timestamp_bytes[1],
                                timestamp_bytes[2],
                                timestamp_bytes[3],
                                timestamp_bytes[4],
                                timestamp_bytes[5],
                                timestamp_bytes[6],
                                timestamp_bytes[7],
                            ]);

                            if self.is_cache_valid(cache_time) {
                                debug!(cache_key = %cache_key, cache_age_seconds = %SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs().saturating_sub(cache_time), "Found valid cache entry");
                                return Ok(Some((bytes[8..].to_vec(), cache_time)));
                            } else {
                                debug!(cache_key = %cache_key, "Cache entry expired");
                                // Clean up expired cache entry
                                let _ =
                                    self.get_storage().delete_file(self.id(), &cache_path).await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(cache_key = %cache_key, error = %e, "Failed to read cache file content");
                    }
                }
            }
            Ok(Some(_)) | Ok(None) => {
                debug!(cache_key = %cache_key, "No cache entry found or not a file");
            }
            Err(e) => {
                warn!(cache_key = %cache_key, error = %e, "Error opening cache file");
            }
        }

        Ok(None)
    }

    /// Cache content with timestamp
    async fn cache_content(
        &self,
        cache_key: &str,
        content: &[u8],
    ) -> Result<(), crate::repository::RepositoryHandlerError> {
        let cache_path = StoragePath::from(format!("go-proxy-cache/{}", cache_key));

        // Add timestamp metadata to the beginning of the file
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let timestamp_bytes = current_time.to_be_bytes();

        let mut cached_content = Vec::with_capacity(8 + content.len());
        cached_content.extend_from_slice(&timestamp_bytes);
        cached_content.extend_from_slice(content);

        self.get_storage()
            .save_file(self.id(), FileContent::Content(cached_content), &cache_path)
            .await
            .map_err(|e| crate::repository::RepositoryHandlerError::Other(Box::new(e)))?;

        record_go_proxy_cache_hit(self.indexer().as_ref(), &cache_path, content.len() as u64)
            .await
            .map_err(|err| {
                crate::repository::RepositoryHandlerError::Other(Box::new(GoRepositoryError::from(
                    err,
                )))
            })?;

        debug!(cache_key = %cache_key, "Cached content");
        Ok(())
    }

    async fn remove_cache_entry(&self, cache_key: &str) {
        let cache_path = StoragePath::from(format!("go-proxy-cache/{}", cache_key));
        match self.get_storage().delete_file(self.id(), &cache_path).await {
            Ok(true) => debug!(cache_key = %cache_key, "Removed cache entry"),
            Ok(false) => {}
            Err(err) => warn!(cache_key = %cache_key, error = %err, "Failed to remove cache entry"),
        }
    }

    async fn proxy_sumdb_request(
        &self,
        request: &GoModuleRequest,
    ) -> Result<RepoResponse, crate::repository::RepositoryHandlerError> {
        let sumdb_path = request.sumdb_path.as_deref().ok_or_else(|| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(
                    "Missing sumdb path for request".to_string(),
                ),
            ))
        })?;

        let mut url = DEFAULT_SUMDB_BASE_URL.trim_end_matches('/').to_string();
        url.push('/');
        url.push_str(sumdb_path.trim_start_matches('/'));

        debug!("Proxying sumdb request to: {}", url);

        let response = crate::utils::upstream::send(
            &self.0.client,
            self.0.client.get(&url).header("Accept", "text/plain, */*"),
        )
        .await
        .map_err(|e| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Failed to reach sumdb upstream: {}",
                    e
                )),
            ))
        })?;

        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.map_err(|e| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Failed to read sumdb response body: {}",
                    e
                )),
            ))
        })?;
        let body_vec = body.to_vec();

        let mut builder = crate::utils::ResponseBuilder::default().status(status);
        if let Some(content_type) = headers.get(CONTENT_TYPE) {
            builder = builder.header(CONTENT_TYPE, content_type.clone());
        } else {
            builder = builder.header(CONTENT_TYPE, "text/plain; charset=utf-8");
        }
        builder = builder.header(CONTENT_LENGTH, body_vec.len().to_string());

        Ok(RepoResponse::Other(builder.body(body_vec)))
    }

    /// Generate cache key for request
    fn generate_cache_key(
        &self,
        request: &GoModuleRequest,
    ) -> Result<String, RepositoryHandlerError> {
        request.cache_key()
    }

    /// Legacy cache key format used before hierarchical cache layout (for migration).
    fn legacy_cache_key(&self, request: &GoModuleRequest) -> Option<String> {
        match &request.request_type {
            GoRequestType::ListVersions => Some(format!("list:{}", request.module_path.as_str())),
            GoRequestType::VersionInfo => Some(format!(
                "info:{}:{}",
                request.module_path.as_str(),
                request.version.as_ref()?.as_str()
            )),
            GoRequestType::GoMod => Some(format!(
                "mod:{}:{}",
                request.module_path.as_str(),
                request.version.as_ref()?.as_str()
            )),
            GoRequestType::ModuleZip => Some(format!(
                "zip:{}:{}",
                request.module_path.as_str(),
                request.version.as_ref()?.as_str()
            )),
            GoRequestType::Latest => Some(format!("latest:{}", request.module_path.as_str())),
            GoRequestType::GoModWithoutVersion => {
                Some(format!("mod-no-version:{}", request.module_path.as_str()))
            }
            GoRequestType::SumdbSupported
            | GoRequestType::SumdbLookup
            | GoRequestType::SumdbTile => None,
        }
    }

    async fn fetch_cached_content(
        &self,
        request: &GoModuleRequest,
        cache_key: &str,
    ) -> Result<Option<Vec<u8>>, crate::repository::RepositoryHandlerError> {
        if let Some((cached, _)) = self.get_cached_content(cache_key).await? {
            return Ok(Some(cached));
        }

        if let Some(legacy_key) = self.legacy_cache_key(request) {
            if let Some((cached, _)) = self.get_cached_content(&legacy_key).await? {
                debug!(
                    cache_key = %cache_key,
                    legacy_key = %legacy_key,
                    "Migrating legacy cache entry to structured layout"
                );
                if let Err(err) = self.cache_content(cache_key, &cached).await {
                    warn!(
                        cache_key = %cache_key,
                        error = %err,
                        "Failed to migrate legacy cache entry"
                    );
                } else {
                    self.remove_cache_entry(&legacy_key).await;
                }
                return Ok(Some(cached));
            }
        }

        Ok(None)
    }

    /// Proxy from upstream servers with caching
    async fn proxy_from_upstream(
        &self,
        request: &GoModuleRequest,
    ) -> Result<Option<Vec<u8>>, crate::repository::RepositoryHandlerError> {
        let cache_key = self.generate_cache_key(request)?;

        // Try cache (with legacy migration)
        if let Some(cached_content) = self.fetch_cached_content(request, &cache_key).await? {
            debug!(cache_key = %cache_key, "Returning cached content");
            return Ok(Some(cached_content));
        }

        let routes = self.routes();

        for route in routes {
            match self.proxy_from_route(&route, request).await {
                Ok(Some(content)) => {
                    debug!(
                        "Successfully proxied from route: {}",
                        route.name.as_deref().unwrap_or("unnamed")
                    );

                    // Cache the successful response
                    if let Err(e) = self.cache_content(&cache_key, &content).await {
                        warn!(cache_key = %cache_key, error = %e, "Failed to cache content");
                    } else if let Some(legacy_key) = self.legacy_cache_key(request) {
                        self.remove_cache_entry(&legacy_key).await;
                    }

                    return Ok(Some(content));
                }
                Ok(None) => {
                    debug!(
                        "Route {} returned no content for {}",
                        route.name.as_deref().unwrap_or("unnamed"),
                        request.module_path.as_str()
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        "Failed to proxy from route {}: {}",
                        route.name.as_deref().unwrap_or("unnamed"),
                        e
                    );
                    continue;
                }
            }
        }

        warn!(
            "All routes failed for Go module: {}",
            request.module_path.as_str()
        );
        Ok(None)
    }

    /// Proxy from a specific route, returning either text or binary data
    async fn proxy_from_route(
        &self,
        route: &GoProxyRoute,
        request: &GoModuleRequest,
    ) -> Result<Option<Vec<u8>>, crate::repository::RepositoryHandlerError> {
        let base_url = route.url.as_str();
        let proxy_url = self.build_proxy_url(base_url, request)?;

        debug!("Proxying Go module request to: {}", proxy_url);

        let response = crate::utils::upstream::send(
            &self.0.client,
            self.0.client.get(&proxy_url).header(
                "Accept",
                "application/json, text/plain, application/zip, */*",
            ),
        )
        .await
        .map_err(|e| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Failed to fetch from proxy: {}",
                    e
                )),
            ))
        })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Proxy returned status: {}",
                    response.status()
                )),
            )));
        }

        let content = response.bytes().await.map_err(|e| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Failed to read response body: {}",
                    e
                )),
            ))
        })?;

        Ok(Some(content.to_vec()))
    }

    /// Proxy HEAD request from a specific route
    async fn proxy_head_from_route(
        &self,
        route: &GoProxyRoute,
        request: &GoModuleRequest,
    ) -> Result<Option<RepoResponse>, crate::repository::RepositoryHandlerError> {
        let base_url = route.url.as_str();
        let proxy_url = self.build_proxy_url(base_url, request)?;

        debug!("Proxying Go module HEAD request to: {}", proxy_url);

        let response = crate::utils::upstream::send(
            &self.0.client,
            self.0.client.head(&proxy_url).header(
                "Accept",
                "application/json, text/plain, application/zip, */*",
            ),
        )
        .await
        .map_err(|e| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Failed to fetch HEAD from proxy: {}",
                    e
                )),
            ))
        })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Proxy HEAD returned status: {}",
                    response.status()
                )),
            )));
        }

        // Build response with headers from upstream
        let mut builder = http::Response::builder().status(response.status());

        // Copy relevant headers
        use http::header;
        if let Some(content_type) = response.headers().get(header::CONTENT_TYPE) {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        if let Some(content_length) = response.headers().get(header::CONTENT_LENGTH) {
            builder = builder.header(header::CONTENT_LENGTH, content_length);
        }
        if let Some(etag) = response.headers().get(header::ETAG) {
            builder = builder.header(header::ETAG, etag);
        }
        if let Some(last_modified) = response.headers().get(header::LAST_MODIFIED) {
            builder = builder.header(header::LAST_MODIFIED, last_modified);
        }

        let response = builder.body(axum::body::Body::empty()).unwrap_or_default();

        Ok(Some(response.into()))
    }
}

impl Repository for GoProxy {
    type Error = crate::repository::RepositoryHandlerError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "go"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            GoRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn name(&self) -> String {
        self.0.name.clone()
    }

    fn id(&self) -> uuid::Uuid {
        self.id()
    }

    fn visibility(&self) -> nr_core::repository::Visibility {
        self.visibility()
    }

    fn is_active(&self) -> bool {
        self.is_active()
    }

    fn site(&self) -> Pkgly {
        self.site()
    }

    fn handle_head<'a>(
        &'a self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let visibility = self.visibility();
        let site = self.site();
        let repository_id = self.id();
        let this = self.clone();
        async move {
            if !can_read_repository_with_auth(
                &request.authentication,
                visibility,
                repository_id,
                site.as_ref(),
                &request.auth_config,
            )
            .await?
            {
                return Ok(RepoResponse::basic_text_response(
                    http::StatusCode::UNAUTHORIZED,
                    "Missing permission to read repository",
                ));
            }

            // Parse the Go module request
            let module_request =
                GoModuleRequest::from_path(&request.path.to_string()).map_err(|e| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(format!(
                            "Invalid Go module path: {}",
                            e
                        )),
                    ))
                })?;

            debug!(
                "Handling Go proxy HEAD request: {:?} for module: {}",
                module_request.request_type,
                module_request.module_path.as_str()
            );

            // For HEAD requests, we want to check if the content exists without downloading it
            match module_request.request_type {
                GoRequestType::SumdbSupported => {
                    let response = http::Response::builder()
                        .status(http::StatusCode::OK)
                        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                        .header(CONTENT_LENGTH, "4") // "true"
                        .body(axum::body::Body::empty())
                        .unwrap_or_default();
                    Ok(response.into())
                }
                GoRequestType::SumdbLookup | GoRequestType::SumdbTile => {
                    use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
                    let response = http::Response::builder()
                        .status(http::StatusCode::NOT_IMPLEMENTED)
                        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                        .header(CONTENT_LENGTH, "30") // "sumdb not implemented"
                        .body(axum::body::Body::empty())
                        .unwrap_or_default();
                    Ok(response.into())
                }
                _ => {
                    // For other requests, try to find cached content or proxy a HEAD request upstream
                    let cache_key = this.generate_cache_key(&module_request)?;

                    // Check cache first
                    if let Some(cached_content) = this
                        .fetch_cached_content(&module_request, &cache_key)
                        .await?
                    {
                        // Return appropriate headers based on request type
                        use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
                        let (content_type, status) = match module_request.request_type {
                            GoRequestType::VersionInfo => {
                                ("application/json", http::StatusCode::OK)
                            }
                            GoRequestType::GoMod | GoRequestType::GoModWithoutVersion => {
                                ("text/plain; charset=utf-8", http::StatusCode::OK)
                            }
                            GoRequestType::ListVersions => {
                                ("text/plain; charset=utf-8", http::StatusCode::OK)
                            }
                            GoRequestType::Latest => ("application/json", http::StatusCode::OK),
                            _ => ("application/octet-stream", http::StatusCode::OK),
                        };

                        let content_length = cached_content.len().to_string();
                        let response = http::Response::builder()
                            .status(status)
                            .header(CONTENT_TYPE, content_type)
                            .header(CONTENT_LENGTH, content_length)
                            .body(axum::body::Body::empty())
                            .unwrap_or_default();
                        return Ok(response.into());
                    }

                    // If not in cache, try to proxy HEAD request upstream
                    let routes = this.routes();
                    for route in routes {
                        match this.proxy_head_from_route(&route, &module_request).await {
                            Ok(Some(headers)) => {
                                debug!(
                                    "Successfully proxied HEAD from route: {}",
                                    route.name.as_deref().unwrap_or("unnamed")
                                );
                                return Ok(headers);
                            }
                            Ok(None) => {
                                debug!(
                                    "Route {} returned no content for HEAD request",
                                    route.name.as_deref().unwrap_or("unnamed")
                                );
                                continue;
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to proxy HEAD from route {}: {}",
                                    route.name.as_deref().unwrap_or("unnamed"),
                                    e
                                );
                                continue;
                            }
                        }
                    }

                    // If all routes failed, return NOT_FOUND
                    Ok(RepoResponse::basic_text_response(
                        http::StatusCode::NOT_FOUND,
                        "Module not found",
                    ))
                }
            }
        }
    }

    fn handle_get<'a>(
        &'a self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let visibility = self.visibility();
        let site = self.site();
        let repository_id = self.id();
        let this = self.clone();
        async move {
            if !can_read_repository_with_auth(
                &request.authentication,
                visibility,
                repository_id,
                site.as_ref(),
                &request.auth_config,
            )
            .await?
            {
                return Ok(RepoResponse::basic_text_response(
                    http::StatusCode::UNAUTHORIZED,
                    "Missing permission to read repository",
                ));
            }

            // Parse the Go module request
            let module_request =
                GoModuleRequest::from_path(&request.path.to_string()).map_err(|e| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(format!(
                            "Invalid Go module path: {}",
                            e
                        )),
                    ))
                })?;

            debug!(
                "Handling Go proxy request: {:?} for module: {}",
                module_request.request_type,
                module_request.module_path.as_str()
            );

            // Handle sumdb requests separately
            match module_request.request_type {
                GoRequestType::SumdbSupported => {
                    return Ok(RepoResponse::basic_text_response(
                        http::StatusCode::OK,
                        "true",
                    ));
                }
                GoRequestType::SumdbLookup | GoRequestType::SumdbTile => {
                    return this.proxy_sumdb_request(&module_request).await;
                }
                _ => {
                    // Continue with normal processing
                }
            }

            // Try to proxy from upstream
            match this.proxy_from_upstream(&module_request).await {
                Ok(Some(content)) => {
                    // Handle different content types
                    match module_request.request_type {
                        GoRequestType::ModuleZip => {
                            // Binary data for zip files - create binary response
                            use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
                            let content_length = content.len().to_string();
                            let response = http::Response::builder()
                                .status(http::StatusCode::OK)
                                .header(CONTENT_TYPE, "application/zip")
                                .header(CONTENT_LENGTH, content_length)
                                .body(axum::body::Body::from(content))
                                .unwrap_or_default();
                            Ok(response.into())
                        }
                        GoRequestType::VersionInfo => {
                            // Convert binary to string for JSON
                            let content_str = String::from_utf8(content).map_err(|e| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Invalid UTF-8 in JSON response: {}",
                                        e
                                    )),
                                ))
                            })?;
                            Ok(RepoResponse::basic_text_response(
                                http::StatusCode::OK,
                                content_str,
                            ))
                        }
                        _ => {
                            // Text data for other types
                            let content_str = String::from_utf8(content).map_err(|e| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Invalid UTF-8 in response: {}",
                                        e
                                    )),
                                ))
                            })?;
                            Ok(RepoResponse::basic_text_response(
                                http::StatusCode::OK,
                                content_str,
                            ))
                        }
                    }
                }
                Ok(None) => Ok(RepoResponse::basic_text_response(
                    http::StatusCode::NOT_FOUND,
                    "Module not found",
                )),
                Err(e) => Err(e),
            }
        }
    }

    #[allow(refining_impl_trait)]
    fn resolve_project_and_version_for_path(
        &self,
        _storage_path: &nr_core::storage::StoragePath,
    ) -> impl std::future::Future<
        Output = Result<nr_core::repository::project::ProjectResolution, Self::Error>,
    > + Send
    + '_ {
        // Go modules don't follow the same project/version pattern as other repositories
        async move {
            Ok(nr_core::repository::project::ProjectResolution {
                project_id: None,
                version_id: None,
            })
        }
    }
}

fn go_proxy_components(path: &StoragePath) -> Option<(String, String)> {
    let components: Vec<String> = path.clone().into_iter().map(String::from).collect();
    if components.len() < 4 || components.first().map(String::as_str) != Some("go-proxy-cache") {
        return None;
    }
    let version_idx = components.iter().position(|segment| segment == "@v")?;
    if version_idx <= 1 || version_idx + 1 >= components.len() {
        return None;
    }
    let module = components[1..version_idx].join("/");
    if module.is_empty() {
        return None;
    }
    let file_name = components.last()?.clone();
    Some((module, file_name))
}

const GO_PROXY_SUFFIXES: [&str; 3] = [".zip", ".mod", ".info"];

fn go_proxy_version_from_filename(file_name: &str) -> Option<String> {
    for suffix in GO_PROXY_SUFFIXES {
        if let Some(stripped) = file_name.strip_suffix(suffix) {
            if stripped.is_empty() {
                return None;
            }
            return Some(stripped.to_string());
        }
    }
    None
}

pub(super) fn go_proxy_meta_from_cache_path(
    path: &StoragePath,
    size: u64,
) -> Option<ProxyArtifactMeta> {
    let (module, file_name) = go_proxy_components(path)?;
    if !file_name.ends_with(".zip") {
        return None;
    }
    let version = go_proxy_version_from_filename(&file_name)?;
    Some(
        ProxyArtifactMeta::builder(module.clone(), module, path.to_string())
            .version(version)
            .size(size)
            .fetched_at(Utc::now())
            .build(),
    )
}

pub(super) fn go_proxy_key_from_cache_path(path: &StoragePath) -> Option<ProxyArtifactKey> {
    let (module, file_name) = go_proxy_components(path)?;
    let version = go_proxy_version_from_filename(&file_name)?;
    Some(ProxyArtifactKey {
        package_key: module,
        version: Some(version),
        cache_path: Some(path.to_string()),
    })
}

pub(super) async fn record_go_proxy_cache_hit(
    indexer: &dyn ProxyIndexing,
    path: &StoragePath,
    size: u64,
) -> Result<(), ProxyIndexingError> {
    let meta = go_proxy_meta_from_cache_path(path, size);
    record_proxy_cache_hit(indexer, meta).await
}

pub(super) async fn evict_go_proxy_cache_entry(
    indexer: &dyn ProxyIndexing,
    path: &StoragePath,
) -> Result<(), ProxyIndexingError> {
    let key = go_proxy_key_from_cache_path(path);
    evict_proxy_cache_entry(indexer, key).await
}

#[cfg(test)]
mod tests;
