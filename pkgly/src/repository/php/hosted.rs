use std::{path::PathBuf, sync::Arc};

use bytes::Bytes;
use futures::StreamExt;
use http::{StatusCode, header::CONTENT_LOCATION};
use nr_core::{
    database::entities::{
        project::{DBProject, ProjectDBType, versions::DBProjectVersion},
        repository::{DBRepository, DBRepositoryConfig},
    },
    repository::{
        Visibility,
        config::RepositoryConfigType,
        project::{PhpPackageMetadata, ReleaseType, VersionData},
    },
    storage::StoragePath,
    user::permissions::RepositoryActions,
};
use nr_storage::{DynStorage, FileContent, Storage, StorageFile};
use parking_lot::RwLock;
use serde_json::to_value;
use sha1::{Digest, Sha1};
use tempfile::{NamedTempFile, TempPath};
use tokio::{fs::File, io::AsyncWriteExt};
use uuid::Uuid;

use super::{
    ComposerDistPath, ComposerMetadataDocument, ComposerPackage, ComposerRootIndex,
    PhpRepositoryError,
    configs::{PhpRepositoryConfig, PhpRepositoryConfigType},
    extract_composer_from_zip, validate_package_against_path,
};
use crate::{
    app::Pkgly,
    repository::{
        RepoResponse, Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        RepositoryRequest, RepositoryRequestBody,
        utils::{RepositoryExt, can_read_repository_with_auth},
    },
    utils::ResponseBuilder,
};

#[derive(Debug)]
pub struct PhpRepositoryInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub repository: DBRepository,
    pub config: PhpRepositoryConfig,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub storage_name: String,
}

#[derive(Debug, Clone)]
pub struct PhpHosted(pub Arc<PhpRepositoryInner>);

