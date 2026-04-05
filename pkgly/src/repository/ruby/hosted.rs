use std::sync::Arc;

use futures_util::StreamExt;
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
        project::{ReleaseType, RubyPackageMetadata, VersionData},
    },
    storage::StoragePath,
    user::permissions::RepositoryActions,
};
use nr_storage::{DynStorage, FileContent, Storage};
use parking_lot::RwLock;
use serde::Deserialize;
use sha2::Digest;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

use super::{REPOSITORY_TYPE_ID, RubyRepositoryConfigType, RubyRepositoryError};
use crate::{
    app::Pkgly,
    repository::{RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError},
    utils::ResponseBuilder,
};
use http::header::CONTENT_LENGTH;

#[derive(Debug)]
pub struct RubyHostedInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub repository: DBRepository,
    pub storage: DynStorage,
    pub site: Pkgly,
}

#[derive(Debug, Clone)]
pub struct RubyHosted(pub Arc<RubyHostedInner>);

fn strip_platform_suffix(version_key: &str, platform: Option<&str>) -> String {
    let Some(platform) = platform else {
        return version_key.to_string();
    };
    let suffix = format!("-{platform}");
    version_key
        .strip_suffix(&suffix)
        .unwrap_or(version_key)
        .to_string()
}

fn other_internal_error_from_message(message: String) -> RubyRepositoryError {
    let err = std::io::Error::new(std::io::ErrorKind::Other, message);
    RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
}

