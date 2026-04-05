use std::sync::{Arc, LazyLock};

use chrono::Utc;
use http::StatusCode;
use nr_core::{
    database::entities::repository::{DBRepository, DBRepositoryConfig},
    repository::{
        Visibility,
        config::RepositoryConfigType,
        project::{ProxyArtifactKey, ProxyArtifactMeta},
        proxy_url::ProxyURL,
    },
    storage::StoragePath,
};
use nr_storage::{DynStorage, FileContent, Storage};
use parking_lot::{RwLock, RwLockReadGuard};
use regex::Regex;
use serde_json::Value;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;

use super::{
    PythonRepositoryError,
    configs::{PythonProxyConfig, PythonProxyRoute, PythonRepositoryConfigType},
    utils::normalize_package_name,
};

static DEFAULT_ROUTE: LazyLock<PythonProxyRoute> = LazyLock::new(|| PythonProxyRoute {
    url: ProxyURL::try_from(String::from("https://pypi.org/simple"))
        .unwrap_or_else(|_| panic!("valid PyPI default route")),
    name: Some("PyPI".to_string()),
});

fn normalize_routes(routes: Vec<PythonProxyRoute>) -> Vec<PythonProxyRoute> {
    if routes.is_empty() {
        vec![DEFAULT_ROUTE.clone()]
    } else {
        routes
    }
}
use crate::{
    app::Pkgly,
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
        proxy::base_proxy::{evict_proxy_cache_entry, record_proxy_cache_hit},
        proxy_indexing::{DatabaseProxyIndexer, ProxyIndexing, ProxyIndexingError},
        utils::can_read_repository_with_auth,
    },
    utils::ResponseBuilder,
};

pub struct PythonProxyInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub routes: RwLock<Vec<PythonProxyRoute>>,
    pub client: reqwest::Client,
    pub active: bool,
    pub storage_name: String,
    pub indexer: Arc<dyn ProxyIndexing>,
}

impl std::fmt::Debug for PythonProxyInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PythonProxyInner")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("visibility", &self.visibility.read())
            .field("storage_name", &self.storage_name)
            .field("active", &self.active)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct PythonProxy(pub Arc<PythonProxyInner>);

impl PythonProxy {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: PythonProxyConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let client = reqwest::Client::builder()
            .user_agent("Pkgly Python Proxy")
            .build()
            .map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    PythonRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;
        let storage_name = storage.storage_config().storage_config.storage_name.clone();
        let indexer: Arc<dyn ProxyIndexing> =
            Arc::new(DatabaseProxyIndexer::new(site.clone(), repository.id));
        Ok(Self(Arc::new(PythonProxyInner {
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
    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }
    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }
    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }
    fn routes(&self) -> RwLockReadGuard<'_, Vec<PythonProxyRoute>> {
        self.0.routes.read()
    }
    fn storage_name(&self) -> &str {
        &self.0.storage_name
    }
    fn repository_slug(&self) -> &str {
        &self.0.name
    }
    fn base_repository_path(&self) -> String {
        format!(
            "/repositories/{}/{}",
            self.storage_name(),
            self.repository_slug()
        )
    }
    fn indexer(&self) -> &Arc<dyn ProxyIndexing> {
        &self.0.indexer
    }

    pub async fn handle_external_eviction(
        &self,
        path: &StoragePath,
    ) -> Result<(), PythonRepositoryError> {
        evict_python_proxy_cache_entry(self.indexer().as_ref(), path).await?;
        Ok(())
    }

