use std::{fmt::Write, sync::Arc};

use axum::http::Uri;
use bytes::Bytes;
use http::StatusCode;
use nr_core::{
    database::entities::{
        project::{
            DBProject, NewProject, ProjectDBType,
            versions::{DBProjectVersion, NewVersion},
        },
        repository::DBRepository,
    },
    repository::{
        Visibility,
        config::RepositoryConfigType,
        project::{
            Author, CargoDependencyMetadata, CargoPackageMetadata, Licence, ProjectSource,
            ReleaseType, VersionData,
        },
    },
};
use nr_storage::{DynStorage, FileContent, Storage, StorageFile};
use parking_lot::RwLock;
use semver::Version;
use sha2::{Digest, Sha256};
use tokio::task;
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::{
    app::{
        Pkgly,
        webhooks::{self, PackageWebhookActor, WebhookEventType},
    },
    repository::{
        RepoResponse, Repository, RepositoryAuthConfig, RepositoryAuthConfigType,
        RepositoryFactoryError, RepositoryRequest, repo_http::RepositoryAuthentication,
        utils::can_read_repository_with_auth,
    },
    utils::ResponseBuilder,
};

use super::utils::{
    PublishPayload, build_config_json, build_index_entry, build_login_response,
    crate_archive_storage_path, parse_publish_payload, sparse_index_storage_path,
};
use super::{CargoRepositoryConfig, CargoRepositoryConfigType, CargoRepositoryError};

#[derive(Debug)]
pub struct CargoRepositoryInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub repository: DBRepository,
    pub config: CargoRepositoryConfig,
    pub storage_name: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct CargoHosted(pub Arc<CargoRepositoryInner>);

