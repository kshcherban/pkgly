use std::sync::Arc;

use axum::body::Body;
use bytes::Bytes;
use futures::stream;
use http::{
    StatusCode,
    header::{CONTENT_TYPE, LOCATION},
};
use nr_core::repository::config::RepositoryConfigType;
use nr_core::storage::StoragePath;
use nr_core::{
    database::entities::{
        project::{DBProject, DBProjectColumn, versions::DBProjectVersion},
        repository::DBRepository,
    },
    repository::{
        Visibility,
        project::{PythonPackageMetadata, VersionData},
    },
    user::permissions::RepositoryActions,
};
use nr_storage::{DynStorage, FileContent, Storage};
use parking_lot::RwLock;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use super::{
    PythonRepositoryError,
    configs::{PythonRepositoryConfig, PythonRepositoryConfigType},
    utils::{PythonPackagePathInfo, html_escape, normalize_package_name},
};
use crate::{
    app::Pkgly,
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest,
        utils::{RepositoryExt, can_read_repository_with_auth},
    },
    utils::ResponseBuilder,
};

use nr_core::database::{
    entities::project::ProjectDBType,
    prelude::{
        DynEncodeType, FilterExpr, QueryTool, SQLOrder, SelectQueryBuilder, TableQuery, TableType,
        WhereableTool,
    },
};
use nr_storage::{FileType, StorageFile};
use serde_json::{from_value, to_value};

#[derive(Debug)]
pub struct PythonRepositoryInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub repository: DBRepository,
    #[allow(dead_code)]
    pub config: PythonRepositoryConfig,
    pub storage: DynStorage,
    pub site: Pkgly,
}

#[derive(Debug, Clone)]
pub struct PythonHosted(pub Arc<PythonRepositoryInner>);