impl RubyHosted {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
    ) -> Result<Self, RepositoryFactoryError> {
        Ok(Self(Arc::new(RubyHostedInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            repository,
            storage,
            site,
        })))
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

    fn id(&self) -> Uuid {
        self.0.id
    }

    fn version_key(version: &str, platform: Option<&str>) -> String {
        match platform {
            Some("ruby") | None => version.to_string(),
            Some(platform) => format!("{version}-{platform}"),
        }
    }

    fn gem_file_name(name: &str, version: &str, platform: Option<&str>) -> String {
        match platform {
            Some("ruby") | None => format!("{name}-{version}.gem"),
            Some(platform) => format!("{name}-{version}-{platform}.gem"),
        }
    }

    fn parse_quick_gemspec_file_name(file_name: &str) -> Option<super::utils::ParsedGemFileName> {
        let base = file_name.strip_suffix(".gemspec.rz")?;
        if base.is_empty() || base.contains('/') {
            return None;
        }
        super::utils::parse_gem_file_name(&format!("{base}.gem"))
    }

    async fn load_ruby_metadata(
        &self,
        package_key: &str,
        version_key: &str,
    ) -> Result<Option<RubyPackageMetadata>, RubyRepositoryError> {
        let Some(project) =
            DBProject::find_by_project_key(package_key, self.id(), self.site().as_ref()).await?
        else {
            return Ok(None);
        };

        let Some(version) = DBProjectVersion::find_by_version_and_project(
            version_key,
            project.id,
            self.site().as_ref(),
        )
        .await?
        else {
            return Ok(None);
        };

        let metadata_value = version.extra.0.extra.ok_or_else(|| {
            let message = format!(
                "Missing ruby metadata for {} {}",
                project.key, version.version
            );
            let err = std::io::Error::new(std::io::ErrorKind::Other, message);
            RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
        })?;
        let metadata: RubyPackageMetadata = serde_json::from_value(metadata_value)?;
        Ok(Some(metadata))
    }

    async fn build_quick_gemspec_bytes(
        &self,
        file_name: &str,
    ) -> Result<Option<Vec<u8>>, RubyRepositoryError> {
        let Some(parsed) = Self::parse_quick_gemspec_file_name(file_name) else {
            return Ok(None);
        };

        let package_key = parsed.name.to_lowercase();
        let version_key = Self::version_key(&parsed.version, parsed.platform.as_deref());
        let Some(metadata) = self.load_ruby_metadata(&package_key, &version_key).await? else {
            return Ok(None);
        };

        let platform = parsed
            .platform
            .clone()
            .unwrap_or_else(|| "ruby".to_string());
        let spec = super::full_index::GemSpecEntry {
            name: parsed.name,
            version: parsed.version,
            platform,
            dependencies: metadata.dependencies,
            required_ruby: metadata.required_ruby,
            required_rubygems: metadata.required_rubygems,
        };
        let bytes = super::full_index::build_gemspec_rz(&spec).map_err(|message| {
            let err = std::io::Error::new(std::io::ErrorKind::Other, message);
            RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
        })?;

        Ok(Some(bytes))
    }

    async fn handle_quick_gemspec_head(
        &self,
        file_name: &str,
    ) -> Result<RepoResponse, RubyRepositoryError> {
        let Some(bytes) = self.build_quick_gemspec_bytes(file_name).await? else {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            ));
        };

        Ok(RepoResponse::Other(
            ResponseBuilder::ok()
                .content_type(mime::APPLICATION_OCTET_STREAM)
                .header(CONTENT_LENGTH, bytes.len().to_string())
                .empty(),
        ))
    }

    async fn parse_gem_from_temp_path(
        &self,
        temp_path: &tempfile::TempPath,
    ) -> Result<super::gem::ParsedGemSpec, RubyRepositoryError> {
        let parse_path = temp_path.to_path_buf();
        tokio::task::spawn_blocking(move || super::gem::parse_gemspec_from_gem_path(&parse_path))
            .await
            .map_err(|err| {
                RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
            })?
            .map_err(|err| RubyRepositoryError::InvalidRequest(err.to_string()))
    }

    async fn ensure_project(
        &self,
        package_key: &str,
        display_name: &str,
    ) -> Result<DBProject, RubyRepositoryError> {
        if let Some(project) =
            DBProject::find_by_project_key(package_key, self.id(), self.site().as_ref()).await?
        {
            return Ok(project);
        }

        let new_project = NewProject {
            scope: None,
            project_key: package_key.to_string(),
            name: display_name.to_string(),
            description: None,
            repository: self.id(),
            storage_path: format!("{package_key}/"),
        };
        Ok(new_project.insert(self.site().as_ref()).await?)
    }

    #[instrument(
        name = "ruby_index_rebuild",
        skip(self),
        fields(
            nr.repository.id = %self.id(),
            nr.repository.name = %self.0.name,
            nr.repository.type = "ruby/hosted",
            nr.ruby.index.rows = tracing::field::Empty,
            nr.ruby.index.gems = tracing::field::Empty
        )
    )]
    async fn rebuild_indexes(&self) -> Result<(), RubyRepositoryError> {
        let rows = self.load_compact_index_rows().await?;
        tracing::Span::current().record("nr.ruby.index.rows", rows.len() as u64);

        self.write_compact_index_files(&rows).await?;
        self.write_full_index_files(&rows).await?;

        debug!("Rebuilt ruby indexes");
        Ok(())
    }

    async fn write_compact_index_files(
        &self,
        rows: &[super::compact_index::RubyCompactIndexRow],
    ) -> Result<(), RubyRepositoryError> {
        let created_at = chrono::Utc::now();
        let artifacts = super::compact_index::build_compact_index_artifacts(created_at, rows)
            .map_err(|message| {
                error!(%message, "Failed to build ruby compact index artifacts");
                let err = std::io::Error::new(std::io::ErrorKind::Other, message);
                RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
            })?;
        tracing::Span::current().record("nr.ruby.index.gems", artifacts.infos.len() as u64);

        let storage = self.storage();
        let repository_id = self.id();

        storage
            .save_file(
                repository_id,
                FileContent::Content(artifacts.names.into_bytes()),
                &StoragePath::from("names"),
            )
            .await?;

        storage
            .save_file(
                repository_id,
                FileContent::Content(artifacts.versions.into_bytes()),
                &StoragePath::from("versions"),
            )
            .await?;

        self.delete_stale_info_files(&artifacts.infos).await?;

        for (gem_key, info) in artifacts.infos {
            let path = StoragePath::from(format!("info/{gem_key}"));
            storage
                .save_file(
                    repository_id,
                    FileContent::Content(info.into_bytes()),
                    &path,
                )
                .await?;
        }

        Ok(())
    }

    async fn delete_stale_info_files(
        &self,
        desired: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), RubyRepositoryError> {
        let storage = self.storage();
        let repository_id = self.id();

        let Some(stream) = storage
            .stream_directory(repository_id, &StoragePath::from("info/"))
            .await?
        else {
            return Ok(());
        };

        let files = nr_storage::collect_directory_stream(stream).await?;
        for file in files {
            if !matches!(file.file_type(), nr_storage::FileType::File(_)) {
                continue;
            }
            if desired.contains_key(file.name()) {
                continue;
            }
            let path = StoragePath::from(format!("info/{}", file.name()));
            storage.delete_file(repository_id, &path).await?;
        }

        Ok(())
    }

    async fn write_full_index_files(
        &self,
        rows: &[super::compact_index::RubyCompactIndexRow],
    ) -> Result<(), RubyRepositoryError> {
        let mut specs = Vec::with_capacity(rows.len());
        for row in rows {
            let platform = row
                .metadata
                .platform
                .clone()
                .unwrap_or_else(|| "ruby".to_string());
            let version = strip_platform_suffix(&row.version, row.metadata.platform.as_deref());
            specs.push(super::full_index::SpecsIndexEntry {
                name: row.gem_name.clone(),
                version,
                platform,
            });
        }

        let specs_bytes =
            super::full_index::build_specs_gz(&specs).map_err(other_internal_error_from_message)?;
        let latest_specs_bytes = specs_bytes.clone();
        let prerelease_specs_bytes =
            super::full_index::build_empty_specs_gz().map_err(other_internal_error_from_message)?;

        let storage = self.storage();
        let repository_id = self.id();

        for (path, payload) in [
            ("specs.4.8.gz", specs_bytes),
            ("latest_specs.4.8.gz", latest_specs_bytes),
            ("prerelease_specs.4.8.gz", prerelease_specs_bytes),
        ] {
            storage
                .save_file(
                    repository_id,
                    FileContent::Content(payload),
                    &StoragePath::from(path),
                )
                .await?;
        }

        Ok(())
    }

    async fn load_compact_index_rows(
        &self,
    ) -> Result<Vec<super::compact_index::RubyCompactIndexRow>, RubyRepositoryError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            project_key: String,
            version: String,
            extra: sqlx::types::Json<VersionData>,
        }

        let db = &self.site().database;
        let repository_id = self.id();
        let rows: Vec<Row> = sqlx::query_as(
            r#"
            SELECT p.key AS project_key,
                   pv.version AS version,
                   pv.extra AS extra
            FROM projects p
            JOIN project_versions pv ON pv.project_id = p.id
            WHERE p.repository_id = $1
            "#,
        )
        .bind(repository_id)
        .fetch_all(db)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let metadata_value = row.extra.0.extra.ok_or_else(|| {
                let message = format!(
                    "Missing ruby metadata for {} {}",
                    row.project_key, row.version
                );
                let err = std::io::Error::new(std::io::ErrorKind::Other, message);
                RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
            })?;
            let metadata: RubyPackageMetadata = serde_json::from_value(metadata_value)?;

            out.push(super::compact_index::RubyCompactIndexRow {
                gem_key: row.project_key.clone(),
                gem_name: row.project_key.clone(),
                version: row.version,
                metadata,
            });
        }

        Ok(out)
    }

    async fn save_uploaded_gem(
        &self,
        request_body: crate::repository::RepositoryRequestBody,
    ) -> Result<(tempfile::TempPath, u64, String), RubyRepositoryError> {
        let temp_path = tempfile::NamedTempFile::new()
            .map_err(|err| {
                RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
            })?
            .into_temp_path();
        let path_buf = temp_path.to_path_buf();
        let mut file = tokio::fs::File::create(&path_buf).await.map_err(|err| {
            RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
        })?;

        let mut size = 0u64;
        let mut hasher = sha2::Sha256::new();
        let mut stream = request_body.into_byte_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            size = size.saturating_add(chunk.len() as u64);
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|err| {
                RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
            })?;
        }
        file.flush().await.map_err(|err| {
            RubyRepositoryError::Other(Box::new(crate::error::OtherInternalError::new(err)))
        })?;

        let sha256 = format!("{:x}", hasher.finalize());
        Ok((temp_path, size, sha256))
    }

    #[instrument(
        name = "ruby_hosted_publish",
        skip(self, request),
        fields(
            nr.repository.id = %self.id(),
            nr.repository.name = %self.0.name,
            nr.repository.type = "ruby/hosted",
            nr.ruby.publish.outcome = tracing::field::Empty,
            nr.user.id = tracing::field::Empty,
            nr.ruby.gem.name = tracing::field::Empty,
            nr.ruby.gem.version = tracing::field::Empty,
            nr.ruby.gem.platform = tracing::field::Empty,
            nr.ruby.upload.size_bytes = tracing::field::Empty,
            nr.ruby.upload.sha256 = tracing::field::Empty
        )
    )]
    async fn handle_publish(
        &self,
        request: crate::repository::RepositoryRequest,
    ) -> Result<RepoResponse, RubyRepositoryError> {
        let Some(user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id(), self.site().as_ref())
            .await?
        else {
            tracing::Span::current().record("nr.ruby.publish.outcome", "unauthorized");
            return Ok(RepoResponse::unauthorized());
        };
        let publisher = user.id;
        tracing::Span::current().record("nr.user.id", publisher);

        let (temp_path, size, sha256) = self.save_uploaded_gem(request.body).await?;
        tracing::Span::current().record("nr.ruby.upload.size_bytes", size);
        tracing::Span::current().record("nr.ruby.upload.sha256", &sha256);
        if size == 0 {
            tracing::Span::current().record("nr.ruby.publish.outcome", "empty_upload");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_REQUEST,
                "Empty gem upload",
            ));
        }
        let parsed = self.parse_gem_from_temp_path(&temp_path).await?;
        tracing::Span::current().record("nr.ruby.gem.name", &parsed.name);
        tracing::Span::current().record("nr.ruby.gem.version", &parsed.version);
        tracing::Span::current().record(
            "nr.ruby.gem.platform",
            parsed.platform.as_deref().unwrap_or("ruby"),
        );
        info!(
            gem = %parsed.name,
            version = %parsed.version,
            platform = %parsed.platform.as_deref().unwrap_or("ruby"),
            size_bytes = size,
            "Publishing ruby gem"
        );

        let package_key = parsed.name.to_lowercase();
        let project = self.ensure_project(&package_key, &parsed.name).await?;
        let version_key = Self::version_key(&parsed.version, parsed.platform.as_deref());

        if DBProjectVersion::find_by_version_and_project(
            &version_key,
            project.id,
            self.site().as_ref(),
        )
        .await?
        .is_some()
        {
            tracing::Span::current().record("nr.ruby.publish.outcome", "version_exists");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::CONFLICT,
                "Version already exists",
            ));
        }

        let file_name =
            Self::gem_file_name(&package_key, &parsed.version, parsed.platform.as_deref());
        let gem_path = StoragePath::from(format!("gems/{file_name}"));
        self.storage()
            .save_file(
                self.id(),
                FileContent::Path(temp_path.to_path_buf()),
                &gem_path,
            )
            .await?;

        let metadata = RubyPackageMetadata {
            filename: file_name,
            platform: parsed.platform.clone(),
            sha256: Some(sha256),
            dependencies: parsed.dependencies,
            required_ruby: parsed.required_ruby,
            required_rubygems: parsed.required_rubygems,
        };

        let release_type = ReleaseType::release_type_from_version(&version_key);
        let new_version = NewVersion {
            project_id: project.id,
            repository_id: self.id(),
            version: version_key,
            release_type,
            version_path: gem_path.to_string(),
            publisher: Some(publisher),
            version_page: None,
            extra: VersionData {
                extra: Some(serde_json::to_value(metadata)?),
                ..Default::default()
            },
        };
        new_version.insert(&self.site().database).await?;

        self.rebuild_indexes().await?;

        tracing::Span::current().record("nr.ruby.publish.outcome", "ok");
        info!(
            gem = %parsed.name,
            version = %parsed.version,
            platform = %parsed.platform.as_deref().unwrap_or("ruby"),
            "Published ruby gem"
        );
        Ok(RepoResponse::Other(ResponseBuilder::ok().body(format!(
            "Successfully registered gem: {} ({})",
            parsed.name, parsed.version
        ))))
    }

    #[instrument(
        name = "ruby_hosted_yank",
        skip(self, request),
        fields(
            nr.repository.id = %self.id(),
            nr.repository.name = %self.0.name,
            nr.repository.type = "ruby/hosted",
            nr.ruby.yank.outcome = tracing::field::Empty,
            nr.user.id = tracing::field::Empty,
            nr.ruby.gem.name = tracing::field::Empty,
            nr.ruby.gem.version = tracing::field::Empty,
            nr.ruby.gem.platform = tracing::field::Empty
        )
    )]
    async fn handle_yank(
        &self,
        request: crate::repository::RepositoryRequest,
    ) -> Result<RepoResponse, RubyRepositoryError> {
        let Some(user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id(), self.site().as_ref())
            .await?
        else {
            tracing::Span::current().record("nr.ruby.yank.outcome", "unauthorized");
            return Ok(RepoResponse::unauthorized());
        };
        tracing::Span::current().record("nr.user.id", user.id);

        #[derive(Debug, Deserialize)]
        struct YankForm {
            gem_name: String,
            version: String,
            platform: Option<String>,
        }

        let body = request.body.body_as_string().await?;
        let form: YankForm = serde_urlencoded::from_str(&body).map_err(|err| {
            RubyRepositoryError::InvalidRequest(format!("Invalid yank form: {err}"))
        })?;
        tracing::Span::current().record("nr.ruby.gem.name", &form.gem_name);
        tracing::Span::current().record("nr.ruby.gem.version", &form.version);
        tracing::Span::current().record(
            "nr.ruby.gem.platform",
            form.platform.as_deref().unwrap_or("ruby"),
        );
        info!(
            gem = %form.gem_name,
            version = %form.version,
            platform = %form.platform.as_deref().unwrap_or("ruby"),
            "Yanking ruby gem"
        );

        let package_key = form.gem_name.to_lowercase();
        let Some(project) =
            DBProject::find_by_project_key(&package_key, self.id(), self.site().as_ref()).await?
        else {
            tracing::Span::current().record("nr.ruby.yank.outcome", "gem_not_found");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Gem not found",
            ));
        };

        let version_key = Self::version_key(&form.version, form.platform.as_deref());

        let Some(version) = DBProjectVersion::find_by_version_and_project(
            &version_key,
            project.id,
            self.site().as_ref(),
        )
        .await?
        else {
            tracing::Span::current().record("nr.ruby.yank.outcome", "version_not_found");
            return Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Version not found",
            ));
        };

        sqlx::query("DELETE FROM project_versions WHERE id = $1")
            .bind(version.id)
            .execute(&self.site().database)
            .await?;

        let gem_path = StoragePath::from(version.path);
        let _ = self.storage().delete_file(self.id(), &gem_path).await?;

        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM project_versions WHERE project_id = $1")
                .bind(project.id)
                .fetch_one(&self.site().database)
                .await?;
        if remaining == 0 {
            let _ = sqlx::query("DELETE FROM projects WHERE id = $1")
                .bind(project.id)
                .execute(&self.site().database)
                .await?;
        }

        self.rebuild_indexes().await?;

        tracing::Span::current().record("nr.ruby.yank.outcome", "ok");
        info!(
            gem = %form.gem_name,
            version = %form.version,
            platform = %form.platform.as_deref().unwrap_or("ruby"),
            "Yanked ruby gem"
        );
        Ok(RepoResponse::Other(ResponseBuilder::ok().body(format!(
            "Successfully yanked gem: {} ({})",
            form.gem_name, form.version
        ))))
    }
}

