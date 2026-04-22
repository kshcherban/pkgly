use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use nr_core::{
    database::entities::repository::DBRepository,
    repository::{Visibility, config::RepositoryConfigType, proxy_url::ProxyURL},
    storage::StoragePath,
};
use nr_storage::{DynStorage, FileContent, Storage};
use parking_lot::RwLock;
use serde_json::Value;
use tokio::sync::RwLock as AsyncRwLock;
use tracing::warn;
use url::Url;
use uuid::Uuid;

use super::{
    NugetError, NugetProxyConfig,
    utils::{
        REPOSITORY_TYPE_ID, base_repository_path, can_read_with_auth, external_repository_base,
        json_response, parse_flatcontainer_index_versions, parse_published_package,
        read_storage_bytes, registration_index_path, rewrite_upstream_urls, save_json_cache,
        save_text_cache, service_index, upsert_proxy_metadata,
    },
};
use crate::{
    app::Pkgly,
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
    },
};

#[derive(Debug, Clone)]
struct ProxyResources {
    flatcontainer_base: String,
    registration_base: String,
    publish_base: Option<String>,
}

#[derive(Debug)]
pub struct NugetProxyInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub active: bool,
    pub upstream_url: ProxyURL,
    pub client: reqwest::Client,
    resources: AsyncRwLock<Option<ProxyResources>>,
}

#[derive(Debug, Clone)]
pub struct NugetProxy(pub Arc<NugetProxyInner>);

impl NugetProxy {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: NugetProxyConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let client = reqwest::Client::builder()
            .user_agent("Pkgly NuGet Proxy")
            .build()
            .map_err(|err| {
                RepositoryFactoryError::InvalidConfig(
                    super::NugetRepositoryConfigType::get_type_static(),
                    err.to_string(),
                )
            })?;
        Ok(Self(Arc::new(NugetProxyInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            storage,
            site,
            active: repository.active,
            upstream_url: config.upstream_url,
            client,
            resources: AsyncRwLock::new(None),
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

    fn base_path(&self) -> String {
        let storage = self
            .storage()
            .storage_config()
            .storage_config
            .storage_name
            .clone();
        base_repository_path(&storage, &self.0.name)
    }

    fn upstream_service_index_url(&self) -> Result<Url, NugetError> {
        let upstream = self.0.upstream_url.as_str();
        if upstream.ends_with("/index.json") {
            return Url::parse(upstream).map_err(NugetError::from);
        }
        let mut url = Url::parse(upstream).map_err(NugetError::from)?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| NugetError::InvalidPackage("Invalid upstream URL".into()))?;
            segments.push("v3");
            segments.push("index.json");
        }
        Ok(url)
    }

    async fn discover_resources(&self) -> Result<ProxyResources, NugetError> {
        let cached_resources = self.0.resources.read().await.clone();
        if let Some(cached) = cached_resources {
            return Ok(cached);
        }

        let service_index_url = self.upstream_service_index_url()?;
        let response =
            crate::utils::upstream::send(&self.0.client, self.0.client.get(service_index_url))
                .await?;
        if !response.status().is_success() {
            return Err(NugetError::InvalidPackage(format!(
                "NuGet upstream service index returned {}",
                response.status()
            )));
        }
        let value: Value = response.json().await?;
        let Some(resources) = value.get("resources").and_then(Value::as_array) else {
            return Err(NugetError::InvalidPackage(
                "NuGet upstream service index missing resources".into(),
            ));
        };

        let mut flatcontainer_base = None;
        let mut registration_base = None;
        let mut publish_base = None;
        for resource in resources {
            let Some(resource_type) = resource.get("@type") else {
                continue;
            };
            let Some(resource_id) = resource.get("@id").and_then(Value::as_str) else {
                continue;
            };
            let matches_type = |needle: &str| match resource_type {
                Value::String(value) => value.contains(needle),
                Value::Array(values) => values
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|value| value.contains(needle)),
                _ => false,
            };
            if flatcontainer_base.is_none() && matches_type("PackageBaseAddress") {
                flatcontainer_base = Some(resource_id.trim_end_matches('/').to_string());
            }
            if registration_base.is_none() && matches_type("RegistrationsBaseUrl") {
                registration_base = Some(resource_id.trim_end_matches('/').to_string());
            }
            if publish_base.is_none() && matches_type("PackagePublish") {
                publish_base = Some(resource_id.trim_end_matches('/').to_string());
            }
        }

        let resources = ProxyResources {
            flatcontainer_base: flatcontainer_base.ok_or_else(|| {
                NugetError::InvalidPackage("NuGet upstream missing PackageBaseAddress".into())
            })?,
            registration_base: registration_base.ok_or_else(|| {
                NugetError::InvalidPackage("NuGet upstream missing RegistrationsBaseUrl".into())
            })?,
            publish_base,
        };
        *self.0.resources.write().await = Some(resources.clone());
        Ok(resources)
    }