impl PythonHosted {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: PythonRepositoryConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let visibility = RwLock::new(repository.visibility);
        Ok(Self(Arc::new(PythonRepositoryInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility,
            repository,
            config,
            storage,
            site,
        })))
    }

    #[instrument(skip(self, request))]
    async fn handle_upload(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, PythonRepositoryError> {
        let Some(user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id(), self.site().as_ref())
            .await?
        else {
            return Ok(RepoResponse::unauthorized());
        };
        let publisher = user.id;
        let _ = user;
        let content_type = request
            .parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        if let Some(content_type) = content_type {
            if content_type.starts_with("multipart/form-data") {
                return self
                    .handle_multipart_upload(request, publisher, content_type)
                    .await;
            }
        }

        let bytes = request.body.body_as_bytes().await?;
        let info = PythonPackagePathInfo::try_from(&request.path)?;
        info!(path = %request.path, ?info, "Saving Python package");
        self.storage()
            .save_file(self.id(), FileContent::Bytes(bytes), &request.path)
            .await?;

        self.upsert_metadata(Some(publisher), &info).await?;

        Ok(RepoResponse::Other(ResponseBuilder::created().empty()))
    }

    async fn handle_multipart_upload(
        &self,
        request: RepositoryRequest,
        publisher: i32,
        content_type: String,
    ) -> Result<RepoResponse, PythonRepositoryError> {
        let boundary = multer::parse_boundary(&content_type)
            .map_err(|err| PythonRepositoryError::InvalidPath(err.to_string()))?;
        let body = request.body.body_as_bytes().await?;
        let stream = stream::once(async move { Ok::<Bytes, multer::Error>(body) });
        let mut multipart = multer::Multipart::new(stream, boundary);

        let mut package: Option<String> = None;
        let mut version: Option<String> = None;
        let mut filename: Option<String> = None;
        let mut file_bytes: Option<Vec<u8>> = None;

        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|err| PythonRepositoryError::InvalidPath(err.to_string()))?
        {
            match field.name() {
                Some("name") => {
                    package = Some(
                        field
                            .text()
                            .await
                            .map_err(|err| PythonRepositoryError::InvalidPath(err.to_string()))?,
                    );
                }
                Some("version") => {
                    version = Some(
                        field
                            .text()
                            .await
                            .map_err(|err| PythonRepositoryError::InvalidPath(err.to_string()))?,
                    );
                }
                Some("content") => {
                    if filename.is_none() {
                        filename = field.file_name().map(|value| value.to_string());
                    }
                    let bytes = field
                        .bytes()
                        .await
                        .map_err(|err| PythonRepositoryError::InvalidPath(err.to_string()))?;
                    file_bytes = Some(bytes.to_vec());
                }
                _ => {
                    // Ignore other fields
                }
            }
        }

        let package = package.ok_or_else(|| {
            PythonRepositoryError::InvalidPath("missing package name".to_string())
        })?;
        let version = version.ok_or_else(|| {
            PythonRepositoryError::InvalidPath("missing package version".to_string())
        })?;
        let filename = filename.ok_or_else(|| {
            PythonRepositoryError::InvalidPath("missing package filename".to_string())
        })?;
        let file_bytes = file_bytes.ok_or_else(|| {
            PythonRepositoryError::InvalidPath("missing package content".to_string())
        })?;

        let storage_path = StoragePath::from(format!("{}/{}/{}", package, version, filename));
        let info = PythonPackagePathInfo::try_from(&storage_path)?;

        self.storage()
            .save_file(self.id(), FileContent::Content(file_bytes), &storage_path)
            .await?;

        self.upsert_metadata(Some(publisher), &info).await?;

        Ok(RepoResponse::Other(ResponseBuilder::created().empty()))
    }

    #[instrument(skip(self, info))]
    pub(crate) async fn upsert_metadata(
        &self,
        publisher: Option<i32>,
        info: &PythonPackagePathInfo,
    ) -> Result<(), PythonRepositoryError> {
        let project_key = info.project_key();
        let project = if let Some(project) =
            DBProject::find_by_project_key(&project_key, self.id(), self.site().as_ref()).await?
        {
            project
        } else {
            let new_project = nr_core::database::entities::project::NewProject {
                scope: None,
                project_key: project_key.clone(),
                name: info.package.clone(),
                description: None,
                repository: self.id(),
                storage_path: info.project_storage_path(),
            };
            new_project.insert(self.site().as_ref()).await?
        };

        if DBProjectVersion::find_by_version_and_project(
            &info.version,
            project.id,
            &self.site().database,
        )
        .await?
        .is_some()
        {
            debug!(
                project_id = %project.id,
                version = %info.version,
                "Python version already exists"
            );
            return Ok(());
        }

        let metadata = PythonPackageMetadata {
            filename: info.file_name.clone(),
            normalized_name: Some(normalize_package_name(&info.package)),
            ..Default::default()
        };

        let new_version = nr_core::database::entities::project::versions::NewVersion {
            project_id: project.id,
            repository_id: self.id(),
            version: info.version.clone(),
            release_type: info.release_type(),
            version_path: info.version_storage_path(),
            publisher,
            version_page: None,
            extra: VersionData {
                extra: Some(to_value(metadata)?),
                ..Default::default()
            },
        };
        new_version.insert(&self.site().database).await?;
        Ok(())
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
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
}

impl RepositoryExt for PythonHosted {}

impl Repository for PythonHosted {
    type Error = PythonRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "python"
    }

    fn full_type(&self) -> &'static str {
        "python/hosted"
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
        self.0.repository.active
    }

    fn site(&self) -> Pkgly {
        self.site()
    }

    fn handle_get(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let visibility = self.visibility();
        let site = self.site();
        let storage = self.storage();
        let repository_id = self.id();
        let this = self.clone();
        async move {
            let can_read = if request.authentication.is_virtual_repository() {
                true
            } else {
                can_read_repository_with_auth(
                    &request.authentication,
                    visibility,
                    repository_id,
                    site.as_ref(),
                    &request.auth_config,
                )
                .await?
            };
            if !can_read {
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::UNAUTHORIZED,
                    "Missing permission to read repository",
                ));
            }

            let uri_path = request.parts.uri.path().to_string();
            let path_clone = request.path.clone();
            let ctx = PythonSimpleRequestContext::new(&path_clone, &uri_path);

            if ctx.is_directory {
                if ctx.redirect_needed {
                    return Ok(RepoResponse::Other(redirect_to_trailing_slash(&uri_path)));
                }
                if let Some(response) = storage_request_directory(&this, &ctx).await? {
                    return Ok(RepoResponse::Other(response));
                }
            }

            let file = storage.open_file(repository_id, &request.path).await?;
            Ok(file.into())
        }
    }

    fn handle_put(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_upload(request).await }
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let visibility = self.visibility();
        let site = self.site();
        let storage = self.storage();
        let repository_id = self.id();
        let this = self.clone();
        async move {
            let can_read = if request.authentication.is_virtual_repository() {
                true
            } else {
                can_read_repository_with_auth(
                    &request.authentication,
                    visibility,
                    repository_id,
                    site.as_ref(),
                    &request.auth_config,
                )
                .await?
            };
            if !can_read {
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::UNAUTHORIZED,
                    "Missing permission to read repository",
                ));
            }

            let uri_path = request.parts.uri.path().to_string();
            let path_clone = request.path.clone();
            let ctx = PythonSimpleRequestContext::new(&path_clone, &uri_path);

            if ctx.is_directory {
                if ctx.redirect_needed {
                    return Ok(RepoResponse::Other(redirect_to_trailing_slash(&uri_path)));
                }
                if let Some(response) = storage_request_directory(&this, &ctx).await? {
                    let response = response.map(|_| Body::empty());
                    return Ok(RepoResponse::Other(response));
                }
            }

            let meta = storage
                .get_file_information(repository_id, &request.path)
                .await?;
            Ok(meta.into())
        }
    }

    fn handle_post(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_upload(request).await }
    }
}