impl Repository for RubyHosted {
    type Error = RubyRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }

    fn full_type(&self) -> &'static str {
        "ruby/hosted"
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
        self.0.repository.active
    }

    fn site(&self) -> Pkgly {
        self.site()
    }

    async fn resolve_project_and_version_for_path(
        &self,
        path: &StoragePath,
    ) -> Result<nr_core::repository::project::ProjectResolution, Self::Error> {
        let directory = path.to_string();
        let Some(ids) =
            DBProjectVersion::find_ids_by_version_dir(&directory, self.id(), self.site().as_ref())
                .await?
        else {
            return Ok(nr_core::repository::project::ProjectResolution::default());
        };
        Ok(ids.into())
    }

    fn handle_get(
        &self,
        request: crate::repository::RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<crate::repository::RepoResponse, Self::Error>> + Send
    {
        let this = self.clone();
        async move {
            if !crate::repository::utils::can_read_repository_with_auth(
                &request.authentication,
                this.visibility(),
                this.id(),
                this.site().as_ref(),
                &request.auth_config,
            )
            .await?
            {
                return Ok(crate::repository::RepoResponse::unauthorized());
            }

            let path = request.path;
            if path.is_directory() {
                return Ok(crate::repository::RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Not Found",
                ));
            }

            let path_str = path.to_string();
            if path_str == "names" {
                if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                    return Ok(file.into());
                }
                let content = super::compact_index::build_names_file(&[]);
                return Ok(crate::repository::RepoResponse::Other(
                    ResponseBuilder::ok().body(content),
                ));
            }
            if path_str == "versions" {
                if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                    return Ok(file.into());
                }
                let content = super::compact_index::build_versions_file(chrono::Utc::now(), &[]);
                return Ok(crate::repository::RepoResponse::Other(
                    ResponseBuilder::ok().body(content),
                ));
            }
            if matches!(
                path_str.as_str(),
                "specs.4.8.gz" | "latest_specs.4.8.gz" | "prerelease_specs.4.8.gz"
            ) {
                if let Some(file) = this.storage().open_file(this.id(), &path).await? {
                    return Ok(file.into());
                }
                let bytes = super::full_index::build_empty_specs_gz()
                    .map_err(other_internal_error_from_message)?;
                return Ok(crate::repository::RepoResponse::Other(
                    ResponseBuilder::ok()
                        .content_type(mime::APPLICATION_OCTET_STREAM)
                        .body(bytes),
                ));
            }
            if let Some(file_name) = path_str.strip_prefix("quick/Marshal.4.8/") {
                let Some(bytes) = this.build_quick_gemspec_bytes(file_name).await? else {
                    return Ok(crate::repository::RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "Not Found",
                    ));
                };
                return Ok(crate::repository::RepoResponse::Other(
                    ResponseBuilder::ok()
                        .content_type(mime::APPLICATION_OCTET_STREAM)
                        .body(bytes),
                ));
            }
            if let Some(name) = path_str.strip_prefix("info/") {
                if name.is_empty() || name.contains('/') {
                    return Ok(crate::repository::RepoResponse::basic_text_response(
                        StatusCode::NOT_FOUND,
                        "Not Found",
                    ));
                }
                let normalized = StoragePath::from(format!("info/{}", name.to_lowercase()));
                let file = this.storage().open_file(this.id(), &normalized).await?;
                return Ok(file.into());
            }
            if path_str.starts_with("gems/") {
                let file = this.storage().open_file(this.id(), &path).await?;
                return Ok(file.into());
            }

            Ok(crate::repository::RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            ))
        }
    }

    fn handle_head(
        &self,
        request: crate::repository::RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<crate::repository::RepoResponse, Self::Error>> + Send
    {
        let this = self.clone();
        async move {
            if !crate::repository::utils::can_read_repository_with_auth(
                &request.authentication,
                this.visibility(),
                this.id(),
                this.site().as_ref(),
                &request.auth_config,
            )
            .await?
            {
                return Ok(crate::repository::RepoResponse::unauthorized());
            }

            let path = request.path;
            if path.is_directory() {
                return Ok(crate::repository::RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Not Found",
                ));
            }

            let path_str = path.to_string();
            if let Some(file_name) = path_str.strip_prefix("quick/Marshal.4.8/") {
                return this.handle_quick_gemspec_head(file_name).await;
            }

            if matches!(
                path_str.as_str(),
                "specs.4.8.gz" | "latest_specs.4.8.gz" | "prerelease_specs.4.8.gz"
            ) {
                if let Some(meta) = this
                    .storage()
                    .get_file_information(this.id(), &path)
                    .await?
                {
                    return Ok(meta.into());
                }
                let bytes = super::full_index::build_empty_specs_gz()
                    .map_err(other_internal_error_from_message)?;
                return Ok(crate::repository::RepoResponse::Other(
                    ResponseBuilder::ok()
                        .content_type(mime::APPLICATION_OCTET_STREAM)
                        .header(CONTENT_LENGTH, bytes.len().to_string())
                        .empty(),
                ));
            }

            if let Some(meta) = this
                .storage()
                .get_file_information(this.id(), &path)
                .await?
            {
                return Ok(meta.into());
            }
            Ok(crate::repository::RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            ))
        }
    }

    fn handle_post(
        &self,
        request: crate::repository::RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<crate::repository::RepoResponse, Self::Error>> + Send
    {
        let this = self.clone();
        async move {
            if request.path.to_string() == "api/v1/gems" {
                return this.handle_publish(request).await;
            }
            Ok(crate::repository::RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            ))
        }
    }

    fn handle_delete(
        &self,
        request: crate::repository::RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<crate::repository::RepoResponse, Self::Error>> + Send
    {
        let this = self.clone();
        async move {
            if request.path.to_string() == "api/v1/gems/yank" {
                return this.handle_yank(request).await;
            }
            Ok(crate::repository::RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            ))
        }
    }
}
