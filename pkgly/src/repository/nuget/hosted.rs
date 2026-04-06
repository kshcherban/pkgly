use std::sync::Arc;

use http::{StatusCode, header::CONTENT_TYPE};
use nr_core::{
    database::entities::repository::DBRepository,
    repository::{Visibility, config::RepositoryConfigType},
    storage::StoragePath,
};
use nr_storage::{DynStorage, FileContent, Storage};
use parking_lot::RwLock;
use tracing::debug;
use uuid::Uuid;

use super::{
    NugetError,
    utils::{
        ParsedNugetPackage, REPOSITORY_TYPE_ID, base_repository_path, build_registration_index,
        build_registration_leaf, can_read_with_auth, external_repository_base,
        find_hosted_version, flatcontainer_index_path, flatcontainer_nuspec_path,
        flatcontainer_package_path, hosted_leaf, json_response, list_hosted_versions,
        parse_published_package, push_requires_write, registration_index_path,
        registration_leaf_path, resolve_project_version, response_from_storage, service_index,
        upsert_hosted_metadata, xml_response,
    },
};
use crate::{
    app::Pkgly,
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
    },
};

#[derive(Debug)]
pub struct NugetHostedInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub repository: DBRepository,
    pub storage: DynStorage,
    pub site: Pkgly,
}

#[derive(Debug, Clone)]
pub struct NugetHosted(pub Arc<NugetHostedInner>);

