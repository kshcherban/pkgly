use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use nr_core::{
    database::entities::repository::DBRepository,
    repository::{
        Visibility, config::RepositoryConfigType, project::ProxyArtifactMeta, proxy_url::ProxyURL,
    },
    storage::StoragePath,
};
use nr_storage::{DynStorage, FileContent, Storage};
use parking_lot::RwLock;
use reqwest::header::RANGE;
use tempfile::TempPath;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, instrument, warn};
use url::Url;
use uuid::Uuid;

use super::{
    REPOSITORY_TYPE_ID, RubyProxyConfig, RubyRepositoryConfig, RubyRepositoryConfigType,
    RubyRepositoryError,
};
use crate::{
    app::Pkgly,
    error::OtherInternalError,
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
        proxy::base_proxy::record_proxy_cache_hit,
        proxy_indexing::{DatabaseProxyIndexer, ProxyIndexing, ProxyIndexingError},
        utils::can_read_repository_with_auth,
    },
    utils::ResponseBuilder,
};

pub struct RubyProxyInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub config: RwLock<RubyProxyConfig>,
    pub active: bool,
    pub client: reqwest::Client,
    pub indexer: Arc<dyn ProxyIndexing>,
}

impl std::fmt::Debug for RubyProxyInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RubyProxyInner")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("visibility", &self.visibility.read())
            .field("active", &self.active)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct RubyProxy(pub Arc<RubyProxyInner>);

