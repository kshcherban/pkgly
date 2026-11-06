// ABOUTME: Serves hosted NPM package metadata and package publish/download requests.
// ABOUTME: Persists package versions and returns registry-compatible package identities.
use std::sync::Arc;

use ahash::{HashMap, HashMapExt};
use axum::response::{IntoResponse, Response};
use derive_more::derive::Deref;
use http::{StatusCode, header::CONTENT_TYPE};
use nr_core::{
    database::entities::{
        project::{npm::DBNpmProjectMetadata, versions::DBProjectVersion},
        repository::DBRepository,
    },
    repository::config::RepositoryConfigType,
    storage::StoragePath,
    user::permissions::RepositoryActions,
};
use nr_storage::{DynStorage, FileContent, Storage};
use tracing::{debug, info, instrument, warn};

use super::{
    metadata::{deprecation_updates, normalize_readme, package_fields, save_deprecations},
    tags::{load_tags, load_tags_on, parse_tag_path, publish_tags},
    types::{
        NPM_COMMAND_HEADER, NpmRegistryPackageResponse,
        request::{GetPath, InvalidNPMCommand, NPMCommand, PublishVersion},
    },
    utils::{NpmRegistryExt, npm_time},
};
use crate::{
    app::{
        Pkgly,
        webhooks::{self, PackageWebhookActor, WebhookEventType},
    },
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
        npm::{NPMRegistryConfigType, NPMRegistryError, types::PublishRequest},
        utils::RepositoryExt,
    },
    utils::ResponseBuilder,
};