    async fn download_and_cache(
        &self,
        path: &StoragePath,
        query: Option<&str>,
    ) -> Result<bool, PythonRepositoryError> {
        if path.is_directory() {
            return Ok(false);
        }
        let routes = {
            let guard = self.routes();
            guard.clone()
        };
        for route in routes.iter() {
            let Some(url) = build_url(&route.url, path.clone(), query) else {
                continue;
            };
            match crate::utils::upstream::send(&self.0.client, self.0.client.get(url.clone())).await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        let bytes = response.bytes().await.map_err(|err| {
                            PythonRepositoryError::Other(Box::new(
                                crate::error::OtherInternalError::new(err),
                            ))
                        })?;
                        self.storage()
                            .save_file(self.id(), FileContent::Bytes(bytes.clone()), path)
                            .await?;

                        let canonical_path =
                            if let Some(cache_path) = cache_path_for_python_proxy(path) {
                                if cache_path != *path {
                                    if let Err(err) = self
                                        .storage()
                                        .save_file(
                                            self.id(),
                                            FileContent::Bytes(bytes.clone()),
                                            &cache_path,
                                        )
                                        .await
                                    {
                                        warn!(
                                            ?err,
                                            ?cache_path,
                                            "Failed to persist python proxy cache entry"
                                        );
                                    }
                                }
                                cache_path
                            } else {
                                path.clone()
                            };

                        record_python_proxy_cache_hit(
                            self.indexer().as_ref(),
                            &canonical_path,
                            bytes.len() as u64,
                            Some(&url),
                        )
                        .await?;
                        debug!(%url, "Cached python proxy resource");
                        return Ok(true);
                    }
                    if response.status().is_client_error() {
                        continue;
                    }
                    warn!(
                        status = ?response.status(),
                        %url,
                        "Upstream returned error while proxying python resource"
                    );
                }
                Err(err) => {
                    warn!(%url, error = %err, "Failed to reach python proxy upstream");
                }
            }
        }
        Ok(false)
    }

    async fn proxy_passthrough(
        &self,
        path: &StoragePath,
        base_path: &str,
        query: Option<&str>,
        include_body: bool,
    ) -> Result<Option<RepoResponse>, PythonRepositoryError> {
        use http::header::{CONTENT_LENGTH, CONTENT_TYPE};

        let routes = {
            let guard = self.routes();
            guard.clone()
        };
        for route in routes.iter() {
            let Some(url) = build_url(&route.url, path.clone(), query) else {
                continue;
            };
            if !include_body {
                match crate::utils::upstream::send(&self.0.client, self.0.client.head(url.clone()))
                    .await
                {
                    Ok(response) if response.status().is_success() => {
                        return Ok(Some(build_head_response(response)));
                    }
                    Ok(response) if response.status() == StatusCode::METHOD_NOT_ALLOWED => {
                        // fall through to GET below
                    }
                    Ok(response) => {
                        if response.status().is_client_error() {
                            continue;
                        }
                        warn!(
                            status = ?response.status(),
                            %url,
                            "Upstream head error for python proxy"
                        );
                        continue;
                    }
                    Err(err) => {
                        warn!(%url, error = %err, "Failed head request for python proxy");
                        continue;
                    }
                }
            }

            match crate::utils::upstream::send(&self.0.client, self.0.client.get(url.clone())).await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        if include_body {
                            let status = response.status();
                            let headers = response.headers().clone();
                            let body = response.bytes().await.map_err(|err| {
                                PythonRepositoryError::Other(Box::new(
                                    crate::error::OtherInternalError::new(err),
                                ))
                            })?;
                            let mut builder = ResponseBuilder::default().status(status);
                            if let Some(content_type) = headers.get(CONTENT_TYPE) {
                                builder = builder.header(CONTENT_TYPE, content_type.clone());
                            }
                            let mut body_vec = body.to_vec();
                            if let Some(content_type) = headers.get(CONTENT_TYPE) {
                                if let Ok(content_type) = content_type.to_str() {
                                    if content_type.starts_with("text/html") {
                                        if let Some(rewritten) =
                                            rewrite_simple_html(&body_vec, base_path, &url)
                                        {
                                            body_vec = rewritten;
                                        }
                                    } else if content_type.contains("application/vnd.pypi.simple")
                                        || content_type.contains("application/json")
                                    {
                                        if let Some(rewritten) =
                                            rewrite_simple_json(&body_vec, base_path, &url)
                                        {
                                            body_vec = rewritten;
                                        }
                                    }
                                }
                            }
                            builder = builder.header(CONTENT_LENGTH, body_vec.len().to_string());
                            return Ok(Some(RepoResponse::Other(builder.body(body_vec))));
                        } else {
                            return Ok(Some(build_head_response(response)));
                        }
                    }
                    if response.status().is_client_error() {
                        continue;
                    }
                    warn!(
                        status = ?response.status(),
                        %url,
                        "Upstream error for python proxy"
                    );
                }
                Err(err) => {
                    warn!(%url, error = %err, "Failed to reach python proxy upstream");
                }
            }
        }
        Ok(None)
    }
}