#[derive(Debug, Clone)]
struct PythonSimpleRequestContext {
    components: Vec<String>,
    is_directory: bool,
    redirect_needed: bool,
    prefix_to_root: String,
}

impl PythonSimpleRequestContext {
    fn new(path: &StoragePath, uri_path: &str) -> Self {
        let mut components: Vec<String> = path.clone().into_iter().map(String::from).collect();

        if let Some(first) = components.first() {
            if first.eq_ignore_ascii_case("simple") {
                components.remove(0);
            }
        }

        let request_segments = path.clone().into_iter().count();
        let mut prefix_to_root = String::new();
        for _ in 0..request_segments {
            prefix_to_root.push_str("../");
        }

        let trailing_slash = uri_path.ends_with('/');
        let is_directory =
            path.is_directory() || trailing_slash || components.is_empty() || components.len() == 1;

        let redirect_needed = is_directory && !trailing_slash;

        Self {
            components,
            is_directory,
            redirect_needed,
            prefix_to_root,
        }
    }
}

async fn storage_request_directory(
    repository: &PythonHosted,
    ctx: &PythonSimpleRequestContext,
) -> Result<Option<axum::response::Response>, PythonRepositoryError> {
    if ctx.components.is_empty() {
        return repository.render_root_simple_index(ctx).await.map(Some);
    }

    if ctx.components.len() == 1 {
        return repository
            .render_package_simple_index(ctx, ctx.components[0].as_str())
            .await
            .map(Some);
    }

    Ok(None)
}

async fn list_projects(repository: &PythonHosted) -> Result<Vec<DBProject>, PythonRepositoryError> {
    let projects = SelectQueryBuilder::with_columns(DBProject::table_name(), DBProject::columns())
        .filter(DBProjectColumn::RepositoryId.equals(repository.id().value()))
        .order_by(DBProjectColumn::Key, SQLOrder::Ascending)
        .query_as()
        .fetch_all(repository.site().as_ref())
        .await?;

    Ok(projects)
}

impl PythonHosted {
    async fn render_root_simple_index(
        &self,
        ctx: &PythonSimpleRequestContext,
    ) -> Result<axum::response::Response, PythonRepositoryError> {
        let projects = list_projects(self).await?;

        let mut body = String::from(
            "<!DOCTYPE html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Simple index</title>\n  </head>\n  <body>\n",
        );

        if projects.is_empty() {
            body.push_str("    <p>No packages uploaded yet.</p>\n");
        } else {
            for project in projects {
                let href = format!("{}{}", ctx.prefix_to_root, project.path);
                body.push_str("    <a href=\"");
                body.push_str(&html_escape(&href));
                body.push_str("\">");
                body.push_str(&html_escape(&project.key));
                body.push_str("</a><br/>\n");
            }
        }

        body.push_str("  </body>\n</html>\n");

        Ok(ResponseBuilder::ok().html(body))
    }