impl NugetHosted {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
    ) -> Result<Self, RepositoryFactoryError> {
        Ok(Self(Arc::new(NugetHostedInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            repository,
            storage,
            site,
        })))
    }

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn id(&self) -> Uuid {
        self.0.id
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }

    fn base_path(&self) -> String {
        let storage = self.storage().storage_config().storage_config.storage_name.clone();
        base_repository_path(&storage, &self.0.name)
    }

    pub async fn resolve_project(
        &self,
        path: &StoragePath,
    ) -> Result<nr_core::repository::project::ProjectResolution, NugetError> {
        resolve_project_version(self.id(), path, self.site().as_ref()).await
    }

    fn handle_service_index(
        &self,
        method: http::Method,
        base_url: String,
        allow_publish: bool,
    ) -> RepoResponse {
        let body = service_index(&base_url, allow_publish);
        RepoResponse::Other(json_response(&method, &body))
    }

    async fn handle_flatcontainer_index(
        &self,
        method: http::Method,
        package_id: &str,
    ) -> Result<RepoResponse, NugetError> {
        let versions = list_hosted_versions(&self.site(), &self.storage(), self.id(), package_id).await?;
        if versions.is_empty() {
            return Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Package not found"));
        }
        let body = serde_json::json!({
            "versions": versions.iter().map(|entry| entry.lower_version.clone()).collect::<Vec<_>>()
        });
        Ok(RepoResponse::Other(json_response(&method, &body)))
    }

    async fn handle_registration_index(
        &self,
        method: http::Method,
        base_url: String,
        package_id: &str,
    ) -> Result<RepoResponse, NugetError> {
        let versions = list_hosted_versions(&self.site(), &self.storage(), self.id(), package_id).await?;
        if versions.is_empty() {
            return Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Package not found"));
        }
        let leaves: Vec<_> = versions
            .iter()
            .map(|entry| hosted_leaf(&base_url, package_id, entry))
            .collect();
        let body = build_registration_index(&base_url, package_id, &leaves);
        Ok(RepoResponse::Other(json_response(&method, &body)))
    }

    async fn handle_registration_leaf(
        &self,
        method: http::Method,
        base_url: String,
        package_id: &str,
        version: &str,
    ) -> Result<RepoResponse, NugetError> {
        let Some(version) =
            find_hosted_version(&self.site(), &self.storage(), self.id(), package_id, version).await?
        else {
            return Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Package not found"));
        };
        let leaf = hosted_leaf(&base_url, package_id, &version);
        let body = build_registration_leaf(&base_url, package_id, &leaf);
        Ok(RepoResponse::Other(json_response(&method, &body)))
    }

    async fn publish_package(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NugetError> {
        let Some(publisher) = push_requires_write(&request.authentication, self.id(), &self.site()).await? else {
            return Ok(RepoResponse::unauthorized());
        };

        let content_type = request
            .parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if !content_type.starts_with("multipart/form-data") {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_REQUEST,
                "NuGet push requires multipart/form-data",
            ));
        }

        let package_bytes = super::utils::first_multipart_bytes(request.body, &content_type).await?;
        let package = parse_published_package(package_bytes)?;
        let nupkg_path = flatcontainer_package_path(&package.package_id, &package.version);
        let nuspec_path = flatcontainer_nuspec_path(&package.package_id, &package.version);

        if self.storage().file_exists(self.id(), &nupkg_path).await? {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::CONFLICT,
                "Package already exists",
            ));
        }

        self.persist_package(&package).await?;
        upsert_hosted_metadata(&self.site(), self.id(), &package, Some(publisher)).await?;

        debug!(
            package_id = %package.package_id,
            version = %package.version,
            "Published NuGet package"
        );

        let location = format!(
            "{}/v3/flatcontainer/{}/{}/{}.{}.nupkg",
            self.base_path(),
            package.lower_id,
            package.lower_version,
            package.lower_id,
            package.lower_version
        );
        let _ = nuspec_path;
        Ok(RepoResponse::put_response(true, location))
    }

    async fn persist_package(&self, package: &ParsedNugetPackage) -> Result<(), NugetError> {
        let nupkg_path = flatcontainer_package_path(&package.package_id, &package.version);
        let nuspec_path = flatcontainer_nuspec_path(&package.package_id, &package.version);

        self.storage()
            .save_file(
                self.id(),
                FileContent::Content(package.nupkg_bytes.clone().to_vec()),
                &nupkg_path,
            )
            .await?;
        self.storage()
            .save_file(
                self.id(),
                FileContent::Content(package.nuspec_xml.clone().into_bytes()),
                &nuspec_path,
            )
            .await?;

        let flat_index_path = flatcontainer_index_path(&package.package_id);
        let reg_index_path = registration_index_path(&package.package_id);
        let reg_leaf_path = registration_leaf_path(&package.package_id, &package.version);
        if let Err(err) = self.storage().delete_file(self.id(), &flat_index_path).await {
            debug!(?err, path = %flat_index_path, "Failed to invalidate NuGet flat-container cache");
        }
        if let Err(err) = self.storage().delete_file(self.id(), &reg_index_path).await {
            debug!(?err, path = %reg_index_path, "Failed to invalidate NuGet registration index cache");
        }
        if let Err(err) = self.storage().delete_file(self.id(), &reg_leaf_path).await {
            debug!(?err, path = %reg_leaf_path, "Failed to invalidate NuGet registration leaf cache");
        }
        Ok(())
    }

    async fn handle_read(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NugetError> {
        let authentication = request.authentication.clone();
        let can_read =
            can_read_with_auth(&authentication, self.visibility(), self.id(), &self.site()).await?;
        if !can_read {
            return Ok(RepoResponse::unauthorized());
        }

        let path = request.path.to_string();
        let method = request.parts.method.clone();
        let base_url = external_repository_base(&self.site(), Some(&request.parts), &self.base_path());
        if path.is_empty() || path == "v3" || path == "v3/" || path == "v3/index.json" || path == "index.json" {
            return Ok(self.handle_service_index(method, base_url, true));
        }

        let parts: Vec<_> = path.split('/').collect();
        if parts.len() >= 4 && parts[0] == "v3" && parts[1] == "flatcontainer" && parts[3] == "index.json" {
            return self.handle_flatcontainer_index(method, parts[2]).await;
        }

        if parts.len() == 5 && parts[0] == "v3" && parts[1] == "flatcontainer" && parts[4].ends_with(".nuspec") {
            let file = response_from_storage(&self.storage(), self.id(), &request.path).await?;
            if matches!(&file, RepoResponse::Other(_)) {
                return Ok(file);
            }
            if let Some(bytes) = super::utils::read_storage_bytes(&self.storage(), self.id(), &request.path).await? {
                let xml = String::from_utf8(bytes)?;
                return Ok(RepoResponse::Other(xml_response(&request.parts.method, xml)));
            }
            return Ok(RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Package not found"));
        }

        if parts.len() == 5 && parts[0] == "v3" && parts[1] == "flatcontainer" && parts[4].ends_with(".nupkg") {
            return response_from_storage(&self.storage(), self.id(), &request.path).await;
        }

        if parts.len() == 4 && parts[0] == "v3" && parts[1] == "registration" && parts[3] == "index.json" {
            return self.handle_registration_index(method, base_url, parts[2]).await;
        }

        if parts.len() == 4 && parts[0] == "v3" && parts[1] == "registration" && parts[3].ends_with(".json") {
            let version = parts[3].trim_end_matches(".json");
            return self.handle_registration_leaf(method, base_url, parts[2], version).await;
        }

        Ok(RepoResponse::basic_text_response(
            StatusCode::NOT_FOUND,
            format!("Unsupported NuGet path: {path}"),
        ))
    }
}

impl Repository for NugetHosted {
    type Error = NugetError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn full_type(&self) -> &'static str {
        "nuget/hosted"
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
        self.0.repository.active
    }

    fn site(&self) -> Pkgly {
        self.site()
    }

    fn resolve_project_and_version_for_path(
        &self,
        path: &StoragePath,
    ) -> impl std::future::Future<Output = Result<nr_core::repository::project::ProjectResolution, Self::Error>> + Send {
        let this = self.clone();
        let path = path.clone();
        async move { this.resolve_project(&path).await }
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
        async move { this.publish_package(request).await }
    }

    fn handle_post(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.publish_package(request).await }
    }
}
