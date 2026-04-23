use ahash::{HashMap, HashMapExt};
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};

use axum::http::header::CONTENT_TYPE;
use bytes::Bytes;
use chrono::{SecondsFormat, Utc};
use futures::stream;
use nr_core::{
    database::entities::{
        project::{
            DBProject, NewProject, ProjectDBType,
            versions::{DBProjectVersion, NewVersion},
        },
        repository::DBRepository,
    },
    repository::Visibility,
};
use nr_storage::{DynStorage, Storage};
use parking_lot::RwLock;
use serde_json::{Value, json};
use tracing::{debug, warn};
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::{
    GoModuleError, GoModulePath, GoVersion,
    configs::GoRepositoryConfig,
    utils::{GoModuleRequest, GoRequestType},
};

use crate::{
    app::{
        Pkgly,
        webhooks::{self, PackageWebhookActor, WebhookEventType},
    },
    repository::{
        RepoResponse, Repository, RepositoryFactoryError, RepositoryRequest,
        utils::can_read_repository_with_auth,
    },
    utils::ResponseBuilder,
};

use super::{
    ext::{GoFileType, GoRepositoryExt},
    utils::{generate_go_mod, generate_go_module_info},
};
use crate::repository::{RepositoryAuthConfigType, utils::RepositoryExt};
use nr_core::repository::config::RepositoryConfigType;
use nr_core::repository::project::{ReleaseType, VersionData};
use nr_core::user::permissions::RepositoryActions;

#[derive(Debug)]
pub struct GoHostedInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub active: bool,
    pub storage_name: String,
}

#[derive(Debug, Clone)]
pub struct GoHosted(pub Arc<GoHostedInner>);

impl GoHosted {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        _config: GoRepositoryConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        let storage_name = storage.storage_config().storage_config.storage_name.clone();