impl Repository for PythonProxy {
    type Error = PythonRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "python"
    }

    fn full_type(&self) -> &'static str {
        "python/proxy"
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
        let config = DBRepositoryConfig::<PythonProxyConfig>::get_config(
            self.id(),
            PythonRepositoryConfigType::get_type_static(),
            self.site().as_ref(),
        )
        .await?
        .map(|cfg| cfg.value.0)
        .unwrap_or_default();
        let mut routes = self.0.routes.write();
        *routes = normalize_routes(config.routes);
        Ok(())
    }

    fn handle_get(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            let query = request.parts.uri.query().map(|q| q.to_string());
            let can_read = if request.authentication.is_virtual_repository() {
                true
            } else {
                can_read_repository_with_auth(
                    &request.authentication,
                    this.visibility(),
                    this.id(),
                    this.site().as_ref(),
                    &request.auth_config,
                )
                .await?
            };
            if !can_read {
                return Ok(RepoResponse::unauthorized());
            }

            let uri_path = request.parts.uri.path().to_string();
            let path = request.path;
            let base_path = derive_request_base_path(&uri_path, &path)
                .unwrap_or_else(|| this.base_repository_path());

            if path.is_directory() {
                if let Some(response) = this
                    .proxy_passthrough(&path, &base_path, query.as_deref(), true)
                    .await?
                {
                    return Ok(response);
                }
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Directory not found",
                ));
            }

            let cache_path = cache_path_for_python_proxy(&path);

            if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                return Ok(file.into());
            }

            if let Some(cache_path) = &cache_path {
                if let Some(file) = this.storage().open_file(this.id(), cache_path).await? {
                    return Ok(file.into());
                }
            }

            if this.download_and_cache(&path, query.as_deref()).await? {
                if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                    return Ok(file.into());
                }
                if let Some(cache_path) = &cache_path {
                    if let Some(file) = this.storage().open_file(this.id(), cache_path).await? {
                        return Ok(file.into());
                    }
                }
            }

            if let Some(response) = this
                .proxy_passthrough(&path, &base_path, query.as_deref(), true)
                .await?
            {
                return Ok(response);
            }

            Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "File not found",
            ))
        }
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            let query = request.parts.uri.query().map(|q| q.to_string());
            let can_read = if request.authentication.is_virtual_repository() {
                true
            } else {
                can_read_repository_with_auth(
                    &request.authentication,
                    this.visibility(),
                    this.id(),
                    this.site().as_ref(),
                    &request.auth_config,
                )
                .await?
            };
            if !can_read {
                return Ok(RepoResponse::unauthorized());
            }

            let uri_path = request.parts.uri.path().to_string();
            let path = request.path;
            let base_path = derive_request_base_path(&uri_path, &path)
                .unwrap_or_else(|| this.base_repository_path());

            if path.is_directory() {
                if let Some(response) = this
                    .proxy_passthrough(&path, &base_path, query.as_deref(), false)
                    .await?
                {
                    return Ok(response);
                }
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Directory not found",
                ));
            }

            let cache_path = cache_path_for_python_proxy(&path);

            if let Some(meta) = this
                .storage()
                .get_file_information(this.id(), &path)
                .await?
            {
                return Ok(meta.into());
            }

            if let Some(cache_path) = &cache_path {
                if let Some(meta) = this
                    .storage()
                    .get_file_information(this.id(), cache_path)
                    .await?
                {
                    return Ok(meta.into());
                }
            }

            if this.download_and_cache(&path, query.as_deref()).await? {
                if let Some(meta) = this
                    .storage()
                    .get_file_information(this.id(), &path)
                    .await?
                {
                    return Ok(meta.into());
                }
                if let Some(cache_path) = &cache_path {
                    if let Some(meta) = this
                        .storage()
                        .get_file_information(this.id(), cache_path)
                        .await?
                    {
                        return Ok(meta.into());
                    }
                }
            }

            if let Some(response) = this
                .proxy_passthrough(&path, &base_path, query.as_deref(), false)
                .await?
            {
                return Ok(response);
            }

            Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "File not found",
            ))
        }
    }
}

const PYTHON_PROXY_SUFFIXES: [&str; 7] = [
    ".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".tar", ".zip", ".whl",
];

fn strip_python_suffix(file_name: &str) -> &str {
    for suffix in PYTHON_PROXY_SUFFIXES {
        if file_name.ends_with(suffix) {
            return &file_name[..file_name.len() - suffix.len()];
        }
    }
    file_name
}

fn python_proxy_name_version(file_name: &str) -> Option<(String, String)> {
    let stem = strip_python_suffix(file_name);
    let mut search = stem.match_indices('-').collect::<Vec<_>>();
    search.sort_by_key(|(idx, _)| *idx);
    for (idx, _) in search {
        if idx + 1 >= stem.len() {
            continue;
        }
        let remainder = &stem[idx + 1..];
        let trimmed = if remainder.starts_with(['v', 'V']) {
            &remainder[1..]
        } else {
            remainder
        };
        if trimmed.is_empty()
            || !trimmed
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            continue;
        }
        let version_end = trimmed.find('-').unwrap_or(trimmed.len());
        if version_end == 0 {
            continue;
        }
        let version = trimmed[..version_end].to_string();
        let name = stem[..idx].to_string();
        if name.is_empty() || version.is_empty() {
            continue;
        }
        return Some((name, version));
    }
    None
}