impl PhpHosted {
    #[cfg(test)]
    pub(super) fn composer_shasum_for_bytes(bytes: &[u8]) -> String {
        let mut hasher = Sha1::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
    ) -> Result<Self, RepositoryFactoryError> {
        let config = DBRepositoryConfig::<PhpRepositoryConfig>::get_config(
            repository.id,
            PhpRepositoryConfigType::get_type_static(),
            site.as_ref(),
        )
        .await?
        .map(|cfg| cfg.value.0)
        .unwrap_or_default();
        let storage_name = storage.storage_config().storage_config.storage_name.clone();
        Ok(Self(Arc::new(PhpRepositoryInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            repository,
            config,
            storage,
            site,
            storage_name,
        })))
    }

    fn id(&self) -> Uuid {
        self.0.id
    }

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn visibility(&self) -> Visibility {
        *self.0.visibility.read()
    }

    fn storage_name(&self) -> &str {
        &self.0.storage_name
    }

    fn metadata_path(&self, vendor: &str, package: &str, is_dev: bool) -> StoragePath {
        let suffix = if is_dev { "~dev" } else { "" };
        StoragePath::from(format!(
            "p2/{}/{}{}.json",
            vendor.to_ascii_lowercase(),
            package.to_ascii_lowercase(),
            suffix
        ))
    }

    fn dist_storage_path(&self, dist: &ComposerDistPath) -> StoragePath {
        StoragePath::from(format!(
            "dist/{}/{}/{}",
            dist.vendor.to_ascii_lowercase(),
            dist.package.to_ascii_lowercase(),
            dist.filename
        ))
    }

    pub(super) fn format_dist_url(
        app_url: &str,
        is_https: bool,
        storage_name: &str,
        repo_name: &str,
        dist: &StoragePath,
    ) -> String {
        let base = if !app_url.is_empty() {
            app_url.trim_end_matches('/').to_string()
        } else {
            // Best-effort fallback for environments that haven't configured a public URL.
            // Keep this consistent with other repos (e.g. PHP proxy).
            let scheme = if is_https { "https" } else { "http" };
            format!("{scheme}://localhost:6742")
        };

        format!(
            "{base}/repositories/{storage_name}/{repo_name}/{dist}",
            base = base,
            storage_name = storage_name,
            repo_name = repo_name,
            dist = dist
        )
    }

    fn dist_url(&self, dist: &StoragePath) -> String {
        let site = self.site();
        let instance = site.inner.instance.lock();
        Self::format_dist_url(
            &instance.app_url,
            instance.is_https,
            self.storage_name(),
            &self.name(),
            dist,
        )
    }

    async fn load_metadata(
        &self,
        path: &StoragePath,
    ) -> Result<Option<ComposerMetadataDocument>, PhpRepositoryError> {
        let Some(file) = self.storage().open_file(self.id(), path).await? else {
            return Ok(None);
        };
        let (content, meta) = match file {
            StorageFile::File { content, meta } => (content, meta),
            StorageFile::Directory { .. } => return Ok(None),
        };
        let size_hint: usize = meta.file_type().file_size.try_into().unwrap_or(16_384);
        let bytes = content
            .read_to_vec(size_hint)
            .await
            .map_err(|err| PhpRepositoryError::InvalidComposer(err.to_string()))?;
        let doc = serde_json::from_slice::<ComposerMetadataDocument>(&bytes)?;
        Ok(Some(doc))
    }

    async fn write_metadata(
        &self,
        package: &ComposerPackage,
        dist_path: &StoragePath,
        shasum: Option<String>,
    ) -> Result<(), PhpRepositoryError> {
        let (vendor, package_name) = package.name.split_once('/').ok_or_else(|| {
            PhpRepositoryError::InvalidComposer("package name missing vendor".into())
        })?;
        let is_dev = package.version.to_ascii_lowercase().contains("dev");
        let metadata_path = self.metadata_path(vendor, package_name, is_dev);
        let dist_url = self.dist_url(dist_path);
        let doc = match self.load_metadata(&metadata_path).await? {
            Some(mut existing) => {
                existing.add_version(package, dist_url.clone(), shasum.clone());
                existing
            }
            None => {
                ComposerMetadataDocument::with_version(package, dist_url.clone(), shasum.clone())
            }
        };

        let tmp_path = StoragePath::from(format!("{}.tmp", metadata_path));
        let json_bytes = serde_json::to_vec(&doc)?;
        self.storage()
            .save_file(self.id(), FileContent::Content(json_bytes), &tmp_path)
            .await?;
        let moved = self
            .storage()
            .move_file(self.id(), &tmp_path, &metadata_path)
            .await?;
        if !moved {
            return Err(PhpRepositoryError::InvalidComposer(
                "failed to finalize metadata file".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn upsert_metadata(
        &self,
        publisher: Option<i32>,
        composer: &ComposerPackage,
        dist_path: &StoragePath,
    ) -> Result<(), PhpRepositoryError> {
        let project_key = composer.name.to_ascii_lowercase();
        let project = if let Some(project) =
            DBProject::find_by_project_key(&project_key, self.id(), self.site().as_ref()).await?
        {
            project
        } else {
            let (vendor, package) = composer.name.split_once('/').ok_or_else(|| {
                PhpRepositoryError::InvalidComposer("package name missing vendor".into())
            })?;
            let new_project = nr_core::database::entities::project::NewProject {
                scope: Some(vendor.to_string()),
                project_key: project_key.clone(),
                name: package.to_string(),
                description: None,
                repository: self.id(),
                storage_path: composer.name.to_ascii_lowercase(),
            };
            new_project.insert(self.site().as_ref()).await?
        };

        if DBProjectVersion::find_by_version_and_project(
            &composer.version,
            project.id,
            &self.site().database,
        )
        .await?
        .is_some()
        {
            return Ok(());
        }

        let metadata = PhpPackageMetadata {
            filename: dist_path
                .clone()
                .into_iter()
                .last()
                .map(|c| c.to_string())
                .unwrap_or_default(),
            ..Default::default()
        };

        let new_version = nr_core::database::entities::project::versions::NewVersion {
            project_id: project.id,
            repository_id: self.id(),
            version: composer.version.clone(),
            release_type: ReleaseType::release_type_from_version(&composer.version),
            version_path: dist_path.to_string(),
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

    async fn receive_upload(
        &self,
        body: RepositoryRequestBody,
    ) -> Result<(TempPath, String), PhpRepositoryError> {
        let mut hasher = Sha1::new();
        let temp = NamedTempFile::new()
            .map_err(|err| PhpRepositoryError::InvalidComposer(err.to_string()))?;
        let mut writer = File::from_std(
            temp.as_file()
                .try_clone()
                .map_err(|err| PhpRepositoryError::InvalidComposer(err.to_string()))?,
        );
        let mut stream = body.into_byte_stream();
        while let Some(chunk) = stream.next().await {
            let chunk: Bytes = chunk?;
            hasher.update(&chunk);
            writer
                .write_all(&chunk)
                .await
                .map_err(|err| PhpRepositoryError::InvalidComposer(err.to_string()))?;
        }
        writer
            .flush()
            .await
            .map_err(|err| PhpRepositoryError::InvalidComposer(err.to_string()))?;
        let shasum = format!("{:x}", hasher.finalize());
        let path = temp.into_temp_path();
        Ok((path, shasum))
    }

    async fn ingest_upload(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, PhpRepositoryError> {
        let Some(user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id(), self.site().as_ref())
            .await?
        else {
            return Ok(RepoResponse::unauthorized());
        };

        let path = request.path.clone();
        let PhpHostedPath::Dist(dist) = parse_path(&path)? else {
            return Ok(RepoResponse::basic_text_response(
                StatusCode::BAD_REQUEST,
                "Uploads must target dist/{vendor}/{package}/{version}.zip",
            ));
        };

        let body = request.body;
        let (temp_path, shasum) = self.receive_upload(body).await?;
        let archive_path = PathBuf::from(temp_path.as_ref() as &std::path::Path);
        let composer =
            tokio::task::spawn_blocking(move || extract_composer_from_zip(&archive_path))
                .await
                .map_err(|err| PhpRepositoryError::InvalidComposer(err.to_string()))??;

        validate_package_against_path(&composer, &dist)?;

        let dist_storage_path = self.dist_storage_path(&dist);
        self.storage()
            .save_file(
                self.id(),
                FileContent::Path(temp_path.to_path_buf()),
                &dist_storage_path,
            )
            .await?;

        self.write_metadata(&composer, &dist_storage_path, Some(shasum.clone()))
            .await?;
        self.upsert_metadata(Some(user.id), &composer, &dist_storage_path)
            .await?;

        Ok(RepoResponse::Other(
            ResponseBuilder::created()
                .header(CONTENT_LOCATION, self.dist_url(&dist_storage_path))
                .empty(),
        ))
    }

    async fn handle_metadata_or_dist(
        &self,
        request: RepositoryRequest,
        path: PhpHostedPath,
    ) -> Result<RepoResponse, PhpRepositoryError> {
        let repository_id = self.id();
        let visibility = self.visibility();
        let site = self.site();
        let storage = self.storage();
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
                StatusCode::UNAUTHORIZED,
                "Missing permission to read repository",
            ));
        }
        match path {
            PhpHostedPath::Metadata {
                vendor,
                package,
                is_dev,
            } => {
                let metadata_path = self.metadata_path(&vendor, &package, is_dev);
                let file = storage.open_file(repository_id, &metadata_path).await?;
                Ok(file.into())
            }
            PhpHostedPath::Dist(dist) => {
                let dist_path = self.dist_storage_path(&dist);
                let file = storage.open_file(repository_id, &dist_path).await?;
                Ok(file.into())
            }
            PhpHostedPath::RootIndex => {
                let index = ComposerRootIndex::new(self.storage_name(), &self.name());
                Ok(RepoResponse::Other(ResponseBuilder::ok().json(&index)))
            }
            PhpHostedPath::Unknown => Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            )),
        }
    }

    async fn handle_head_request(
        &self,
        request: RepositoryRequest,
        path: PhpHostedPath,
    ) -> Result<RepoResponse, PhpRepositoryError> {
        let repository_id = self.id();
        let visibility = self.visibility();
        let site = self.site();
        let storage = self.storage();
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
                StatusCode::UNAUTHORIZED,
                "Missing permission to read repository",
            ));
        }
        match path {
            PhpHostedPath::Metadata {
                vendor,
                package,
                is_dev,
            } => {
                let metadata_path = self.metadata_path(&vendor, &package, is_dev);
                let meta = storage
                    .get_file_information(repository_id, &metadata_path)
                    .await?;
                Ok(meta.into())
            }
            PhpHostedPath::Dist(dist) => {
                let dist_path = self.dist_storage_path(&dist);
                let meta = storage
                    .get_file_information(repository_id, &dist_path)
                    .await?;
                Ok(meta.into())
            }
            PhpHostedPath::RootIndex => Ok(RepoResponse::Other(ResponseBuilder::ok().empty())),
            PhpHostedPath::Unknown => Ok(RepoResponse::basic_text_response(
                StatusCode::NOT_FOUND,
                "Not Found",
            )),
        }
    }
}