        Ok(Self(Arc::new(GoHostedInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            storage,
            site,
            active: repository.active,
            storage_name,
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

    #[allow(clippy::too_many_lines)]
    async fn handle_athens_upload(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, crate::repository::RepositoryHandlerError> {
        let Some(user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id(), self.site().as_ref())
            .await?
        else {
            return Ok(RepoResponse::unauthorized());
        };
        let publisher_id = user.id;

        let content_type = request
            .parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                crate::repository::RepositoryHandlerError::Other(Box::new(
                    crate::utils::bad_request::BadRequestErrors::Other(
                        "Missing Content-Type header".to_string(),
                    ),
                ))
            })?;

        if !content_type.starts_with("multipart/form-data") {
            return Ok(RepoResponse::basic_text_response(
                http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Expected multipart/form-data payload",
            ));
        }

        let boundary = multer::parse_boundary(content_type).map_err(|err| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Invalid multipart boundary: {}",
                    err
                )),
            ))
        })?;

        let body = request.body.body_as_bytes().await?;
        let stream = stream::once(async move { Ok::<Bytes, multer::Error>(Bytes::from(body)) });
        let mut multipart = multer::Multipart::new(stream, boundary);

        let mut module_bytes: Option<Vec<u8>> = None;
        let mut raw_version: Option<String> = None;
        let mut raw_module_name: Option<String> = None;
        let mut info_bytes: Option<Vec<u8>> = None;
        let mut go_mod_bytes: Option<Vec<u8>> = None;

        while let Some(field) = multipart.next_field().await.map_err(|err| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(format!(
                    "Multipart parsing error: {}",
                    err
                )),
            ))
        })? {
            match field.name() {
                Some("module") => {
                    module_bytes = Some(
                        field
                            .bytes()
                            .await
                            .map_err(|err| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Failed reading module payload: {}",
                                        err
                                    )),
                                ))
                            })?
                            .to_vec(),
                    );
                }
                Some("version") => {
                    raw_version = Some(
                        field
                            .text()
                            .await
                            .map_err(|err| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Invalid version field: {}",
                                        err
                                    )),
                                ))
                            })?
                            .trim()
                            .to_string(),
                    );
                }
                Some("module_name") => {
                    raw_module_name = Some(
                        field
                            .text()
                            .await
                            .map_err(|err| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Invalid module_name field: {}",
                                        err
                                    )),
                                ))
                            })?
                            .trim()
                            .to_string(),
                    );
                }
                Some("info") => {
                    info_bytes = Some(
                        field
                            .bytes()
                            .await
                            .map_err(|err| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Failed reading info payload: {}",
                                        err
                                    )),
                                ))
                            })?
                            .to_vec(),
                    );
                }
                Some("gomod") => {
                    go_mod_bytes = Some(
                        field
                            .bytes()
                            .await
                            .map_err(|err| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Failed reading go.mod payload: {}",
                                        err
                                    )),
                                ))
                            })?
                            .to_vec(),
                    );
                }
                _ => {}
            }
        }

        let module_bytes = module_bytes.ok_or_else(|| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(
                    "Missing module archive in upload".to_string(),
                ),
            ))
        })?;
        let raw_version = raw_version.ok_or_else(|| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(
                    "Missing version field in upload".to_string(),
                ),
            ))
        })?;
        let raw_module_name = raw_module_name.ok_or_else(|| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                crate::utils::bad_request::BadRequestErrors::Other(
                    "Missing module_name field in upload".to_string(),
                ),
            ))
        })?;

        let module_path = GoModulePath::new(raw_module_name)
            .map_err(|err| crate::repository::RepositoryHandlerError::Other(Box::new(err)))?;
        let version = GoVersion::new(raw_version)
            .map_err(|err| crate::repository::RepositoryHandlerError::Other(Box::new(err)))?;

        let info_bytes = match info_bytes {
            Some(bytes) => {
                let value: Value = serde_json::from_slice(&bytes).map_err(|err| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(format!(
                            "Invalid info JSON: {}",
                            err
                        )),
                    ))
                })?;
                if let Some(info_version) = value.get("Version").and_then(Value::as_str) {
                    if info_version != version.as_str() {
                        return Ok(RepoResponse::basic_text_response(
                            http::StatusCode::BAD_REQUEST,
                            format!(
                                "Version mismatch: info file declares {}, expected {}",
                                info_version,
                                version.as_str()
                            ),
                        ));
                    }
                }
                bytes
            }
            None => serde_json::to_vec(&json!({
                "Version": version.as_str(),
                "Time": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
            }))
            .map_err(|err| {
                crate::repository::RepositoryHandlerError::Other(Box::new(
                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                        "Failed to generate info JSON: {}",
                        err
                    )),
                ))
            })?,
        };

        let go_mod_bytes = match go_mod_bytes {
            Some(bytes) => bytes,
            None => {
                let mut archive = ZipArchive::new(Cursor::new(&module_bytes)).map_err(|err| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(format!(
                            "Invalid module zip archive: {}",
                            err
                        )),
                    ))
                })?;
                let mut extracted = Vec::new();
                let mut found = false;
                for idx in 0..archive.len() {
                    let mut file = archive.by_index(idx).map_err(|err| {
                        crate::repository::RepositoryHandlerError::Other(Box::new(
                            crate::utils::bad_request::BadRequestErrors::Other(format!(
                                "Failed to read module archive: {}",
                                err
                            )),
                        ))
                    })?;
                    if file.name().ends_with("go.mod") {
                        file.read_to_end(&mut extracted).map_err(|err| {
                            crate::repository::RepositoryHandlerError::Other(Box::new(
                                crate::utils::bad_request::BadRequestErrors::Other(format!(
                                    "Failed to extract go.mod: {}",
                                    err
                                )),
                            ))
                        })?;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(RepoResponse::basic_text_response(
                        http::StatusCode::BAD_REQUEST,
                        "Uploaded module zip is missing go.mod",
                    ));
                }
                extracted
            }
        };

        self.save_go_module_file(
            &module_path,
            &version,
            GoFileType::Zip,
            module_bytes.clone(),
        )
        .await?;
        let canonical_zip =
            self.canonicalize_module_zip(&module_path, &version, &module_bytes, &go_mod_bytes)?;
        self.save_go_module_file(&module_path, &version, GoFileType::Zip, canonical_zip)
            .await?;
        self.save_go_module_file(
            &module_path,
            &version,
            GoFileType::GoMod,
            go_mod_bytes.clone(),
        )
        .await?;
        self.save_go_module_file(&module_path, &version, GoFileType::Info, info_bytes)
            .await?;

        self.ensure_version_list(&module_path, &version).await?;
        self.record_go_catalog_entry(&module_path, &version, GoFileType::Zip, publisher_id)
            .await?;
        if let Err(err) = webhooks::enqueue_package_path_event(
            &self.site(),
            self.id(),
            WebhookEventType::PackagePublished,
            go_cache_path(&module_path, &version, GoFileType::Zip),
            PackageWebhookActor {
                user_id: Some(publisher_id),
                username: None,
            },
            false,
        )
        .await
        {
            warn!(error = %err, "Failed to enqueue Go module publish webhook");
        }
        if let Err(err) = self.refresh_latest_aliases(&module_path).await {
            warn!(
                module = %module_path.as_str(),
                version = %version.as_str(),
                ?err,
                "Failed to refresh latest aliases after module upload"
            );
        }

        let response = ResponseBuilder::created().json(&json!({
            "module": module_path.as_str(),
            "version": version.as_str()
        }));
        Ok(RepoResponse::Other(response))
    }

    async fn record_go_catalog_entry(
        &self,
        module_path: &GoModulePath,
        version: &GoVersion,
        file_type: GoFileType,
        publisher_id: i32,
    ) -> Result<(), crate::repository::RepositoryHandlerError> {
        let repository_id = self.id();
        let database = &self.site().database;
        let project_key = module_path.as_str().to_string();
        let storage_path = format!("{}/", module_path.as_str());

        let project =
            match DBProject::find_by_project_key(&project_key, repository_id, database).await? {
                Some(existing) => existing,
                None => {
                    NewProject {
                        scope: None,
                        project_key: project_key.clone(),
                        name: project_key.clone(),
                        description: None,
                        repository: repository_id,
                        storage_path,
                    }
                    .insert(database)
                    .await?
                }
            };

        let new_priority = go_file_priority(file_type.clone());
        if let Some(existing) =
            DBProjectVersion::find_by_version_and_project(version.as_str(), project.id, database)
                .await?
        {
            let existing_priority = go_path_priority(&existing.path);
            if existing_priority >= new_priority {
                return Ok(());
            }
            sqlx::query("DELETE FROM project_versions WHERE id = $1")
                .bind(existing.id)
                .execute(database)
                .await?;
        }

        let version_path = go_cache_path(module_path, version, file_type);
        let new_version = NewVersion {
            project_id: project.id,
            repository_id,
            version: version.as_str().to_string(),
            release_type: ReleaseType::release_type_from_version(version.as_str()),
            version_path,
            publisher: Some(publisher_id),
            version_page: None,
            extra: VersionData::default(),
        };
        new_version.insert(database).await?;
        Ok(())
    }

    fn canonicalize_module_zip(
        &self,
        module_path: &GoModulePath,
        version: &GoVersion,
        archive_bytes: &[u8],
        go_mod_bytes: &[u8],
    ) -> Result<Vec<u8>, crate::repository::RepositoryHandlerError> {
        let mut archive = ZipArchive::new(Cursor::new(archive_bytes)).map_err(|err| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                GoModuleError::InvalidRequest(format!("Invalid module archive: {}", err)),
            ))
        })?;

        let mut components_list: Vec<Vec<String>> = Vec::new();
        let mut component_map: HashMap<String, Vec<String>> = HashMap::new();

        for idx in 0..archive.len() {
            let file = archive.by_index(idx).map_err(|err| {
                crate::repository::RepositoryHandlerError::Other(Box::new(
                    GoModuleError::InvalidRequest(format!(
                        "Failed to read module archive: {}",
                        err
                    )),
                ))
            })?;
            if file.is_dir() {
                continue;
            }
            let normalized = file.name().replace('\\', "/");
            let trimmed = normalized.trim_matches('/');
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("../") || trimmed.contains("/../") || trimmed.starts_with('/') {
                return Err(crate::repository::RepositoryHandlerError::Other(Box::new(
                    GoModuleError::InvalidRequest(format!(
                        "Illegal path `{}` in module archive",
                        file.name()
                    )),
                )));
            }
            let components: Vec<String> = trimmed.split('/').map(|s| s.to_string()).collect();
            if components.is_empty() {
                continue;
            }
            components_list.push(components.clone());
            component_map.insert(trimmed.to_string(), components);
        }

        let prefix = longest_common_prefix(&components_list);
        let root_prefix = format!("{}@{}/", module_path.as_str(), version.as_str());
        let mut normalized_files: HashMap<String, Vec<u8>> = HashMap::new();

        let mut archive = ZipArchive::new(Cursor::new(archive_bytes)).map_err(|err| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                GoModuleError::InvalidRequest(format!("Invalid module archive: {}", err)),
            ))
        })?;

        for idx in 0..archive.len() {
            let mut file = archive.by_index(idx).map_err(|err| {
                crate::repository::RepositoryHandlerError::Other(Box::new(
                    GoModuleError::InvalidRequest(format!(
                        "Failed to read module archive: {}",
                        err
                    )),
                ))
            })?;
            if file.is_dir() {
                continue;
            }
            let normalized = file.name().replace('\\', "/");
            let trimmed = normalized.trim_matches('/');
            if trimmed.is_empty() {
                continue;
            }
            let components = component_map
                .get(trimmed)
                .cloned()
                .unwrap_or_else(|| trimmed.split('/').map(|s| s.to_string()).collect());
            let skip = prefix.len().min(components.len());
            let relative_components = components.into_iter().skip(skip).collect::<Vec<_>>();
            if relative_components.is_empty() {
                continue;
            }
            let relative_path = relative_components.join("/");
            let canonical_path = format!("{}{}", root_prefix, relative_path);
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(|err| {
                crate::repository::RepositoryHandlerError::Other(Box::new(
                    GoModuleError::InvalidRequest(format!(
                        "Failed to extract `{}`: {}",
                        file.name(),
                        err
                    )),
                ))
            })?;
            normalized_files
                .entry(canonical_path)
                .or_insert_with(|| bytes.to_vec());
        }

        normalized_files.insert(format!("{}go.mod", root_prefix), go_mod_bytes.to_vec());

        let mut files: Vec<_> = normalized_files.into_iter().collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));

        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);

        for (path, data) in files {
            writer.start_file(path, options).map_err(|err| {
                crate::repository::RepositoryHandlerError::Other(Box::new(
                    GoModuleError::InvalidRequest(format!(
                        "Failed to write module archive entry: {}",
                        err
                    )),
                ))
            })?;
            writer.write_all(&data).map_err(|err| {
                crate::repository::RepositoryHandlerError::Other(Box::new(
                    GoModuleError::InvalidRequest(format!(
                        "Failed to write module archive data: {}",
                        err
                    )),
                ))
            })?;
        }

        let cursor = writer.finish().map_err(|err| {
            crate::repository::RepositoryHandlerError::Other(Box::new(
                GoModuleError::InvalidRequest(format!(
                    "Failed to finalize module archive: {}",
                    err
                )),
            ))
        })?;
        Ok(cursor.into_inner())
    }
}