fn storage_file_name(path: &StoragePath) -> Option<String> {
    path.clone().into_iter().last().map(String::from)
}

fn canonical_cache_path(path: &StoragePath) -> String {
    path.to_string()
}

pub(super) fn python_proxy_meta_from_cache_path(
    path: &StoragePath,
    size: u64,
    upstream_url: Option<&Url>,
) -> Option<ProxyArtifactMeta> {
    let file_name = storage_file_name(path)?;
    let (package_name, version) = python_proxy_name_version(&file_name)?;
    let mut builder = ProxyArtifactMeta::builder(
        package_name.clone(),
        normalize_package_name(&package_name),
        canonical_cache_path(path),
    )
    .version(version)
    .size(size)
    .fetched_at(Utc::now());
    if let Some(url) = upstream_url {
        builder = builder.upstream_url(url.to_string());
    }
    Some(builder.build())
}

pub(super) fn python_proxy_key_from_cache_path(path: &StoragePath) -> Option<ProxyArtifactKey> {
    let file_name = storage_file_name(path)?;
    let (package_name, version) = python_proxy_name_version(&file_name)?;
    Some(ProxyArtifactKey {
        package_key: normalize_package_name(&package_name),
        version: Some(version),
        cache_path: Some(canonical_cache_path(path)),
    })
}

pub(super) async fn record_python_proxy_cache_hit(
    indexer: &dyn ProxyIndexing,
    path: &StoragePath,
    size: u64,
    upstream_url: Option<&Url>,
) -> Result<(), ProxyIndexingError> {
    let meta = python_proxy_meta_from_cache_path(path, size, upstream_url);
    record_proxy_cache_hit(indexer, meta).await
}

pub(super) async fn evict_python_proxy_cache_entry(
    indexer: &dyn ProxyIndexing,
    path: &StoragePath,
) -> Result<(), ProxyIndexingError> {
    let key = python_proxy_key_from_cache_path(path);
    evict_proxy_cache_entry(indexer, key).await
}

fn cache_path_for_python_proxy(path: &StoragePath) -> Option<StoragePath> {
    if path.is_directory() {
        return None;
    }
    let components: Vec<String> = path
        .clone()
        .into_iter()
        .map(|component| component.to_string())
        .collect();
    if components.is_empty() {
        return None;
    }
    if matches!(components.first().map(String::as_str), Some("packages")) {
        return Some(path.clone());
    }
    if components.len() >= 3 && matches!(components.first().map(String::as_str), Some("simple")) {
        let package = components.get(1)?.clone();
        let file_name = components.last()?.clone();
        let normalized = normalize_package_name(&package);
        return Some(StoragePath::from(format!(
            "packages/{}/{}",
            normalized, file_name
        )));
    }
    None
}

#[cfg(test)]
mod tests;

fn build_head_response(response: reqwest::Response) -> RepoResponse {
    use http::header::{CONTENT_LENGTH, CONTENT_TYPE};

    let status = response.status();
    let headers = response.headers().clone();
    let mut builder = ResponseBuilder::default().status(status);
    if let Some(content_type) = headers.get(CONTENT_TYPE) {
        builder = builder.header(CONTENT_TYPE, content_type.clone());
    }
    if let Some(content_length) = headers.get(CONTENT_LENGTH) {
        builder = builder.header(CONTENT_LENGTH, content_length.clone());
    }
    RepoResponse::Other(builder.empty())
}

fn rewrite_simple_html(body: &[u8], base_path: &str, upstream_base: &Url) -> Option<Vec<u8>> {
    let html = std::str::from_utf8(body).ok()?;
    let normalized_base = normalize_base_path(base_path);
    let rewritten = HREF_REGEX.replace_all(html, |caps: &regex::Captures<'_>| {
        let original = &caps[1];
        match resolve_upstream_link(original, upstream_base) {
            Some(resolved) => format!("href=\"{}\"", build_local_url(&resolved, &normalized_base)),
            None => caps[0].to_string(),
        }
    });
    let rewritten = replace_absolute_urls(&rewritten, &normalized_base, upstream_base);
    Some(rewritten.into_bytes())
}

fn rewrite_simple_json(body: &[u8], base_path: &str, upstream_base: &Url) -> Option<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    let normalized_base = normalize_base_path(base_path);
    rewrite_json_value(&mut value, &normalized_base, upstream_base);
    serde_json::to_vec(&value).ok()
}