#[derive(derive_more::Debug)]
pub struct NpmRegistryInner {
    #[debug(skip)]
    pub site: Pkgly,
    pub storage: DynStorage,
    pub id: uuid::Uuid,
    pub repository: DBRepository,
}
#[derive(Debug, Clone, Deref)]
pub struct NPMHostedRegistry(Arc<NpmRegistryInner>);
impl NPMHostedRegistry {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
    ) -> Result<Self, RepositoryFactoryError> {
        Ok(Self(Arc::new(NpmRegistryInner {
            site,
            storage,
            id: repository.id,
            repository,
        })))
    }
    #[instrument(skip(self, request), fields(repository_id = %self.id))]
    async fn handle_publish(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let Some(user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id, self.site.as_ref())
            .await?
        else {
            info!("No acceptable user authentication provided");
            return Ok(RepoResponse::unauthorized());
        };
        let body = request.body.body_as_string().await?;
        debug!(body.len = body.len(), "Handling publish request");
        let PublishRequest {
            name,
            attachments,
            versions,
            dist_tags,
            other,
        }: PublishRequest = serde_json::from_str(&body)?;
        if versions.len() != 1 || attachments.len() != 1 {
            return Err(NPMRegistryError::OnlyOneReleaseOrAttachmentAtATime);
        }
        let (version, mut data) = versions
            .into_iter()
            .next()
            .ok_or(NPMRegistryError::VersionNotFound)?;
        normalize_readme(&mut data, &other);
        let dist_tags = publish_tags(dist_tags, &version)?;
        let (file, attachment) = attachments
            .into_iter()
            .next()
            .ok_or(NPMRegistryError::OnlyOneReleaseOrAttachmentAtATime)?;
        let file = self.validate_publication(&request.path, &name, &version, &data, &file)?;
        let attachment_data = attachment.read_data()?;
        let version_path = self
            .persist_publication(user.id, &data, &file, attachment_data, dist_tags)
            .await?;

        if let Err(err) = webhooks::enqueue_package_path_event(
            &self.site,
            self.id,
            WebhookEventType::PackagePublished,
            version_path.to_string(),
            PackageWebhookActor::from_user(user),
            false,
        )
        .await
        {
            warn!(error = %err, "Failed to enqueue npm publish webhook");
        }

        Ok(ResponseBuilder::no_content().empty().into())
    }
    #[instrument(skip(self), fields(repository_id = %self.id))]
    async fn resolve_tarball_path(
        &self,
        name: &str,
        version: &str,
        file: &str,
    ) -> Result<Option<StoragePath>, NPMRegistryError> {
        let Some(project) = self.get_project_from_key(name).await? else {
            return Ok(None);
        };
        let Some(version) = self.get_project_version(project.id, version).await? else {
            return Ok(None);
        };
        let mut path = StoragePath::from(version.path.as_str());
        path.push_mut(file);
        debug!(project_id = %project.id, version_id = %version.id, storage_path = %path, "Getting file");
        Ok(Some(path))
    }

    #[instrument(skip(self, data), fields(repository_id = %self.id))]
    fn validate_publication(
        &self,
        route: &StoragePath,
        name: &str,
        version: &str,
        data: &PublishVersion,
        attachment: &str,
    ) -> Result<String, NPMRegistryError> {
        if name.is_empty()
            || data.name.name.is_empty()
            || name != data.name.to_string()
            || route.to_string() != name
            || version != data.version
            || semver::Version::parse(version).is_err()
        {
            return Err(NPMRegistryError::InvalidPublication);
        }
        let file = format!("{}-{version}.tgz", data.name.name);
        let scoped_attachment = data
            .name
            .scope
            .as_ref()
            .map(|scope| format!("@{scope}/{file}"));
        if attachment != file && scoped_attachment.as_deref() != Some(attachment) {
            return Err(NPMRegistryError::InvalidPublication);
        }
        let storage = self.storage.storage_config();
        let advertised = data
            .dist
            .validate_tarball(&storage.storage_config.storage_name, &self.repository.name)?;
        match GetPath::try_from(advertised) {
            Ok(GetPath::GetTar {
                name: target,
                version: target_version,
                file: target_file,
            }) if target == name && target_version == version && target_file == file => Ok(file),
            _ => Err(NPMRegistryError::InvalidPublication),
        }
    }

    #[instrument(skip(self, data, attachment, tags), fields(repository_id = %self.id, attachment_bytes = attachment.len()))]
    async fn persist_publication(
        &self,
        publisher: i32,
        data: &PublishVersion,
        file: &str,
        attachment: Vec<u8>,
        tags: HashMap<String, String>,
    ) -> Result<StoragePath, NPMRegistryError> {
        let project_path = StoragePath::from(data.name.to_string());
        let project = self.get_or_create_project(&project_path, data).await?;
        let fallback = load_tags(project.id, &self.site.database).await?;
        let mut version_path = project_path;
        version_path.push_mut(&data.version);
        self.create_version_if_missing(publisher, &version_path, &project, data)
            .await?;

        let mut tarball_path = version_path.clone();
        tarball_path.push_mut(file);
        info!(%file, attachment.size_bytes = attachment.len(), "Saving attachment");
        self.storage
            .save_file(self.id, FileContent::Content(attachment), &tarball_path)
            .await?;

        let mut tx = self.site.database.begin().await?;
        DBNpmProjectMetadata::lock_project(&mut tx, project.id).await?;
        DBNpmProjectMetadata::merge_tags(project.id, tags, fallback, &mut *tx).await?;
        tx.commit().await?;
        Ok(version_path)
    }

    #[instrument(skip(self, request), fields(repository_id = %self.id))]
    async fn handle_deprecate(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let GetPath::GetPackageInfo { name } = GetPath::try_from(request.path.clone())? else {
            return Err(NPMRegistryError::InvalidMetadataUpdate);
        };
        let body = request.body.body_as_string().await?;
        let body =
            serde_json::from_str(&body).map_err(|_| NPMRegistryError::InvalidMetadataUpdate)?;
        let updates = deprecation_updates(&body, &name)?;
        let version_count = updates.len();
        let Some(project) = self.get_project_from_key(&name).await? else {
            return Ok(ResponseBuilder::not_found().empty().into());
        };
        if !save_deprecations(project.id, updates, &self.site.database).await? {
            return Ok(ResponseBuilder::not_found().empty().into());
        }
        debug!(package = %name, version_count, "Updated npm deprecation messages");
        Ok(ResponseBuilder::no_content().empty().into())
    }
    #[instrument(skip(self, request), fields(repository_id = %self.id))]
    async fn handle_dist_tags(
        &self,
        request: RepositoryRequest,
        name: String,
        tag: Option<String>,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let method = request.parts.method.clone();
        if method == http::Method::GET
            && !super::tags::can_read_tags(
                &request.authentication,
                request.auth_config.enabled,
                self.repository.visibility,
                self.id,
                &self.site,
            )
            .await?
        {
            return Ok(RepoResponse::unauthorized());
        }
        if method != http::Method::GET
            && request
                .authentication
                .get_user_if_has_action(RepositoryActions::Write, self.id, self.site.as_ref())
                .await?
                .is_none()
        {
            return Ok(RepoResponse::unauthorized());
        }
        let Some(project) = self.get_project_from_key(&name).await? else {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Project not found".into())
                .into());
        };
        match (method, tag) {
            (http::Method::GET, None) => {
                let tags = load_tags(project.id, &self.site.database).await?;
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "application/json")
                    .body(serde_json::to_string(&tags)?.into())
                    .into())
            }
            (method @ (http::Method::PUT | http::Method::DELETE), Some(tag)) => {
                self.change_dist_tag(project.id, request, method, tag).await
            }
            _ => Err(NPMRegistryError::InvalidDistTags),
        }
    }

    #[instrument(skip(self, request), fields(repository_id = %self.id))]
    async fn change_dist_tag(
        &self,
        project: uuid::Uuid,
        request: RepositoryRequest,
        method: http::Method,
        tag: String,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let version = if method == http::Method::PUT {
            Some(
                serde_json::from_str::<String>(&request.body.body_as_string().await?)
                    .map_err(|_| NPMRegistryError::InvalidDistTags)?,
            )
        } else {
            if tag == "latest" {
                return Err(NPMRegistryError::CannotRemoveLatest);
            }
            None
        };
        let mut tx = self.site.database.begin().await?;
        DBNpmProjectMetadata::lock_project(&mut tx, project).await?;
        let fallback = load_tags_on(project, &mut tx).await?;
        if let Some(version) = version {
            if !DBNpmProjectMetadata::set_tag(project, tag, version, fallback, &mut tx).await? {
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Version not found",
                ));
            }
        } else {
            if !fallback.contains_key(&tag) {
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Tag not found",
                ));
            }
            DBNpmProjectMetadata::remove_tag(project, &tag, fallback, &mut *tx).await?;
        }
        tx.commit().await?;
        Ok(ResponseBuilder::no_content().empty().into())
    }
}
impl NpmRegistryExt for NPMHostedRegistry {}
impl RepositoryExt for NPMHostedRegistry {}
impl Repository for NPMHostedRegistry {
    type Error = NPMRegistryError;
    fn get_storage(&self) -> DynStorage {
        self.0.storage.clone()
    }
    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn get_type(&self) -> &'static str {
        "npm"
    }
    fn full_type(&self) -> &'static str {
        "npm/hosted"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            NPMRegistryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }

    fn name(&self) -> String {
        self.0.repository.name.to_string()
    }

    fn id(&self) -> uuid::Uuid {
        self.id
    }

    fn visibility(&self) -> nr_core::repository::Visibility {
        nr_core::repository::Visibility::Public
    }

    fn is_active(&self) -> bool {
        true
    }
    async fn handle_get(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let path_as_string = request.path.to_string();
        debug!(%path_as_string, "Handling npm GET request");
        if let Some((name, tag)) = parse_tag_path(&path_as_string) {
            return self.handle_dist_tags(request, name, tag).await;
        }
        let get_path = match GetPath::try_from(request.path.clone()) {
            Ok(ok) => ok,
            Err(err) => return Ok(err.into_response().into()),
        };
        match get_path {
            GetPath::GetPackageInfo { name } => {
                let Some(project) = self.get_project_from_key(&name).await? else {
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(format!("Project {} not found in repository", name).into())
                        .into());
                };
                debug!(?project, "Got project");
                let versions =
                    DBProjectVersion::get_all_versions(project.id, self.site.as_ref()).await?;
                let dist_tags = load_tags(project.id, &self.site.database).await?;
                let mut times = HashMap::new();
                times.insert(
                    "created".to_owned(),
                    npm_time::format_date_time(&project.created_at),
                );
                times.insert(
                    "modified".to_owned(),
                    npm_time::format_date_time(&project.updated_at),
                );
                let mut versions_map = HashMap::new();
                let mut newest_version = None;
                for version in versions {
                    times.insert(
                        version.version.clone(),
                        npm_time::format_date_time(&version.created_at),
                    );
                    debug!(version_id = %version.id, version = %version.version, "Loaded npm version");
                    if let Some(extra) = version.extra.0.extra {
                        let extra: PublishVersion = match serde_json::from_value(extra) {
                            Ok(ok) => ok,
                            Err(_err) => {
                                warn!("Invalid NPM Project");
                                continue;
                            }
                        };
                        if newest_version.is_none() {
                            newest_version = Some(version.version.clone());
                        }
                        versions_map.insert(version.version.clone(), extra);
                    } else {
                        warn!(version_id = %version.id, "Invalid npm version metadata");
                    }
                }
                let metadata = package_fields(
                    dist_tags
                        .get("latest")
                        .and_then(|version| versions_map.get(version))
                        .or_else(|| {
                            newest_version
                                .as_ref()
                                .and_then(|version| versions_map.get(version))
                        }),
                );
                let project_response = NpmRegistryPackageResponse {
                    id: project.key.clone(),
                    name: project.key.clone(),
                    metadata,
                    dist_tags,
                    versions: versions_map,
                    time: times,
                };
                debug!(package = %project_response.name, version_count = project_response.versions.len(), "Returning npm package metadata");
                let as_string = serde_json::to_string(&project_response)?;
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "application/json")
                    .body(as_string.into())
                    .into())
            }
            GetPath::VersionInfo { name, version } => {
                let Some(project) = self.get_project_from_key(&name).await? else {
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(format!("Project {} not found in repository", name).into())
                        .into());
                };
                let tags = load_tags(project.id, &self.site.database).await?;
                let version = tags.get(&version).cloned().unwrap_or(version);
                debug!(?project, ?version, "Getting version");
                let Some(version) = self.get_project_version(project.id, &version).await? else {
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(format!("Version {} not found in project {}", version, name).into())
                        .into());
                };
                debug!(version_id = %version.id, version = %version.version, "Loaded npm version");
                if let Some(extra) = version.extra.0.extra {
                    let as_string = serde_json::to_string(&extra)?;
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header(CONTENT_TYPE, "application/json")
                        .body(as_string.into())
                        .into())
                } else {
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body("Invalid NPM Project".into())
                        .into())
                }
            }
            GetPath::GetTar {
                name,
                version,
                file,
            } => {
                let Some(path) = self.resolve_tarball_path(&name, &version, &file).await? else {
                    return Ok(ResponseBuilder::not_found().empty().into());
                };
                Ok(self.storage.open_file(self.id, &path).await?.into())
            }
            _ => Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Not Found".into())
                .into()),
        }
    }

    #[instrument(skip(self, request), fields(repository_id = %self.id))]
    async fn handle_head(
        &self,
        mut request: RepositoryRequest,
    ) -> Result<RepoResponse, NPMRegistryError> {
        if let Ok(GetPath::GetTar {
            name,
            version,
            file,
        }) = GetPath::try_from(request.path.clone())
        {
            let Some(path) = self.resolve_tarball_path(&name, &version, &file).await? else {
                return Ok(ResponseBuilder::not_found().empty().into());
            };
            return Ok(self
                .storage
                .get_file_information(self.id, &path)
                .await?
                .into());
        }
        request.parts.method = http::Method::GET;
        let mut response = self.handle_get(request).await?.into_response_default();
        *response.body_mut() = axum::body::Body::empty();
        Ok(response.into())
    }

    async fn handle_put(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NPMRegistryError> {
        let path_as_string = request.path.to_string();
        debug!(%path_as_string, "Handling npm PUT request");
        if path_as_string.starts_with(r#"-/user/org.couchdb.user:"#) {
            return super::login::couch_db::perform_login(self, request).await;
        } else if path_as_string.eq("-/v1/login") {
            return super::login::web_login::perform_login(self, request).await;
        }
        if let Some((name, tag)) = parse_tag_path(&path_as_string) {
            return self.handle_dist_tags(request, name, tag).await;
        }
        let Some(_user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id, self.site.as_ref())
            .await?
        else {
            info!("No acceptable user authentication provided");
            return Ok(RepoResponse::unauthorized());
        };
        let command_header = match request
            .headers()
            .get(NPM_COMMAND_HEADER)
            .ok_or(InvalidNPMCommand::NoHeaderFound)
            .and_then(NPMCommand::try_from)
        {
            Ok(ok) => ok,
            Err(err) => return Ok(err.into_response().into()),
        };

        match command_header {
            NPMCommand::Publish => self.handle_publish(request).await,
            NPMCommand::Deprecate => self.handle_deprecate(request).await,
        }
    }
    async fn handle_delete(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, NPMRegistryError> {
        if let Some((name, tag)) = parse_tag_path(&request.path.to_string()) {
            return self.handle_dist_tags(request, name, tag).await;
        }
        Err(NPMRegistryError::InvalidGetRequest)
    }
}
