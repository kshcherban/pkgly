use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use parking_lot::RwLock;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;

use nr_core::{
    database::entities::repository::DBRepository,
    repository::{Visibility, config::RepositoryConfigType},
    storage::StoragePath,
};
use nr_storage::{DynStorage, FileContent, Storage};

use crate::{
    app::Pkgly,
    error::OtherInternalError,
    repository::{
        RepoResponse, Repository, RepositoryFactoryError, RepositoryRequest,
        utils::can_read_repository_with_auth,
    },
};

use super::{
    configs::DebProxyConfig,
    proxy_indexing::{DatabaseDebProxyIndexer, DebProxyIndexing, record_deb_proxy_cache_hit},
};

pub struct DebProxyInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub repository: DBRepository,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub config: DebProxyConfig,
    pub client: reqwest::Client,
    pub indexer: Arc<dyn DebProxyIndexing>,
}

impl std::fmt::Debug for DebProxyInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebProxyInner")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("visibility", &self.visibility.read())
            .field("active", &self.repository.active)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct DebProxyRepository(pub Arc<DebProxyInner>);

impl DebProxyRepository {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: DebProxyConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let client = reqwest::Client::builder()
            .user_agent("Pkgly Debian Proxy")
            .build()
            .map_err(|err| RepositoryFactoryError::InvalidConfig("deb", err.to_string()))?;
        let indexer: Arc<dyn DebProxyIndexing> =
            Arc::new(DatabaseDebProxyIndexer::new(site.clone(), repository.id));
        Ok(Self(Arc::new(DebProxyInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            repository,
            storage,
            site,
            config,
            client,
            indexer,
        })))
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn upstream(&self) -> &nr_core::repository::proxy_url::ProxyURL {
        &self.0.config.upstream_url
    }

    fn indexer(&self) -> &dyn DebProxyIndexing {
        self.0.indexer.as_ref()
    }

    pub async fn refresh_offline_mirror(
        &self,
    ) -> Result<
        super::proxy_refresh::DebProxyRefreshSummary,
        super::proxy_refresh::DebProxyRefreshError,
    > {
        super::proxy_refresh::refresh_deb_proxy_offline_mirror(
            &self.0.client,
            &self.storage(),
            self.id(),
            &self.0.config,
            self.indexer(),
        )
        .await
    }
}