fn rewrite_json_value(value: &mut Value, normalized_base: &str, upstream_base: &Url) {
    match value {
        Value::String(s) => {
            if let Some(resolved) = resolve_upstream_link(s, upstream_base) {
                *s = build_local_url(&resolved, normalized_base);
            } else if let Some(rewritten) =
                rewrite_known_host_url(s, normalized_base, upstream_base)
            {
                *s = rewritten;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_json_value(item, normalized_base, upstream_base);
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                rewrite_json_value(value, normalized_base, upstream_base);
            }
        }
        _ => {}
    }
}

fn normalize_base_path(base_path: &str) -> String {
    if base_path.ends_with('/') {
        base_path.to_string()
    } else {
        format!("{}/", base_path)
    }
}

fn derive_request_base_path(uri_path: &str, path: &StoragePath) -> Option<String> {
    let uri_trimmed = uri_path.trim_end_matches('/');
    let storage = path.to_string();
    let storage_trimmed = storage.trim_end_matches('/');
    if storage_trimmed.is_empty() {
        return Some(uri_trimmed.to_string());
    }
    let suffix = format!("/{}", storage_trimmed);
    let base = uri_trimmed.strip_suffix(&suffix)?;
    if base.is_empty() {
        return Some("/".to_string());
    }
    Some(base.to_string())
}

fn resolve_upstream_link(original: &str, upstream_base: &Url) -> Option<Url> {
    if let Ok(parsed) = Url::parse(original) {
        let host = parsed.host_str()?;
        if is_allowed_host(host, upstream_base) {
            return Some(parsed);
        }
        return None;
    }
    upstream_base.join(original).ok()
}

fn rewrite_known_host_url(
    original: &str,
    normalized_base: &str,
    upstream_base: &Url,
) -> Option<String> {
    let parsed = Url::parse(original).ok()?;
    let host = parsed.host_str()?;
    if !is_allowed_host(host, upstream_base) {
        return None;
    }
    Some(build_local_url(&parsed, normalized_base))
}

fn build_local_url(resolved: &Url, normalized_base: &str) -> String {
    let mut path = normalized_base.to_string();
    path.push_str(resolved.path().trim_start_matches('/'));
    if let Some(query) = resolved.query() {
        path.push('?');
        path.push_str(query);
    }
    if let Some(fragment) = resolved.fragment() {
        path.push('#');
        path.push_str(fragment);
    }
    path
}

fn replace_absolute_urls(input: &str, normalized_base: &str, upstream_base: &Url) -> String {
    let mut output = input.to_string();
    output = output
        .replace("https://files.pythonhosted.org/", normalized_base)
        .replace("http://files.pythonhosted.org/", normalized_base);

    if let Some(host) = upstream_base.host_str() {
        let https = format!("https://{host}/");
        let http = format!("http://{host}/");
        output = output.replace(&https, normalized_base);
        output = output.replace(&http, normalized_base);
    }
    output
}

fn is_allowed_host(host: &str, upstream_base: &Url) -> bool {
    let upstream_host = upstream_base.host_str().unwrap_or_default();
    host == upstream_host || host.ends_with("pythonhosted.org")
}

fn build_url(base: &ProxyURL, path: StoragePath, query: Option<&str>) -> Option<Url> {
    let mut upstream = Url::parse(base.as_ref()).ok()?;
    let path_string = path.to_string();
    let mut segments: Vec<&str> = path_string
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.first().map(|s| *s) == Some("simple")
        && segments.get(1).map(|s| *s) == Some("packages")
    {
        segments.remove(0);
    }
    if segments.first().map(|s| *s) == Some("packages") {
        if upstream.host_str() == Some("pypi.org") {
            if let Err(err) = upstream.set_host(Some("files.pythonhosted.org")) {
                warn!(?err, "Failed to set upstream host for python proxy");
                return None;
            }
            upstream.set_path("");
        }
    }
    {
        let mut path_segments = match upstream.path_segments_mut() {
            Ok(segments_mut) => segments_mut,
            Err(_) => {
                warn!("Upstream URL cannot be a base");
                return None;
            }
        };
        path_segments.clear();
        for segment in &segments {
            path_segments.push(segment);
        }
    }
    if path_string.ends_with('/') {
        let mut current = upstream.path().to_string();
        if !current.ends_with('/') {
            current.push('/');
            upstream.set_path(&current);
        }
    }
    upstream.set_query(query);
    Some(upstream)
}
static HREF_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"href="([^"]+)""#).unwrap_or_else(|e| panic!("Invalid Regex: {}", e))
});