    async fn fetch_json(
        &self,
        upstream_url: Url,
        cache_path: &StoragePath,
        method: http::Method,
        local_base: &str,
        rewrite_registration: bool,
    ) -> Result<RepoResponse, NugetError> {
        if let Some(bytes) =
            super::utils::read_storage_bytes(&self.storage(), self.id(), cache_path).await?
        {
            let value: Value = serde_json::from_slice(&bytes)?;
            return Ok(RepoResponse::Other(json_response(&method, &value)));
        }

        let response =
            crate::utils::upstream::send(&self.0.client, self.0.client.get(upstream_url.clone()))
                .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Package not found",
            ));
        }
        if !response.status().is_success() {
            warn!(status = %response.status(), url = %upstream_url, "NuGet proxy upstream request failed");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_GATEWAY,
                "Upstream NuGet request failed",
            ));
        }

        let mut value: Value = response.json().await?;
        if rewrite_registration {
            let resources = self.discover_resources().await?;
            rewrite_upstream_urls(
                &mut value,
                &resources.registration_base,
                &resources.flatcontainer_base,
                resources.publish_base.as_deref(),
                local_base,
            );
        }
        save_json_cache(&self.storage(), self.id(), cache_path, &value).await?;
        Ok(RepoResponse::Other(json_response(&method, &value)))
    }

    async fn fetch_binary(
        &self,
        upstream_url: Url,
        cache_path: &StoragePath,
    ) -> Result<RepoResponse, NugetError> {
        if self.storage().file_exists(self.id(), cache_path).await? {
            if let Some(bytes) = read_storage_bytes(&self.storage(), self.id(), cache_path).await? {
                self.index_cached_package(cache_path, bytes.into(), Some(&upstream_url))
                    .await;
            }
            return Ok(self
                .storage()
                .open_file(self.id(), cache_path)
                .await?
                .into());
        }
        let response =
            crate::utils::upstream::send(&self.0.client, self.0.client.get(upstream_url.clone()))
                .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Package not found",
            ));
        }
        if !response.status().is_success() {
            warn!(status = %response.status(), url = %upstream_url, "NuGet proxy upstream binary request failed");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_GATEWAY,
                "Upstream NuGet request failed",
            ));
        }
        let bytes = response.bytes().await?;
        self.storage()
            .save_file(self.id(), FileContent::Content(bytes.to_vec()), cache_path)
            .await?;
        self.index_cached_package(cache_path, bytes.clone(), Some(&upstream_url))
            .await;
        Ok(self
            .storage()
            .open_file(self.id(), cache_path)
            .await?
            .into())
    }

    async fn index_cached_package(
        &self,
        cache_path: &StoragePath,
        bytes: Bytes,
        upstream_url: Option<&Url>,
    ) {
        let package = match parse_published_package(bytes.clone()) {
            Ok(package) => package,
            Err(err) => {
                warn!(?err, path = %cache_path, "Failed to parse cached NuGet package for indexing");
                return;
            }
        };

        let upstream_url_str = upstream_url.map(Url::as_str);
        if let Err(err) = upsert_proxy_metadata(
            &self.site(),
            self.id(),
            &package,
            cache_path,
            upstream_url_str,
            bytes.len() as u64,
        )
        .await
        {
            warn!(?err, path = %cache_path, "Failed to upsert NuGet proxy metadata");
        }
    }

    async fn fetch_nuspec(
        &self,
        upstream_url: Url,
        cache_path: &StoragePath,
        method: http::Method,
    ) -> Result<RepoResponse, NugetError> {
        if let Some(bytes) =
            super::utils::read_storage_bytes(&self.storage(), self.id(), cache_path).await?
        {
            let xml = String::from_utf8(bytes)?;
            return Ok(RepoResponse::Other(super::utils::xml_response(
                &method, xml,
            )));
        }
        let response =
            crate::utils::upstream::send(&self.0.client, self.0.client.get(upstream_url.clone()))
                .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Package not found",
            ));
        }
        if !response.status().is_success() {
            warn!(status = %response.status(), url = %upstream_url, "NuGet proxy upstream nuspec request failed");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_GATEWAY,
                "Upstream NuGet request failed",
            ));
        }
        let text = response.text().await?;
        save_text_cache(&self.storage(), self.id(), cache_path, text.clone()).await?;
        Ok(RepoResponse::Other(super::utils::xml_response(
            &method, text,
        )))
    }

    async fn handle_read(&self, request: RepositoryRequest) -> Result<RepoResponse, NugetError> {
        let authentication = request.authentication.clone();
        let can_read =
            can_read_with_auth(&authentication, self.visibility(), self.id(), &self.site()).await?;
        if !can_read {
            return Ok(RepoResponse::unauthorized());
        }

        let path = request.path.to_string();
        let method = request.parts.method.clone();
        let local_base =
            external_repository_base(&self.site(), Some(&request.parts), &self.base_path());
        if path.is_empty()
            || path == "v3"
            || path == "v3/"
            || path == "v3/index.json"
            || path == "index.json"
        {
            let body = service_index(&local_base, false);
            return Ok(RepoResponse::Other(json_response(&method, &body)));
        }

        let parts: Vec<_> = path.split('/').collect();
        let resources = self.discover_resources().await?;
        if parts.len() >= 4
            && parts[0] == "v3"
            && parts[1] == "flatcontainer"
            && parts[3] == "index.json"
        {
            let upstream = Url::parse(&format!(
                "{}/{}/index.json",
                resources.flatcontainer_base, parts[2]
            ))?;
            return self
                .fetch_json(upstream, &request.path, method.clone(), &local_base, false)
                .await;
        }
        if parts.len() == 5
            && parts[0] == "v3"
            && parts[1] == "flatcontainer"
            && parts[4].ends_with(".nupkg")
        {
            let upstream = Url::parse(&format!(
                "{}/{}/{}/{}",
                resources.flatcontainer_base, parts[2], parts[3], parts[4]
            ))?;
            return self.fetch_binary(upstream, &request.path).await;
        }
        if parts.len() == 5
            && parts[0] == "v3"
            && parts[1] == "flatcontainer"
            && parts[4].ends_with(".nuspec")
        {
            let upstream = Url::parse(&format!(
                "{}/{}/{}/{}",
                resources.flatcontainer_base, parts[2], parts[3], parts[4]
            ))?;
            return self
                .fetch_nuspec(upstream, &request.path, method.clone())
                .await;
        }
        if parts.len() == 4
            && parts[0] == "v3"
            && parts[1] == "registration"
            && parts[3] == "index.json"
        {
            let upstream = Url::parse(&format!(
                "{}/{}/index.json",
                resources.registration_base, parts[2]
            ))?;
            return self
                .fetch_json(upstream, &request.path, method.clone(), &local_base, true)
                .await;
        }
        if parts.len() == 4
            && parts[0] == "v3"
            && parts[1] == "registration"
            && parts[3].ends_with(".json")
        {
            let upstream = Url::parse(&format!(
                "{}/{}/{}",
                resources.registration_base, parts[2], parts[3]
            ))?;
            return self
                .fetch_json(upstream, &request.path, method.clone(), &local_base, true)
                .await;
        }

        let _ = parse_flatcontainer_index_versions(&Value::Null);
        let _ = registration_index_path(parts.get(2).copied().unwrap_or_default());
        Ok(RepoResponse::basic_text_response(
            StatusCode::NOT_FOUND,
            format!("Unsupported NuGet path: {path}"),
        ))
    }
}

impl Repository for NugetProxy {
    type Error = NugetError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn full_type(&self) -> &'static str {
        "nuget/proxy"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            super::NugetRepositoryConfigType::get_type_static(),
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
}