impl RubyProxy {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: RubyProxyConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let client = reqwest::Client::builder()
            .user_agent("Pkgly Ruby Proxy")
            .build()
            .map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    RubyRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;
        let indexer: Arc<dyn ProxyIndexing> =
            Arc::new(DatabaseProxyIndexer::new(site.clone(), repository.id));
        Ok(Self(Arc::new(RubyProxyInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            storage,
            site,
            config: RwLock::new(config),
            active: repository.active,
            client,
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

    fn upstream(&self) -> ProxyURL {
        self.0.config.read().upstream_url.clone()
    }

    fn indexer(&self) -> &Arc<dyn ProxyIndexing> {
        &self.0.indexer
    }
}

fn is_supported_rubygems_path(path: &StoragePath) -> bool {
    let path = path.to_string();
    if path == "names" || path == "versions" {
        return true;
    }
    if matches!(
        path.as_str(),
        "specs.4.8.gz" | "latest_specs.4.8.gz" | "prerelease_specs.4.8.gz"
    ) {
        return true;
    }
    if let Some(suffix) = path.strip_prefix("info/") {
        return !suffix.is_empty() && !suffix.contains('/');
    }
    if let Some(suffix) = path.strip_prefix("gems/") {
        return suffix.ends_with(".gem") && suffix.len() > ".gem".len();
    }
    if let Some(suffix) = path.strip_prefix("quick/Marshal.4.8/") {
        return suffix.ends_with(".gemspec.rz")
            && suffix.len() > ".gemspec.rz".len()
            && !suffix.contains('/');
    }
    false
}

fn build_upstream_url(upstream: &ProxyURL, path: &StoragePath, query: Option<&str>) -> Option<Url> {
    let mut url = upstream.add_storage_path(path.clone()).ok()?;
    url.set_query(query);
    Some(url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheThroughOutcome {
    Hit,
    Fetched { size: u64 },
    UpstreamStatus(StatusCode),
}

async fn download_to_tempfile(
    mut response: reqwest::Response,
) -> Result<(TempPath, u64), RubyRepositoryError> {
    let temp_path = tempfile::NamedTempFile::new()
        .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?
        .into_temp_path();
    let path_buf = temp_path.to_path_buf();
    let mut file = tokio::fs::File::create(&path_buf)
        .await
        .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;

    let mut written = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?
    {
        written = written.saturating_add(chunk.len() as u64);
        file.write_all(&chunk)
            .await
            .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;
    }
    file.flush()
        .await
        .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;
    Ok((temp_path, written))
}

#[instrument(
    name = "ruby_proxy_cache_miss_fetch",
    skip(client, storage, upstream, path),
    fields(
        nr.repository.id = %repository_id,
        nr.ruby.path = %path.to_string(),
        nr.ruby.upstream = %upstream,
        nr.ruby.cache.outcome = tracing::field::Empty,
        nr.ruby.cache.size_bytes = tracing::field::Empty,
        nr.ruby.cache.upstream_status = tracing::field::Empty,
        nr.ruby.cache.url = tracing::field::Empty
    )
)]
async fn fetch_and_cache_if_missing(
    client: &reqwest::Client,
    storage: &DynStorage,
    repository_id: Uuid,
    upstream: &ProxyURL,
    path: &StoragePath,
    query: Option<&str>,
) -> Result<CacheThroughOutcome, RubyRepositoryError> {
    if storage
        .get_file_information(repository_id, path)
        .await?
        .is_some()
    {
        tracing::Span::current().record("nr.ruby.cache.outcome", "hit");
        return Ok(CacheThroughOutcome::Hit);
    }

    let Some(url) = build_upstream_url(upstream, path, query) else {
        tracing::Span::current().record("nr.ruby.cache.outcome", "bad_upstream_url");
        return Ok(CacheThroughOutcome::UpstreamStatus(StatusCode::BAD_GATEWAY));
    };
    tracing::Span::current().record("nr.ruby.cache.url", &url.to_string());

    let response = crate::utils::upstream::send(client, client.get(url.clone()))
        .await
        .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;
    let status = response.status();
    tracing::Span::current().record("nr.ruby.cache.upstream_status", status.as_u16());
    if !status.is_success() {
        tracing::Span::current().record("nr.ruby.cache.outcome", "upstream_status");
        return Ok(CacheThroughOutcome::UpstreamStatus(status));
    }

    let (temp_path, size) = download_to_tempfile(response).await?;
    storage
        .save_file(
            repository_id,
            FileContent::Path(temp_path.to_path_buf()),
            path,
        )
        .await?;
    tracing::Span::current().record("nr.ruby.cache.outcome", "fetched");
    tracing::Span::current().record("nr.ruby.cache.size_bytes", size);
    debug!(%url, path = %path.to_string(), "Cached ruby proxy upstream response");
    Ok(CacheThroughOutcome::Fetched { size })
}

fn range_start_bytes(range: &str) -> Option<u64> {
    let range = range.strip_prefix("bytes=")?;
    let (start, _end) = range.split_once('-')?;
    start.trim().parse::<u64>().ok()
}

#[derive(Debug, Clone)]
struct RangeFetchOutcome {
    status: StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
}

async fn fetch_range_and_maybe_append(
    client: &reqwest::Client,
    storage: &DynStorage,
    repository_id: Uuid,
    upstream: &ProxyURL,
    path: &StoragePath,
    query: Option<&str>,
    range: &str,
) -> Result<RangeFetchOutcome, RubyRepositoryError> {
    tracing::Span::current().record("nr.ruby.range", range);
    let mut appended = false;
    let Some(url) = build_upstream_url(upstream, path, query) else {
        return Ok(RangeFetchOutcome {
            status: StatusCode::BAD_GATEWAY,
            headers: http::HeaderMap::new(),
            body: Bytes::new(),
        });
    };

    let response =
        crate::utils::upstream::send(client, client.get(url.clone()).header(RANGE, range))
            .await
            .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|err| RubyRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;

    if status == StatusCode::PARTIAL_CONTENT {
        if let Some(start) = range_start_bytes(range) {
            if let Some(meta) = storage.get_file_information(repository_id, path).await? {
                if let nr_storage::FileType::File(file) = meta.file_type() {
                    if file.file_size == start {
                        if let Err(err) = storage
                            .append_file(repository_id, FileContent::Bytes(body.clone()), path)
                            .await
                        {
                            warn!(
                                ?err,
                                path = %path.to_string(),
                                "Failed to append range payload to cached ruby proxy resource"
                            );
                        } else {
                            appended = true;
                        }
                    }
                }
            }
        }
    }

    if appended {
        debug!(
            url = %url,
            path = %path.to_string(),
            range = %range,
            "Appended ruby proxy range response to cache"
        );
    }

    Ok(RangeFetchOutcome {
        status,
        headers,
        body,
    })
}

fn build_passthrough_response(
    status: StatusCode,
    headers: &http::HeaderMap,
    body: Bytes,
) -> RepoResponse {
    use http::header::{
        ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
        LAST_MODIFIED,
    };

    let mut builder = ResponseBuilder::default().status(status);
    if let Some(value) = headers.get(ACCEPT_RANGES) {
        builder = builder.header(ACCEPT_RANGES, value.clone());
    }
    if let Some(value) = headers.get(CACHE_CONTROL) {
        builder = builder.header(CACHE_CONTROL, value.clone());
    }
    if let Some(value) = headers.get(CONTENT_LENGTH) {
        builder = builder.header(CONTENT_LENGTH, value.clone());
    }
    if let Some(value) = headers.get(CONTENT_RANGE) {
        builder = builder.header(CONTENT_RANGE, value.clone());
    }
    if let Some(value) = headers.get(CONTENT_TYPE) {
        builder = builder.header(CONTENT_TYPE, value.clone());
    }
    if let Some(value) = headers.get(ETAG) {
        builder = builder.header(ETAG, value.clone());
    }
    if let Some(value) = headers.get(LAST_MODIFIED) {
        builder = builder.header(LAST_MODIFIED, value.clone());
    }
    if let Some(value) = headers.get("Repr-Digest") {
        builder = builder.header("Repr-Digest", value.clone());
    }
    RepoResponse::Other(builder.body(body))
}

fn ruby_proxy_meta_from_cache_path(path: &StoragePath, size: u64) -> Option<ProxyArtifactMeta> {
    let path_str = path.to_string();
    let file_name = path_str.strip_prefix("gems/")?;
    let parsed = crate::repository::ruby::utils::parse_gem_file_name(file_name)?;
    let package_key = parsed.name.to_lowercase();
    let version = match parsed.platform.as_deref() {
        Some(platform) => format!("{}-{}", parsed.version, platform),
        None => parsed.version,
    };

    Some(
        ProxyArtifactMeta::builder(parsed.name, package_key, path_str)
            .version(version)
            .size(size)
            .build(),
    )
}

async fn record_ruby_proxy_cache_hit(
    indexer: &dyn ProxyIndexing,
    path: &StoragePath,
    size: u64,
) -> Result<(), ProxyIndexingError> {
    let meta = ruby_proxy_meta_from_cache_path(path, size);
    record_proxy_cache_hit(indexer, meta).await
}

impl Repository for RubyProxy {
    type Error = RubyRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn full_type(&self) -> &'static str {
        "ruby/proxy"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            RubyRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn name(&self) -> String {
        self.0.name.clone()
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

    async fn reload(&self) -> Result<(), RepositoryFactoryError> {
        let config = nr_core::database::entities::repository::DBRepositoryConfig::<
            RubyRepositoryConfig,
        >::get_config(
            self.id(),
            RubyRepositoryConfigType::get_type_static(),
            self.site().as_ref(),
        )
        .await?
        .map(|cfg| cfg.value.0)
        .ok_or(RepositoryFactoryError::MissingConfig(
            RubyRepositoryConfigType::get_type_static(),
        ))?;

        let RubyRepositoryConfig::Proxy(proxy_config) = config else {
            return Err(RepositoryFactoryError::InvalidSubType);
        };

        let mut guard = self.0.config.write();
        *guard = proxy_config;
        Ok(())
    }

    fn handle_get(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_get_internal(request).await }
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_head_internal(request).await }
    }
}

impl RubyProxy {
    #[instrument(
        name = "ruby_proxy_get",
        skip(self, request),
        fields(
            nr.repository.id = %self.id(),
            nr.repository.name = %self.0.name,
            nr.repository.type = "ruby/proxy",
            nr.http.method = "GET",
            nr.ruby.path = %request.path.to_string(),
            nr.ruby.cache.outcome = tracing::field::Empty,
            nr.ruby.cache.size_bytes = tracing::field::Empty,
            nr.ruby.cache.upstream_status = tracing::field::Empty,
            nr.ruby.range = tracing::field::Empty
        )
    )]
    async fn handle_get_internal(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, RubyRepositoryError> {
        if !can_read_repository_with_auth(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &request.auth_config,
        )
        .await?
        {
            tracing::Span::current().record("nr.ruby.cache.outcome", "unauthorized");
            return Ok(RepoResponse::unauthorized());
        }

        let query = request.parts.uri.query();
        let path = request.path;
        if path.is_directory() {
            tracing::Span::current().record("nr.ruby.cache.outcome", "directory");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Directory not found",
            ));
        }