fn build_upstream_url(
    base: &nr_core::repository::proxy_url::ProxyURL,
    path: &StoragePath,
    query: Option<&str>,
) -> Option<Url> {
    let mut upstream = Url::parse(base.as_ref()).ok()?;
    let base_path = upstream.path().trim_end_matches('/').to_string();
    let request_path = path.to_string();
    let request_path = request_path.trim_start_matches('/').to_string();
    let had_trailing_slash = request_path.ends_with('/');

    let mut combined = if base_path.is_empty() || base_path == "/" {
        format!("/{}", request_path)
    } else if request_path.is_empty() {
        base_path
    } else {
        format!("{}/{}", base_path, request_path)
    };

    if had_trailing_slash && !combined.ends_with('/') {
        combined.push('/');
    }

    upstream.set_path(&combined);
    upstream.set_query(query);
    Some(upstream)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CacheThroughOutcome {
    Hit,
    Fetched(Bytes),
    UpstreamStatus(StatusCode),
}

async fn fetch_and_cache_if_missing(
    client: &reqwest::Client,
    storage: &DynStorage,
    repository_id: Uuid,
    upstream: &nr_core::repository::proxy_url::ProxyURL,
    path: &StoragePath,
    query: Option<&str>,
) -> Result<CacheThroughOutcome, super::DebRepositoryError> {
    if storage
        .get_file_information(repository_id, path)
        .await?
        .is_some()
    {
        return Ok(CacheThroughOutcome::Hit);
    }

    let Some(url) = build_upstream_url(upstream, path, query) else {
        return Ok(CacheThroughOutcome::UpstreamStatus(StatusCode::BAD_GATEWAY));
    };

    let response = crate::utils::upstream::send(client, client.get(url.clone()))
        .await
        .map_err(|err| super::DebRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;
    let status = response.status();
    if !status.is_success() {
        return Ok(CacheThroughOutcome::UpstreamStatus(status));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| super::DebRepositoryError::Other(Box::new(OtherInternalError::new(err))))?;
    storage
        .save_file(repository_id, FileContent::Bytes(bytes.clone()), path)
        .await?;
    debug!(
        url.full = %crate::utils::upstream::sanitize_url_for_logging(&url),
        path = %path.to_string(),
        "Cached deb proxy upstream response"
    );
    Ok(CacheThroughOutcome::Fetched(bytes))
}

impl Repository for DebProxyRepository {
    type Error = super::DebRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "deb"
    }

    fn full_type(&self) -> &'static str {
        "deb/proxy"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            super::configs::DebRepositoryConfigType::get_type_static(),
            super::super::RepositoryAuthConfigType::get_type_static(),
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
        self.0.repository.active
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
            if !can_read_repository_with_auth(
                &request.authentication,
                this.visibility(),
                this.id(),
                this.site().as_ref(),
                &request.auth_config,
            )
            .await?
            {
                return Ok(RepoResponse::unauthorized());
            }

            let query = request.parts.uri.query();
            let path = request.path;
            if path.is_directory() {
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Directory not found",
                ));
            }

            if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                return Ok(file.into());
            }

            let outcome = fetch_and_cache_if_missing(
                &this.0.client,
                &this.storage(),
                this.id(),
                this.upstream(),
                &path,
                query,
            )
            .await?;

            match outcome {
                CacheThroughOutcome::Fetched(bytes) => {
                    if let Err(err) =
                        record_deb_proxy_cache_hit(this.indexer(), &path, bytes, None).await
                    {
                        return Err(super::DebRepositoryError::Other(Box::new(err)));
                    }
                    if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                        return Ok(file.into());
                    }
                    warn!(path = %path.to_string(), "Deb proxy cached response but file missing");
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "File not found",
                    ))
                }
                CacheThroughOutcome::Hit => {
                    if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                        return Ok(file.into());
                    }
                    warn!(path = %path.to_string(), "Deb proxy cache reported success but file missing");
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "File not found",
                    ))
                }
                CacheThroughOutcome::UpstreamStatus(status) => {
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
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            if !can_read_repository_with_auth(
                &request.authentication,
                this.visibility(),
                this.id(),
                this.site().as_ref(),
                &request.auth_config,
            )
            .await?
            {
                return Ok(RepoResponse::unauthorized());
            }

            let query = request.parts.uri.query();
            let path = request.path;
            if path.is_directory() {
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Directory not found",
                ));
            }

            if let Some(meta) = this
                .storage()
                .get_file_information(this.id(), &path)
                .await?
            {
                return Ok(meta.into());
            }

            let outcome = fetch_and_cache_if_missing(
                &this.0.client,
                &this.storage(),
                this.id(),
                this.upstream(),
                &path,
                query,
            )
            .await?;

            match outcome {
                CacheThroughOutcome::Fetched(bytes) => {
                    if let Err(err) =
                        record_deb_proxy_cache_hit(this.indexer(), &path, bytes, None).await
                    {
                        return Err(super::DebRepositoryError::Other(Box::new(err)));
                    }
                    if let Some(meta) = this
                        .storage()
                        .get_file_information(this.id(), &path)
                        .await?
                    {
                        return Ok(meta.into());
                    }
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "File not found",
                    ))
                }
                CacheThroughOutcome::Hit => {
                    if let Some(meta) = this
                        .storage()
                        .get_file_information(this.id(), &path)
                        .await?
                    {
                        return Ok(meta.into());
                    }
                    Ok(RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "File not found",
                    ))
                }
                CacheThroughOutcome::UpstreamStatus(status) => {
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
    }
}

#[cfg(test)]
mod tests;