impl RepositoryExt for PhpHosted {}

impl Repository for PhpHosted {
    type Error = PhpRepositoryError;

    fn get_storage(&self) -> DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "php"
    }

    fn full_type(&self) -> &'static str {
        "php/hosted"
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            PhpRepositoryConfigType::get_type_static(),
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
        let this = self.clone();
        async move {
            let path = parse_path(&request.path)?;
            this.handle_metadata_or_dist(request, path).await
        }
    }

    fn handle_put(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.ingest_upload(request).await }
    }

    fn handle_post(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.ingest_upload(request).await }
    }

    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move {
            let path = parse_path(&request.path)?;
            this.handle_head_request(request, path).await
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PhpHostedPath {
    RootIndex,
    Metadata {
        vendor: String,
        package: String,
        is_dev: bool,
    },
    Dist(ComposerDistPath),
    Unknown,
}

fn parse_path(path: &StoragePath) -> Result<PhpHostedPath, PhpRepositoryError> {
    let components: Vec<String> = path.clone().into_iter().map(|c| c.to_string()).collect();
    if components.is_empty() {
        return Ok(PhpHostedPath::RootIndex);
    }
    if components.len() == 1 && components[0].eq_ignore_ascii_case("packages.json") {
        return Ok(PhpHostedPath::RootIndex);
    }

    if components.get(0).map(|s| s.as_str()) == Some("p2") && components.len() >= 3 {
        let vendor = components[1].clone();
        let file = components.last().cloned().unwrap_or_default();
        let is_dev = file.ends_with("~dev.json");
        let trimmed = file.trim_end_matches("~dev.json").trim_end_matches(".json");
        let package = trimmed.to_string();
        return Ok(PhpHostedPath::Metadata {
            vendor,
            package,
            is_dev,
        });
    }

    if components.get(0).map(|s| s.as_str()) == Some("dist") || components.len() >= 3 {
        let dist = ComposerDistPath::try_from(path)?;
        return Ok(PhpHostedPath::Dist(dist));
    }

    Ok(PhpHostedPath::Unknown)
}