const fn go_file_priority(file_type: GoFileType) -> u8 {
    match file_type {
        GoFileType::Zip => 3,
        GoFileType::GoMod => 2,
        GoFileType::Info => 1,
        GoFileType::GoModWithoutVersion => 0,
    }
}

fn go_path_priority(path: &str) -> u8 {
    if path.ends_with(".zip") {
        return 3;
    }
    if path.ends_with(".mod") {
        return 2;
    }
    if path.ends_with(".info") {
        return 1;
    }
    0
}

fn go_cache_path(module_path: &GoModulePath, version: &GoVersion, file_type: GoFileType) -> String {
    match file_type {
        GoFileType::Zip => format!("{}/@v/{}.zip", module_path.as_str(), version.as_str()),
        GoFileType::GoMod => format!("{}/@v/{}.mod", module_path.as_str(), version.as_str()),
        GoFileType::Info => format!("{}/@v/{}.info", module_path.as_str(), version.as_str()),
        GoFileType::GoModWithoutVersion => format!("{}/go.mod", module_path.as_str()),
    }
}

impl RepositoryExt for GoHosted {}

impl Repository for GoHosted {
    type Error = crate::repository::RepositoryHandlerError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "go"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            super::configs::GoRepositoryConfigType::get_type_static(),
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