    async fn render_package_simple_index(
        &self,
        ctx: &PythonSimpleRequestContext,
        package_component: &str,
    ) -> Result<axum::response::Response, PythonRepositoryError> {
        let normalized = normalize_package_name(package_component);
        let Some(project) =
            DBProject::find_by_project_key(&normalized, self.id(), self.site().as_ref()).await?
        else {
            let message = format!(
                "<!DOCTYPE html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Package not found</title>\n  </head>\n  <body>\n    <p>Package {} not found.</p>\n  </body>\n</html>\n",
                html_escape(package_component)
            );
            return Ok(ResponseBuilder::not_found().html(message));
        };

        let mut versions =
            DBProjectVersion::get_all_versions(project.id, self.site().as_ref()).await?;
        versions.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let project_dir = project.path.trim_end_matches('/');

        let mut lines = Vec::new();

        for version in versions {
            let version_path = format!("{project_dir}/{}/", version.version);
            let storage_path = StoragePath::from(version_path.as_str());
            let metadata = version
                .extra
                .extra
                .as_ref()
                .and_then(|value| from_value::<PythonPackageMetadata>(value.clone()).ok());
            let requires_python = metadata
                .as_ref()
                .and_then(|meta| meta.requires_python.as_deref());
            let metadata_hash = metadata.as_ref().and_then(|meta| meta.sha256.as_deref());

            if let Some(StorageFile::Directory { files, .. }) =
                self.storage().open_file(self.id(), &storage_path).await?
            {
                let mut file_entries = files
                    .iter()
                    .filter_map(|entry| match entry.file_type() {
                        FileType::File(file_meta) => Some((entry.name().to_string(), file_meta)),
                        FileType::Directory(_) => None,
                    })
                    .collect::<Vec<_>>();

                file_entries.sort_by(|a, b| a.0.cmp(&b.0));

                for (file_name, file_meta) in file_entries {
                    if should_ignore(&file_name) {
                        continue;
                    }
                    let relative_path = format!("{project_dir}/{}/{file_name}", version.version);
                    let mut href = format!("{}{}", ctx.prefix_to_root, relative_path);
                    if let Some(hash) = file_meta.file_hash.sha2_256.as_deref().or(metadata_hash) {
                        href.push_str("#sha256=");
                        // Convert base64-encoded hash to hex format for PEP 503 compliance
                        if let Some(hex_hash) = base64_to_hex(hash) {
                            href.push_str(&hex_hash);
                        } else {
                            // Fallback to original hash if conversion fails
                            href.push_str(hash);
                        }
                    }

                    let mut line = String::from("    <a href=\"");
                    line.push_str(&html_escape(&href));
                    line.push('"');
                    if let Some(rp) = requires_python {
                        line.push_str(" data-requires-python=\"");
                        line.push_str(&html_escape(rp));
                        line.push_str("\"");
                    }
                    line.push('>');
                    line.push_str(&html_escape(&file_name));
                    line.push_str("</a><br/>\n");
                    lines.push(line);
                }
            }
        }

        let display_name = if project.name.is_empty() {
            package_component
        } else {
            project.name.as_str()
        };

        let mut body = format!(
            "<!DOCTYPE html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Links for {}</title>\n  </head>\n  <body>\n    <h1>Links for {}</h1>\n",
            html_escape(display_name),
            html_escape(display_name)
        );

        if lines.is_empty() {
            body.push_str("    <p>No files available.</p>\n");
        } else {
            for line in lines {
                body.push_str(&line);
            }
        }

        body.push_str("  </body>\n</html>\n");

        Ok(ResponseBuilder::ok().html(body))
    }
}

fn redirect_to_trailing_slash(uri_path: &str) -> axum::response::Response {
    let mut location = if uri_path.is_empty() {
        String::from("/")
    } else if uri_path.ends_with('/') {
        uri_path.to_string()
    } else {
        format!("{}/", uri_path)
    };

    if location.is_empty() {
        location = String::from("/");
    }

    ResponseBuilder::default()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(LOCATION, location)
        .empty()
}

fn should_ignore(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".nr-meta")
}

/// Converts a base64-encoded hash to lowercase hex format for PEP 503 compliance
fn base64_to_hex(base64_hash: &str) -> Option<String> {
    use nr_core::utils::base64_utils;

    // Decode base64 to bytes
    let bytes = base64_utils::decode(base64_hash).ok()?;

    // Convert bytes to lowercase hex string
    Some(
        bytes
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect::<String>(),
    )
}

#[cfg(test)]
mod tests;