        if !is_supported_rubygems_path(&path) {
            tracing::Span::current().record("nr.ruby.cache.outcome", "unsupported_path");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            ));
        }

        let range = request
            .parts
            .headers
            .get(RANGE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());

        if let Some(range) = range {
            tracing::Span::current().record("nr.ruby.range", &range);
            tracing::Span::current().record("nr.ruby.cache.outcome", "range_passthrough");

            let outcome = fetch_range_and_maybe_append(
                &self.0.client,
                &self.storage(),
                self.id(),
                &self.upstream(),
                &path,
                query,
                &range,
            )
            .await?;
            tracing::Span::current()
                .record("nr.ruby.cache.upstream_status", outcome.status.as_u16());

            return Ok(build_passthrough_response(
                outcome.status,
                &outcome.headers,
                outcome.body,
            ));
        }

        if let Some(file) = self.storage().open_file(self.id(), &path).await? {
            tracing::Span::current().record("nr.ruby.cache.outcome", "hit");
            return Ok(file.into());
        }

        let outcome = fetch_and_cache_if_missing(
            &self.0.client,
            &self.storage(),
            self.id(),
            &self.upstream(),
            &path,
            query,
        )
        .await?;

        match outcome {
            CacheThroughOutcome::Hit => {
                tracing::Span::current().record("nr.ruby.cache.outcome", "hit");
                if let Some(file) = self.storage().open_file(self.id(), &path).await? {
                    Ok(file.into())
                } else {
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "File not found",
                    ))
                }
            }
            CacheThroughOutcome::Fetched { size } => {
                tracing::Span::current().record("nr.ruby.cache.outcome", "fetched");
                tracing::Span::current().record("nr.ruby.cache.size_bytes", size);

                if path.to_string().starts_with("gems/") {
                    if let Err(err) =
                        record_ruby_proxy_cache_hit(self.indexer().as_ref(), &path, size).await
                    {
                        error!(
                            ?err,
                            path = %path.to_string(),
                            "Failed to record ruby proxy cache hit"
                        );
                        return Err(err.into());
                    }
                }

                if let Some(file) = self.storage().open_file(self.id(), &path).await? {
                    Ok(file.into())
                } else {
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "File not found",
                    ))
                }
            }
            CacheThroughOutcome::UpstreamStatus(status) => {
                tracing::Span::current().record("nr.ruby.cache.outcome", "upstream_status");
                tracing::Span::current().record("nr.ruby.cache.upstream_status", status.as_u16());
                if status == StatusCode::NOT_FOUND {
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "File not found",
                    ))
                } else {
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::BAD_GATEWAY,
                        format!("Upstream returned status {status}"),
                    ))
                }
            }
        }
    }

    #[instrument(
        name = "ruby_proxy_head",
        skip(self, request),
        fields(
            nr.repository.id = %self.id(),
            nr.repository.name = %self.0.name,
            nr.repository.type = "ruby/proxy",
            nr.http.method = "HEAD",
            nr.ruby.path = %request.path.to_string(),
            nr.ruby.cache.outcome = tracing::field::Empty,
            nr.ruby.cache.url = tracing::field::Empty,
            nr.ruby.cache.upstream_status = tracing::field::Empty
        )
    )]
    async fn handle_head_internal(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, RubyRepositoryError> {
        if !can_read_repository_with_auth(
            &request.authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &request.auth_config,
        )
        .await?
        {
            tracing::Span::current().record("nr.ruby.cache.outcome", "unauthorized");
            return Ok(RepoResponse::unauthorized());
        }

        let query = request.parts.uri.query();
        let path = request.path;
        if path.is_directory() {
            tracing::Span::current().record("nr.ruby.cache.outcome", "directory");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Directory not found",
            ));
        }

        if !is_supported_rubygems_path(&path) {
            tracing::Span::current().record("nr.ruby.cache.outcome", "unsupported_path");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            ));
        }

        if let Some(meta) = self
            .storage()
            .get_file_information(self.id(), &path)
            .await?
        {
            tracing::Span::current().record("nr.ruby.cache.outcome", "hit");
            return Ok(meta.into());
        }

        let Some(url) = build_upstream_url(&self.upstream(), &path, query) else {
            tracing::Span::current().record("nr.ruby.cache.outcome", "bad_upstream_url");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_GATEWAY,
                "Bad upstream URL",
            ));
        };
        tracing::Span::current().record("nr.ruby.cache.url", &url.to_string());

        let response =
            crate::utils::upstream::send(&self.0.client, self.0.client.head(url.clone()))
                .await
                .map_err(|err| {
                    RubyRepositoryError::Other(Box::new(OtherInternalError::new(err)))
                })?;
        let status = response.status();
        tracing::Span::current().record("nr.ruby.cache.upstream_status", status.as_u16());
        let headers = response.headers().clone();

        let body = Bytes::new();
        Ok(build_passthrough_response(status, &headers, body))
    }
}

#[cfg(test)]
mod tests;