    fn handle_post<'a>(
        &'a self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            if request.path.to_string() == "upload" {
                return this.handle_athens_upload(request).await;
            }
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                this.full_type(),
            ))
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
                "Handling Go hosted request: {:?} for module: {}",
                module_request.request_type,
                module_request.module_path.as_str()
            );

            match module_request.request_type {
                GoRequestType::ListVersions => {
                    // Get actual versions from database
                    match this
                        .list_go_module_versions(&module_request.module_path)
                        .await
                    {
                        Ok(versions) => {
                            if versions.is_empty() {
                                Ok(RepoResponse::basic_text_response(
                                    http::StatusCode::NOT_FOUND,
                                    "Module not found",
                                ))
                            } else {
                                let version_list = versions.join("\n");
                                Ok(RepoResponse::basic_text_response(
                                    http::StatusCode::OK,
                                    format!("{}\n", version_list),
                                ))
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                GoRequestType::VersionInfo => {
                    let version = module_request.version.as_ref().ok_or_else(|| {
                        crate::repository::RepositoryHandlerError::Other(Box::new(
                            crate::utils::bad_request::BadRequestErrors::Other(
                                "Version required for version info request".to_string(),
                            ),
                        ))
                    })?;

                    // Try to load from storage first
                    match this
                        .load_go_module_file(
                            &module_request.module_path,
                            Some(version),
                            GoFileType::Info,
                        )
                        .await
                    {
                        Ok(Some(content)) => {
                            let content_str = String::from_utf8(content).map_err(|e| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Invalid UTF-8 in info file: {}",
                                        e
                                    )),
                                ))
                            })?;
                            Ok(RepoResponse::basic_text_response(
                                http::StatusCode::OK,
                                content_str,
                            ))
                        }
                        Ok(None) => {
                            // Generate info file if not found
                            let project = this
                                .get_project_from_key(module_request.module_path.as_str())
                                .await?;
                            if let Some(project) = project {
                                let info_json = generate_go_module_info(
                                    &module_request.module_path,
                                    version,
                                    project.created_at.into(),
                                )?;
                                Ok(RepoResponse::basic_text_response(
                                    http::StatusCode::OK,
                                    info_json,
                                ))
                            } else {
                                Ok(RepoResponse::basic_text_response(
                                    http::StatusCode::NOT_FOUND,
                                    "Module not found",
                                ))
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                GoRequestType::GoMod => {
                    let version = module_request.version.as_ref().ok_or_else(|| {
                        crate::repository::RepositoryHandlerError::Other(Box::new(
                            crate::utils::bad_request::BadRequestErrors::Other(
                                "Version required for go.mod request".to_string(),
                            ),
                        ))
                    })?;

                    // Try to load from storage first
                    match this
                        .load_go_module_file(
                            &module_request.module_path,
                            Some(version),
                            GoFileType::GoMod,
                        )
                        .await
                    {
                        Ok(Some(content)) => {
                            let content_str = String::from_utf8(content).map_err(|e| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Invalid UTF-8 in go.mod file: {}",
                                        e
                                    )),
                                ))
                            })?;
                            Ok(RepoResponse::basic_text_response(
                                http::StatusCode::OK,
                                content_str,
                            ))
                        }
                        Ok(None) => {
                            // Generate basic go.mod if not found
                            let go_mod_content =
                                generate_go_mod(module_request.module_path.as_str());
                            Ok(RepoResponse::basic_text_response(
                                http::StatusCode::OK,
                                go_mod_content,
                            ))
                        }
                        Err(e) => Err(e),
                    }
                }
                GoRequestType::ModuleZip => {
                    let version = module_request.version.as_ref().ok_or_else(|| {
                        crate::repository::RepositoryHandlerError::Other(Box::new(
                            crate::utils::bad_request::BadRequestErrors::Other(
                                "Version required for module zip request".to_string(),
                            ),
                        ))
                    })?;

                    // Try to load from storage
                    match this
                        .load_go_module_file(
                            &module_request.module_path,
                            Some(version),
                            GoFileType::Zip,
                        )
                        .await
                    {
                        Ok(Some(content)) => {
                            // Return binary response for zip files
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
                        Ok(None) => Ok(RepoResponse::basic_text_response(
                            http::StatusCode::NOT_FOUND,
                            "Module zip not found - use 'go publish' to upload modules",
                        )),
                        Err(e) => Err(e),
                    }
                }
                GoRequestType::Latest => {
                    // Get the latest version
                    match this
                        .get_latest_go_version(&module_request.module_path)
                        .await
                    {
                        Ok(Some(latest_version)) => {
                            // Generate info for latest version
                            let project = this
                                .get_project_from_key(module_request.module_path.as_str())
                                .await?;
                            if let Some(project) = project {
                                let info_json = generate_go_module_info(
                                    &module_request.module_path,
                                    &latest_version,
                                    project.created_at.into(),
                                )?;
                                Ok(RepoResponse::basic_text_response(
                                    http::StatusCode::OK,
                                    info_json,
                                ))
                            } else {
                                Ok(RepoResponse::basic_text_response(
                                    http::StatusCode::NOT_FOUND,
                                    "Module not found",
                                ))
                            }
                        }
                        Ok(None) => Ok(RepoResponse::basic_text_response(
                            http::StatusCode::NOT_FOUND,
                            "No versions found for module",
                        )),
                        Err(e) => Err(e),
                    }
                }
                GoRequestType::GoModWithoutVersion => {
                    // Try to load go.mod without version, otherwise use latest version's go.mod
                    match this
                        .load_go_module_file(
                            &module_request.module_path,
                            None,
                            GoFileType::GoModWithoutVersion,
                        )
                        .await
                    {
                        Ok(Some(content)) => {
                            let content_str = String::from_utf8(content).map_err(|e| {
                                crate::repository::RepositoryHandlerError::Other(Box::new(
                                    crate::utils::bad_request::BadRequestErrors::Other(format!(
                                        "Invalid UTF-8 in go.mod file: {}",
                                        e
                                    )),
                                ))
                            })?;
                            Ok(RepoResponse::basic_text_response(
                                http::StatusCode::OK,
                                content_str,
                            ))
                        }
                        Ok(None) => {
                            // Fall back to latest version's go.mod
                            match this
                                .get_latest_go_version(&module_request.module_path)
                                .await
                            {
                                Ok(Some(latest_version)) => {
                                    match this
                                        .load_go_module_file(
                                            &module_request.module_path,
                                            Some(&latest_version),
                                            GoFileType::GoMod,
                                        )
                                        .await
                                    {
                                        Ok(Some(content)) => {
                                            let content_str = String::from_utf8(content)
                                                .map_err(|e| crate::repository::RepositoryHandlerError::Other(Box::new(crate::utils::bad_request::BadRequestErrors::Other(
                                                        format!("Invalid UTF-8 in go.mod file: {}", e)
                                                    ))))?;
                                            Ok(RepoResponse::basic_text_response(
                                                http::StatusCode::OK,
                                                content_str,
                                            ))
                                        }
                                        _ => {
                                            // Generate basic go.mod as last resort
                                            let go_mod_content = generate_go_mod(
                                                module_request.module_path.as_str(),
                                            );
                                            Ok(RepoResponse::basic_text_response(
                                                http::StatusCode::OK,
                                                go_mod_content,
                                            ))
                                        }
                                    }
                                }
                                _ => {
                                    // Generate basic go.mod as last resort
                                    let go_mod_content =
                                        generate_go_mod(module_request.module_path.as_str());
                                    Ok(RepoResponse::basic_text_response(
                                        http::StatusCode::OK,
                                        go_mod_content,
                                    ))
                                }
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                GoRequestType::SumdbSupported => {
                    Ok(RepoResponse::basic_text_response(
                        http::StatusCode::OK,
                        "false", // Hosted repositories don't support sumdb
                    ))
                }
                GoRequestType::SumdbLookup | GoRequestType::SumdbTile => {
                    Ok(RepoResponse::basic_text_response(
                        http::StatusCode::NOT_IMPLEMENTED,
                        "sumdb not supported in hosted repositories",
                    ))
                }
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

    fn handle_put<'a>(
        &'a self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            let Some(user) = request
                .authentication
                .get_user_if_has_action(RepositoryActions::Write, this.id(), this.site().as_ref())
                .await?
            else {
                return Ok(RepoResponse::unauthorized());
            };
            let publisher_id = user.id;

            let module_request =
                GoModuleRequest::from_path(&request.path.to_string()).map_err(|err| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(format!(
                            "Invalid Go module path: {}",
                            err
                        )),
                    ))
                })?;

            let module_path = module_request.module_path.clone();
            let version = module_request.version.clone();

            let (file_type, version) = match module_request.request_type {
                GoRequestType::VersionInfo => {
                    let version = version.ok_or_else(|| {
                        crate::repository::RepositoryHandlerError::Other(Box::new(
                            crate::utils::bad_request::BadRequestErrors::Other(
                                "Version required for version info upload".to_string(),
                            ),
                        ))
                    })?;
                    (GoFileType::Info, version)
                }
                GoRequestType::GoMod => {
                    let version = version.ok_or_else(|| {
                        crate::repository::RepositoryHandlerError::Other(Box::new(
                            crate::utils::bad_request::BadRequestErrors::Other(
                                "Version required for go.mod upload".to_string(),
                            ),
                        ))
                    })?;
                    (GoFileType::GoMod, version)
                }
                GoRequestType::ModuleZip => {
                    let version = version.ok_or_else(|| {
                        crate::repository::RepositoryHandlerError::Other(Box::new(
                            crate::utils::bad_request::BadRequestErrors::Other(
                                "Version required for module zip upload".to_string(),
                            ),
                        ))
                    })?;
                    (GoFileType::Zip, version)
                }
                _ => {
                    return Ok(RepoResponse::unsupported_method_response(
                        request.parts.method,
                        this.full_type(),
                    ));
                }
            };

            let bytes = request.body.body_as_bytes().await?;
            if bytes.is_empty() {
                return Ok(RepoResponse::basic_text_response(
                    http::StatusCode::BAD_REQUEST,
                    "Upload payload must not be empty",
                ));
            }

            // Validate .info payload to ensure version consistency
            if file_type == GoFileType::Info {
                let value: Value = serde_json::from_slice(&bytes).map_err(|err| {
                    crate::repository::RepositoryHandlerError::Other(Box::new(
                        crate::utils::bad_request::BadRequestErrors::Other(format!(
                            "Invalid .info payload: {}",
                            err
                        )),
                    ))
                })?;
                if let Some(info_version) = value.get("Version").and_then(Value::as_str) {
                    if info_version != version.as_str() {
                        return Ok(RepoResponse::basic_text_response(
                            http::StatusCode::BAD_REQUEST,
                            format!(
                                "Version mismatch: info file declares {}, expected {}",
                                info_version,
                                version.as_str()
                            ),
                        ));
                    }
                }
            }

            this.save_go_module_file(&module_path, &version, file_type.clone(), bytes.to_vec())
                .await?;
            this.record_go_catalog_entry(&module_path, &version, file_type.clone(), publisher_id)
                .await?;
            if file_type == GoFileType::Zip {
                if let Err(err) = webhooks::enqueue_package_path_event(
                    &this.site(),
                    this.id(),
                    WebhookEventType::PackagePublished,
                    go_cache_path(&module_path, &version, GoFileType::Zip),
                    PackageWebhookActor::from_user(&user),
                    false,
                )
                .await
                {
                    warn!(error = %err, "Failed to enqueue Go zip publish webhook");
                }
            }

            match file_type {
                GoFileType::Info => {
                    this.ensure_version_list(&module_path, &version).await?;
                    let has_mod = this
                        .module_file_exists(&module_path, &version, GoFileType::GoMod)
                        .await?;
                    let has_zip = this
                        .module_file_exists(&module_path, &version, GoFileType::Zip)
                        .await?;
                    if has_mod && has_zip {
                        if let Err(err) = this.refresh_latest_aliases(&module_path).await {
                            warn!(
                                module = %module_path.as_str(),
                                version = %version.as_str(),
                                ?err,
                                "Failed to refresh latest aliases after info upload"
                            );
                        }
                    }
                }
                GoFileType::GoMod => {
                    let has_info = this
                        .module_file_exists(&module_path, &version, GoFileType::Info)
                        .await?;
                    let has_zip = this
                        .module_file_exists(&module_path, &version, GoFileType::Zip)
                        .await?;
                    if has_info && has_zip {
                        this.ensure_version_list(&module_path, &version).await?;
                        if let Err(err) = this.refresh_latest_aliases(&module_path).await {
                            warn!(
                                module = %module_path.as_str(),
                                version = %version.as_str(),
                                ?err,
                                "Failed to refresh latest aliases after go.mod upload"
                            );
                        }
                    }
                }
                GoFileType::Zip => {
                    let has_info = this
                        .module_file_exists(&module_path, &version, GoFileType::Info)
                        .await?;
                    if has_info {
                        this.ensure_version_list(&module_path, &version).await?;
                        let has_mod = this
                            .module_file_exists(&module_path, &version, GoFileType::GoMod)
                            .await?;
                        if has_mod {
                            if let Err(err) = this.refresh_latest_aliases(&module_path).await {
                                warn!(
                                    module = %module_path.as_str(),
                                    version = %version.as_str(),
                                    ?err,
                                    "Failed to refresh latest aliases after zip upload"
                                );
                            }
                        }
                    }
                }
                GoFileType::GoModWithoutVersion => {}
            }

            Ok(RepoResponse::Other(
                ResponseBuilder::created().body("Go module artifact stored"),
            ))
        }
    }
}

fn longest_common_prefix(components: &[Vec<String>]) -> Vec<String> {
    let mut prefix: Option<Vec<String>> = None;
    for entry in components {
        match &mut prefix {
            Some(existing) => {
                let len = existing
                    .iter()
                    .zip(entry.iter())
                    .take_while(|(a, b)| a == b)
                    .count();
                existing.truncate(len);
            }
            None => prefix = Some(entry.clone()),
        }
        if matches!(prefix, Some(ref p) if p.is_empty()) {
            break;
        }
    }
    prefix.unwrap_or_default()
}

#[cfg(test)]
mod tests;