impl CargoHosted {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: CargoRepositoryConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let storage_name = storage.storage_config().storage_config.storage_name.clone();
        let repo_name = repository.name.to_string();
        let visibility = repository.visibility;
        let active = repository.active;
        Ok(Self(Arc::new(CargoRepositoryInner {
            id: repository.id,
            name: repo_name,
            visibility: RwLock::new(visibility),
            storage,
            site,
            repository,
            config,
            storage_name,
            active,
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

    fn repository(&self) -> &DBRepository {
        &self.0.repository
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }

    fn storage_name(&self) -> &str {
        &self.0.storage_name
    }

    fn ensure_active(&self) -> Result<(), CargoRepositoryError> {
        if !self.0.active {
            return Err(CargoRepositoryError::InvalidRequest(
                "Repository is not active".to_string(),
            ));
        }
        Ok(())
    }

    async fn ensure_can_read(
        &self,
        authentication: RepositoryAuthentication,
        auth_config: RepositoryAuthConfig,
    ) -> Result<bool, CargoRepositoryError> {
        Ok(can_read_repository_with_auth(
            &authentication,
            self.visibility(),
            self.id(),
            self.site().as_ref(),
            &auth_config,
        )
        .await?)
    }

    fn base_uri(&self, request: &RepositoryRequest) -> Uri {
        let headers = &request.parts.headers;
        let (base, _) =
            crate::repository::docker::auth::resolve_registry_location(&self.site(), Some(headers));
        base.parse::<Uri>()
            .unwrap_or_else(|_| Uri::from_static("http://localhost"))
    }

    async fn handle_config(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, CargoRepositoryError> {
        let uri = self.base_uri(&request);
        let requires_auth = request.auth_config.enabled
            || matches!(self.visibility(), Visibility::Private | Visibility::Hidden);
        let json = build_config_json(
            &uri,
            self.storage_name(),
            &self.repository().name,
            requires_auth,
        );
        Ok(RepoResponse::Other(ResponseBuilder::ok().json(&json)))
    }

    async fn handle_me(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, CargoRepositoryError> {
        let uri = self.base_uri(&request);
        let json = build_login_response(&uri);
        Ok(RepoResponse::Other(ResponseBuilder::ok().json(&json)))
    }

    async fn handle_login(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, CargoRepositoryError> {
        let _ = request.body.body_as_bytes().await?;
        Ok(RepoResponse::Other(
            ResponseBuilder::ok().json(&serde_json::json!({ "ok": true })),
        ))
    }

    async fn handle_sparse_index(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, CargoRepositoryError> {
        let auth = request.authentication.clone();
        let auth_config = request.auth_config.clone();
        if !self.ensure_can_read(auth, auth_config).await? {
            return Ok(RepoResponse::unauthorized());
        }
        let storage = self.storage();
        match storage.open_file(self.id(), &request.path).await? {
            Some(file) => Ok(file.into()),
            None => Ok(RepoResponse::Other(ResponseBuilder::not_found().empty())),
        }
    }

    async fn handle_download(
        &self,
        crate_name: &str,
        version: &str,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, CargoRepositoryError> {
        let auth = request.authentication.clone();
        let auth_config = request.auth_config.clone();
        if !self.ensure_can_read(auth, auth_config).await? {
            return Ok(RepoResponse::unauthorized());
        }
        let version = Version::parse(version).map_err(|err| {
            CargoRepositoryError::InvalidRequest(format!("Invalid version: {err}"))
        })?;
        let path = crate_archive_storage_path(crate_name, &version);
        match self.storage().open_file(self.id(), &path).await? {
            Some(file) => Ok(file.into()),
            None => Ok(RepoResponse::Other(
                ResponseBuilder::not_found().body("Crate version not found"),
            )),
        }
    }

    async fn handle_crate_metadata(
        &self,
        crate_name: &str,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, CargoRepositoryError> {
        let auth = request.authentication.clone();
        let auth_config = request.auth_config.clone();
        if !self.ensure_can_read(auth, auth_config).await? {
            return Ok(RepoResponse::unauthorized());
        }
        let normalized = super::utils::normalize_crate_name(crate_name);
        let Some(project) =
            DBProject::find_by_project_key(&normalized, self.id(), self.site().as_ref()).await?
        else {
            return Ok(RepoResponse::Other(
                ResponseBuilder::not_found().body(format!("Crate {crate_name} not found")),
            ));
        };
        let versions =
            DBProjectVersion::get_all_versions(project.id, &self.site().database).await?;
        let versions_json: Vec<serde_json::Value> = versions
            .iter()
            .map(|version| {
                serde_json::json!({
                    "crate": crate_name,
                    "num": version.version,
                    "created_at": version.created_at,
                    "updated_at": version.updated_at,
                    "downloads": 0,
                    "features": version.extra.0.extra.as_ref().and_then(|extra| extra.get("features")).cloned().unwrap_or_else(|| serde_json::json!({})),
                    "yanked": version
                        .extra
                        .0
                        .extra
                        .as_ref()
                        .and_then(|extra| extra.get("yanked"))
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                })
            })
            .collect();

        let response = serde_json::json!({
            "crate": {
                "id": crate_name,
                "name": crate_name,
                "description": project.description,
            },
            "versions": versions_json,
        });
        Ok(RepoResponse::Other(ResponseBuilder::ok().json(&response)))
    }

    async fn handle_publish(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, CargoRepositoryError> {
        self.ensure_active()?;
        let Some(user) = request
            .authentication
            .get_user_if_has_action(
                nr_core::user::permissions::RepositoryActions::Write,
                self.id(),
                self.site().as_ref(),
            )
            .await?
        else {
            return Ok(RepoResponse::unauthorized());
        };
        let body = request.body.body_as_bytes().await?;
        let payload = parse_publish_payload(&body)?;
        let publish_path = crate_archive_storage_path(&payload.metadata.name, &payload.metadata.vers);
        self.persist_publish(Some(user.id), payload).await?;
        if let Err(err) = webhooks::enqueue_package_path_event(
            &self.site(),
            self.id(),
            WebhookEventType::PackagePublished,
            publish_path.to_string(),
            PackageWebhookActor::from_user(&user),
            false,
        )
        .await
        {
            tracing::warn!(error = %err, "Failed to enqueue cargo publish webhook");
        }
        Ok(RepoResponse::Other(ResponseBuilder::ok().json(
            &serde_json::json!({
                "ok": true
            }),
        )))
    }

    #[instrument(skip_all, fields(crate = %payload.metadata.name, version = %payload.metadata.vers))]
    async fn persist_publish(
        &self,
        publisher: Option<i32>,
        payload: PublishPayload,
    ) -> Result<(), CargoRepositoryError> {
        let site = self.site();
        let normalized = super::utils::normalize_crate_name(&payload.metadata.name);
        let project = if let Some(project) =
            DBProject::find_by_project_key(&normalized, self.id(), site.as_ref()).await?
        {
            project
        } else {
            let storage_path = format!("crates/{normalized}/");
            let new_project = NewProject {
                scope: None,
                project_key: normalized.clone(),
                name: payload.metadata.name.clone(),
                description: payload.metadata.description.clone(),
                repository: self.id(),
                storage_path,
            };
            new_project.insert(site.as_ref()).await?
        };

        if DBProjectVersion::find_by_version_and_project(
            &payload.metadata.vers.to_string(),
            project.id,
            &site.database,
        )
        .await?
        .is_some()
        {
            return Err(CargoRepositoryError::VersionExists {
                crate_name: payload.metadata.name.clone(),
                version: payload.metadata.vers.clone(),
            });
        }

        let checksum = self
            .save_crate_archive(&payload.metadata.name, &payload.metadata.vers, &payload)
            .await?;

        self.update_sparse_index(&payload.metadata, &checksum)
            .await?;

        self.persist_version_metadata(publisher, project.id, &payload, checksum)
            .await?;

        Ok(())
    }

    async fn save_crate_archive(
        &self,
        crate_name: &str,
        version: &Version,
        payload: &PublishPayload,
    ) -> Result<String, CargoRepositoryError> {
        let path = crate_archive_storage_path(crate_name, version);
        let bytes = Bytes::from(payload.crate_archive.clone());
        let (_, _) = self
            .storage()
            .save_file(self.id(), FileContent::Bytes(bytes.clone()), &path)
            .await?;

        let mut hasher = Sha256::new();
        hasher.update(&payload.crate_archive);
        let digest = hasher.finalize();
        let mut checksum = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(&mut checksum, "{byte:02x}").map_err(|err| {
                CargoRepositoryError::InvalidRequest(format!("Failed to write checksum: {err}"))
            })?;
        }
        Ok(checksum)
    }

    async fn update_sparse_index(
        &self,
        metadata: &super::utils::PublishMetadata,
        checksum: &str,
    ) -> Result<(), CargoRepositoryError> {
        let path = sparse_index_storage_path(&metadata.name)?;
        let storage = self.storage();
        let repository = self.id();
        let existing_bytes = if let Some(StorageFile::File { content, meta }) =
            storage.open_file(repository, &path).await?
        {
            let size_hint = meta.file_type.file_size as usize;
            Some(content.read_to_vec(size_hint).await.map_err(|err| {
                CargoRepositoryError::InvalidRequest(format!(
                    "Failed to read existing index: {err}"
                ))
            })?)
        } else {
            None
        };

        let metadata_clone = metadata.clone();
        let checksum_owned = checksum.to_string();
        let content = task::spawn_blocking(move || -> Result<String, CargoRepositoryError> {
            let mut entries: Vec<serde_json::Value> = Vec::new();
            if let Some(bytes) = existing_bytes {
                if !bytes.is_empty() {
                    let existing = String::from_utf8(bytes).map_err(|err| {
                        CargoRepositoryError::InvalidRequest(format!(
                            "Invalid UTF-8 in index file: {err}"
                        ))
                    })?;
                    for line in existing.lines().filter(|line| !line.trim().is_empty()) {
                        let entry: serde_json::Value =
                            serde_json::from_str(line).map_err(|err| {
                                CargoRepositoryError::InvalidRequest(format!(
                                    "Invalid JSON entry in index: {err}"
                                ))
                            })?;
                        if entry["vers"] == metadata_clone.vers.to_string() {
                            return Err(CargoRepositoryError::VersionExists {
                                crate_name: metadata_clone.name.clone(),
                                version: metadata_clone.vers.clone(),
                            });
                        }
                        entries.push(entry);
                    }
                }
            }

            let new_entry = build_index_entry(&metadata_clone, &checksum_owned);
            entries.push(new_entry);

            let mut serialized = Vec::with_capacity(entries.len());
            for entry in entries {
                serialized.push(serde_json::to_string(&entry).map_err(|err| {
                    CargoRepositoryError::InvalidRequest(format!(
                        "Failed to serialize index entry: {err}"
                    ))
                })?);
            }
            Ok(serialized.join("\n"))
        })
        .await
        .map_err(|err| {
            CargoRepositoryError::InvalidRequest(format!("Index update task failed: {err}"))
        })??;

        storage
            .save_file(
                repository,
                FileContent::Content(content.into_bytes()),
                &path,
            )
            .await?;
        Ok(())
    }

    async fn persist_version_metadata(
        &self,
        publisher: Option<i32>,
        project_id: Uuid,
        payload: &PublishPayload,
        checksum: String,
    ) -> Result<(), CargoRepositoryError> {
        let version = payload.metadata.vers.to_string();
        let release_type = ReleaseType::release_type_from_version(&version);
        let normalized = super::utils::normalize_crate_name(&payload.metadata.name);
        let version_path = format!("crates/{normalized}/{version}");

        let dependencies: Vec<CargoDependencyMetadata> = payload
            .metadata
            .deps
            .iter()
            .map(|dep| CargoDependencyMetadata {
                name: dep.name.clone(),
                req: dep.vers.clone(),
                optional: dep.optional,
                default_features: dep.default_features,
                features: dep.features.clone(),
                target: dep.target.clone(),
                kind: dep.kind.clone(),
                registry: dep.registry.clone(),
                package: dep.package.clone(),
            })
            .collect();

        let authors = payload
            .metadata
            .authors
            .iter()
            .map(|author| Author {
                name: Some(author.clone()),
                email: None,
                website: None,
            })
            .collect();

        let source = payload
            .metadata
            .repository
            .as_ref()
            .map(|url| ProjectSource::Git {
                url: url.clone(),
                branch: None,
                commit: None,
            });
        let licence = payload
            .metadata
            .license
            .as_ref()
            .map(|lic| Licence::Simple(lic.clone()));

        let extra = CargoPackageMetadata {
            checksum,
            crate_size: payload.crate_archive.len() as u64,
            yanked: false,
            features: payload.metadata.features.clone(),
            dependencies,
            extra: None,
        };

        let version_data = VersionData {
            documentation_url: payload.metadata.documentation.clone(),
            website: payload.metadata.homepage.clone(),
            authors,
            description: payload.metadata.description.clone(),
            source,
            licence,
            extra: Some(serde_json::to_value(extra)?),
        };

        let new_version = NewVersion {
            project_id,
            repository_id: self.id(),
            version,
            release_type,
            version_path,
            publisher,
            version_page: None,
            extra: version_data,
        };
        new_version.insert(&self.site().database).await?;
        Ok(())
    }
}

impl Repository for CargoHosted {
    type Error = CargoRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "cargo"
    }

    fn full_type(&self) -> &'static str {
        "cargo/hosted"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            CargoRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn name(&self) -> String {
        self.repository().name.to_string()
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
            let path_string = request.path.to_string();
            debug!(%path_string, "Cargo repository GET");
            if path_string == "config.json" || path_string == "index/config.json" {
                return this.handle_config(request).await;
            }
            if path_string == "api/v1/me" {
                return this.handle_me(request).await;
            }
            if path_string.starts_with("index/") {
                return this.handle_sparse_index(request).await;
            }

            let components: Vec<String> = request
                .path
                .clone()
                .into_iter()
                .map(|c| c.to_string())
                .collect();

            if components.len() == 6
                && components[0] == "api"
                && components[1] == "v1"
                && components[2] == "crates"
                && components[5] == "download"
            {
                return this
                    .handle_download(&components[3], &components[4], request)
                    .await;
            }

            if components.len() == 4
                && components[0] == "api"
                && components[1] == "v1"
                && components[2] == "crates"
            {
                return this.handle_crate_metadata(&components[3], request).await;
            }

            Ok(RepoResponse::Other(
                ResponseBuilder::not_found().body("Not Found"),
            ))
        }
    }

    fn handle_put(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            let path_string = request.path.to_string();
            if path_string == "api/v1/crates/new" {
                return this.handle_publish(request).await;
            }
            if path_string == "api/v1/me" {
                return this.handle_login(request).await;
            }
            Ok(RepoResponse::Other(
                ResponseBuilder::default()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body("Unsupported PUT path for cargo repository"),
            ))
        }
    }
}
