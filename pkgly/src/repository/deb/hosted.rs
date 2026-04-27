use std::{io::Write, sync::Arc};

use bytes::Bytes;
use flate2::{Compression, write::GzEncoder};
use futures::stream;
use http::{
    StatusCode,
    header::{CONTENT_LENGTH, CONTENT_TYPE},
};
use md5::Md5;
use multer::Multipart;
use parking_lot::RwLock;
use serde_json::{self, to_value};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use tokio::task::spawn_blocking;
use tracing::warn;
use uuid::Uuid;
use xz2::write::XzEncoder;

use super::{
    configs::DebHostedConfig,
    metadata::{PackagesRecord, ReleaseEntry, build_release_file, format_packages_entry},
    package::parse_deb_package,
};
use crate::{
    app::{
        Pkgly,
        webhooks::{self, PackageWebhookActor, WebhookEventType},
    },
    repository::{
        RepoResponse, Repository, RepositoryFactoryError, RepositoryRequest,
        utils::{RepositoryExt, can_read_repository_with_auth},
    },
    utils::ResponseBuilder,
};

use nr_core::{
    database::entities::{project::versions::DBProjectVersion, repository::DBRepository},
    repository::{
        Visibility,
        config::RepositoryConfigType,
        project::{DebPackageMetadata, ReleaseType, VersionData},
    },
    storage::StoragePath,
    user::permissions::RepositoryActions,
};
use nr_storage::{DynStorage, FileContent, Storage};

#[derive(Debug)]
pub struct DebHostedInner {
    pub id: Uuid,
    pub name: String,
    pub visibility: RwLock<Visibility>,
    pub repository: DBRepository,
    pub storage: DynStorage,
    pub site: Pkgly,
    pub config: DebHostedConfig,
}

#[derive(Debug, Clone)]
pub struct DebHostedRepository(pub Arc<DebHostedInner>);

impl DebHostedRepository {
    pub async fn load(
        site: Pkgly,
        storage: DynStorage,
        repository: DBRepository,
        config: DebHostedConfig,
    ) -> Result<Self, RepositoryFactoryError> {
        Ok(Self(Arc::new(DebHostedInner {
            id: repository.id,
            name: repository.name.to_string(),
            visibility: RwLock::new(repository.visibility),
            repository,
            storage,
            site,
            config,
        })))
    }

    fn storage(&self) -> DynStorage {
        self.0.storage.clone()
    }

    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }

    fn config(&self) -> &DebHostedConfig {
        &self.0.config
    }

    fn repository_id(&self) -> Uuid {
        self.0.id
    }

    async fn handle_upload(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, super::DebRepositoryError> {
        let Some(user) = request
            .authentication
            .get_user_if_has_action(
                RepositoryActions::Write,
                self.repository_id(),
                self.site().as_ref(),
            )
            .await?
        else {
            return Ok(RepoResponse::unauthorized());
        };
        let content_type = request
            .parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| {
                super::DebRepositoryError::InvalidRequest("Missing content-type".into())
            })?;
        if !content_type.starts_with("multipart/form-data") {
            return Err(super::DebRepositoryError::InvalidRequest(
                "Uploads must use multipart/form-data".into(),
            ));
        }
        let boundary = multer::parse_boundary(&content_type)?;
        let body = request.body.body_as_bytes().await?;
        let stream = stream::once(async move { Ok::<Bytes, multer::Error>(body) });
        let mut multipart = Multipart::new(stream, boundary);

        let mut distribution = None;
        let mut component = None;
        let mut package_bytes: Option<Vec<u8>> = None;
        let mut upload_filename: Option<String> = None;

        while let Some(field) = multipart.next_field().await? {
            match field.name() {
                Some("distribution") => {
                    distribution = Some(field.text().await?);
                }
                Some("component") => {
                    component = Some(field.text().await?);
                }
                Some("package") | Some("file") => {
                    if upload_filename.is_none() {
                        upload_filename = field.file_name().map(|value| value.to_string());
                    }
                    let bytes = field.bytes().await?;
                    package_bytes = Some(bytes.to_vec());
                }
                _ => {}
            }
        }

        let package_bytes = package_bytes.ok_or_else(|| {
            super::DebRepositoryError::InvalidRequest("Missing package field".into())
        })?;
        let package_bytes = Bytes::from(package_bytes);
        let distribution = self
            .lookup_or_default(&self.config().distributions, distribution.as_deref())
            .ok_or_else(|| {
                super::DebRepositoryError::InvalidRequest("Invalid distribution".into())
            })?;
        let component = self
            .lookup_or_default(&self.config().components, component.as_deref())
            .ok_or_else(|| super::DebRepositoryError::InvalidRequest("Invalid component".into()))?;

        let parsed = parse_package_metadata(package_bytes.clone()).await?;
        let package_name = parsed
            .control
            .get("Package")
            .ok_or_else(|| {
                super::DebRepositoryError::InvalidRequest("Missing Package field".into())
            })?
            .to_string();
        let version = parsed
            .control
            .get("Version")
            .ok_or_else(|| {
                super::DebRepositoryError::InvalidRequest("Missing Version field".into())
            })?
            .to_string();
        let architecture = parsed
            .control
            .get("Architecture")
            .ok_or_else(|| {
                super::DebRepositoryError::InvalidRequest("Missing Architecture field".into())
            })?
            .to_string();
        let architecture = self
            .lookup_value(&self.config().architectures, &architecture)
            .ok_or_else(|| {
                super::DebRepositoryError::InvalidRequest("Unsupported architecture".into())
            })?;
        let file_name = upload_filename
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{}_{}_{}.deb", package_name, version, architecture));
        let storage_path = build_pool_path(&component, &package_name, &file_name);
        self.storage()
            .save_file(
                self.repository_id(),
                FileContent::Bytes(package_bytes.clone()),
                &storage_path,
            )
            .await?;
        self.push_metadata(
            user.id,
            distribution,
            component,
            architecture,
            package_name,
            version,
            file_name,
            parsed,
        )
        .await?;
        if let Err(err) = webhooks::enqueue_package_path_event(
            &self.site(),
            self.repository_id(),
            WebhookEventType::PackagePublished,
            storage_path.to_string(),
            PackageWebhookActor::from_user(user),
            false,
        )
        .await
        {
            warn!(error = %err, "Failed to enqueue deb publish webhook");
        }

        Ok(RepoResponse::Other(ResponseBuilder::created().empty()))
    }

    async fn push_metadata(
        &self,
        publisher: i32,
        distribution: String,
        component: String,
        architecture: String,
        package: String,
        version: String,
        file_name: String,
        parsed: ParsedPackage,
    ) -> Result<(), super::DebRepositoryError> {
        let key = package.to_lowercase();
        let project = if let Some(project) = self.get_project_from_key(&key).await? {
            project
        } else {
            let storage_path = build_pool_path(&component, &package, "").to_string();
            let new_project = nr_core::database::entities::project::NewProject {
                scope: None,
                project_key: key.clone(),
                name: package.clone(),
                description: None,
                repository: self.repository_id(),
                storage_path,
            };
            new_project.insert(self.site().as_ref()).await?
        };

        if DBProjectVersion::find_by_version_and_project(&version, project.id, self.site().as_ref())
            .await?
            .is_some()
        {
            return Err(super::DebRepositoryError::InvalidRequest(format!(
                "Version {version} already exists"
            )));
        }

        let metadata = DebPackageMetadata {
            distribution: distribution.clone(),
            component: component.clone(),
            architecture: architecture.clone(),
            filename: build_pool_path(&component, &package, &file_name).to_string(),
            size: parsed.file_size,
            md5: parsed.md5.clone(),
            sha1: parsed.sha1.clone(),
            sha256: parsed.sha256.clone(),
            section: parsed.control.get("Section").map(str::to_owned),
            priority: parsed.control.get("Priority").map(str::to_owned),
            maintainer: parsed.control.get("Maintainer").map(str::to_owned),
            installed_size: parsed
                .control
                .get("Installed-Size")
                .and_then(|value| value.parse::<u64>().ok()),
            depends: parse_dependency_list(parsed.control.get("Depends")),
            homepage: parsed.control.get("Homepage").map(str::to_owned),
            description: parsed.control.get("Description").map(str::to_owned),
        };
        let description = metadata
            .description
            .clone()
            .and_then(|value| value.lines().next().map(str::to_owned));

        let new_version = nr_core::database::entities::project::versions::NewVersion {
            project_id: project.id,
            repository_id: self.id(),
            version: version.clone(),
            release_type: ReleaseType::release_type_from_version(&version),
            version_path: build_pool_path(&component, &package, &file_name).to_string(),
            publisher: Some(publisher),
            version_page: None,
            extra: VersionData {
                description,
                extra: Some(to_value(metadata)?),
                ..Default::default()
            },
        };
        new_version.insert(self.site().as_ref()).await?;
        Ok(())
    }

    async fn handle_dists_request(
        &self,
        request: RepositoryRequest,
    ) -> Result<RepoResponse, super::DebRepositoryError> {
        let segments: Vec<String> = request
            .path
            .clone()
            .into_iter()
            .map(|segment| segment.to_string())
            .collect();
        if segments.len() < 3 {
            return Ok(not_found());
        }
        let distribution = match self.lookup_value(&self.config().distributions, &segments[1]) {
            Some(value) => value,
            None => return Ok(not_found()),
        };
        match segments[2].as_str() {
            value if value.eq_ignore_ascii_case("Release") && segments.len() == 3 => {
                self.serve_release(&distribution).await
            }
            value if value.eq_ignore_ascii_case("Release.gpg") => {
                Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "Release signatures are not available. Configure apt with [trusted=yes].",
                ))
            }
            value if value.eq_ignore_ascii_case("InRelease") => {
                Ok(RepoResponse::basic_text_response(
                    StatusCode::NOT_FOUND,
                    "InRelease is not available. Configure apt with [trusted=yes].",
                ))
            }
            _ => {
                if segments.len() < 5 {
                    return Ok(not_found());
                }
                let component = match self.lookup_value(&self.config().components, &segments[2]) {
                    Some(value) => value,
                    None => return Ok(not_found()),
                };
                let binary = &segments[3];
                let Some(architecture_raw) = binary.strip_prefix("binary-") else {
                    return Ok(not_found());
                };
                let architecture =
                    match self.lookup_value(&self.config().architectures, architecture_raw) {
                        Some(value) => value,
                        None => return Ok(not_found()),
                    };
                let file = segments[4].as_str();
                let variant = match file {
                    "Packages" => PackageVariant::Plain,
                    "Packages.gz" => PackageVariant::Gzip,
                    "Packages.xz" => PackageVariant::Xz,
                    _ => return Ok(not_found()),
                };
                self.serve_packages(&distribution, &component, &architecture, variant)
                    .await
            }
        }
    }

    async fn serve_release(
        &self,
        distribution: &str,
    ) -> Result<RepoResponse, super::DebRepositoryError> {
        let mut entries = Vec::new();
        for component in &self.config().components {
            for architecture in &self.config().architectures {
                let index = self
                    .build_packages_index(distribution, component, architecture)
                    .await?;
                let base_path =
                    format!("dists/{distribution}/{component}/binary-{architecture}/Packages");
                entries.push(ReleaseEntry {
                    path: base_path.clone(),
                    size: index.raw_hashes.size,
                    md5: index.raw_hashes.md5.clone(),
                    sha1: index.raw_hashes.sha1.clone(),
                    sha256: index.raw_hashes.sha256.clone(),
                });
                entries.push(ReleaseEntry {
                    path: format!("{base_path}.gz"),
                    size: index.gz_hashes.size,
                    md5: index.gz_hashes.md5.clone(),
                    sha1: index.gz_hashes.sha1.clone(),
                    sha256: index.gz_hashes.sha256.clone(),
                });
                entries.push(ReleaseEntry {
                    path: format!("{base_path}.xz"),
                    size: index.xz_hashes.size,
                    md5: index.xz_hashes.md5.clone(),
                    sha1: index.xz_hashes.sha1.clone(),
                    sha256: index.xz_hashes.sha256.clone(),
                });
            }
        }
        let body = build_release_file(
            distribution,
            &self.config().components,
            &self.config().architectures,
            &entries,
        );
        Ok(RepoResponse::Other(
            ResponseBuilder::ok()
                .header(CONTENT_TYPE, mime::TEXT_PLAIN_UTF_8.to_string())
                .body(body),
        ))
    }

    async fn serve_packages(
        &self,
        distribution: &str,
        component: &str,
        architecture: &str,
        variant: PackageVariant,
    ) -> Result<RepoResponse, super::DebRepositoryError> {
        let index = self
            .build_packages_index(distribution, component, architecture)
            .await?;
        let (bytes, mime, hashes) = match variant {
            PackageVariant::Plain => (
                index.raw,
                mime::TEXT_PLAIN_UTF_8.to_string(),
                index.raw_hashes,
            ),
            PackageVariant::Gzip => (index.gz, "application/gzip".into(), index.gz_hashes),
            PackageVariant::Xz => (index.xz, "application/x-xz".into(), index.xz_hashes),
        };
        Ok(RepoResponse::Other(
            ResponseBuilder::ok()
                .header(CONTENT_TYPE, mime)
                .header(CONTENT_LENGTH, hashes.size.to_string())
                .body(bytes),
        ))
    }

    async fn build_packages_index(
        &self,
        distribution: &str,
        component: &str,
        architecture: &str,
    ) -> Result<PackagesIndex, super::DebRepositoryError> {
        let rows = sqlx::query_as::<_, DebVersionRow>(
            r#"
            SELECT
                p.name as project_name,
                pv.version,
                pv.extra
            FROM project_versions pv
            INNER JOIN projects p ON p.id = pv.project_id
            WHERE p.repository_id = $1
              AND pv.extra -> 'extra' ->> 'distribution' = $2
              AND pv.extra -> 'extra' ->> 'component' = $3
              AND pv.extra -> 'extra' ->> 'architecture' = $4
            ORDER BY pv.created_at DESC
            "#,
        )
        .bind(self.repository_id())
        .bind(distribution)
        .bind(component)
        .bind(architecture)
        .fetch_all(self.site().as_ref())
        .await?;

        let mut body = String::new();
        for row in rows {
            if let Some(metadata) = metadata_from_version(&row.extra.0) {
                let description = metadata
                    .description
                    .clone()
                    .unwrap_or_else(|| row.extra.0.description.clone().unwrap_or_default());
                let record = PackagesRecord {
                    package: row.project_name.clone(),
                    version: row.version.clone(),
                    architecture: metadata.architecture.clone(),
                    section: metadata.section.clone(),
                    priority: metadata.priority.clone(),
                    maintainer: metadata.maintainer.clone(),
                    installed_size: metadata.installed_size,
                    depends: dependency_display(&metadata.depends),
                    description,
                    homepage: metadata.homepage.clone(),
                    filename: metadata.filename.clone(),
                    size: metadata.size,
                    md5: metadata.md5.clone(),
                    sha1: metadata.sha1.clone(),
                    sha256: metadata.sha256.clone(),
                };
                body.push_str(&format_packages_entry(&record));
            }
        }

        build_packages_index_bytes(body.into_bytes()).await
    }

    fn lookup_value(&self, values: &[String], requested: &str) -> Option<String> {
        values
            .iter()
            .find(|value| value.eq_ignore_ascii_case(requested))
            .cloned()
    }

    fn lookup_or_default(&self, values: &[String], requested: Option<&str>) -> Option<String> {
        if let Some(requested) = requested {
            self.lookup_value(values, requested)
        } else {
            values.first().cloned()
        }
    }
}

impl RepositoryExt for DebHostedRepository {}

impl Repository for DebHostedRepository {
    type Error = super::DebRepositoryError;

    fn get_storage(&self) -> nr_storage::DynStorage {
        self.storage()
    }

    fn get_type(&self) -> &'static str {
        "deb"
    }

    fn full_type(&self) -> &'static str {
        "deb/hosted"
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
                this.repository_id(),
                this.site().as_ref(),
                &request.auth_config,
            )
            .await?
            {
                return Ok(RepoResponse::basic_text_response(
                    StatusCode::UNAUTHORIZED,
                    "Missing permission to read repository",
                ));
            }

            let segments: Vec<String> = request
                .path
                .clone()
                .into_iter()
                .map(|segment| segment.to_string())
                .collect();
            if segments.first().map(|value| value.as_str()) == Some("dists") {
                return this.handle_dists_request(request).await;
            }
            if segments.first().map(|value| value.as_str()) == Some("pool") {
                let file = this
                    .storage()
                    .open_file(this.repository_id(), &request.path)
                    .await?;
                return Ok(file.into());
            }

            if segments.is_empty() {
                return Ok(not_found());
            }
            let file = this
                .storage()
                .open_file(this.repository_id(), &request.path)
                .await?;
            Ok(file.into())
        }
    }

    fn handle_post(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_upload(request).await }
    }

    fn handle_put(
        &self,
        request: RepositoryRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse, Self::Error>> + Send {
        let this = self.clone();
        async move { this.handle_upload(request).await }
    }
}

#[cfg(test)]
mod tests;

#[derive(Debug)]
struct PackagesIndex {
    raw: Vec<u8>,
    gz: Vec<u8>,
    xz: Vec<u8>,
    raw_hashes: HashSummary,
    gz_hashes: HashSummary,
    xz_hashes: HashSummary,
}

impl PackagesIndex {
    fn new(raw: Vec<u8>) -> Result<Self, std::io::Error> {
        let gz = compress_gzip(&raw)?;
        let xz = compress_xz(&raw)?;
        Ok(Self {
            raw_hashes: compute_hashes(&raw),
            gz_hashes: compute_hashes(&gz),
            xz_hashes: compute_hashes(&xz),
            raw,
            gz,
            xz,
        })
    }
}

#[derive(Debug, Clone)]
struct HashSummary {
    size: u64,
    md5: String,
    sha1: String,
    sha256: String,
}

enum PackageVariant {
    Plain,
    Gzip,
    Xz,
}

struct ParsedPackage {
    control: super::metadata::ControlFile,
    file_size: u64,
    md5: String,
    sha1: String,
    sha256: String,
}

impl From<crate::repository::deb::package::ParsedDeb> for ParsedPackage {
    fn from(value: crate::repository::deb::package::ParsedDeb) -> Self {
        Self {
            control: value.control,
            file_size: value.file_size,
            md5: value.md5,
            sha1: value.sha1,
            sha256: value.sha256,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DebVersionRow {
    project_name: String,
    version: String,
    extra: Json<VersionData>,
}

fn metadata_from_version(data: &VersionData) -> Option<DebPackageMetadata> {
    data.extra
        .as_ref()
        .and_then(|value| serde_json::from_value::<DebPackageMetadata>(value.clone()).ok())
}

fn parse_dependency_list(raw: Option<&str>) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn dependency_display(values: &[String]) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}

async fn parse_package_metadata(bytes: Bytes) -> Result<ParsedPackage, super::DebRepositoryError> {
    let parsed = spawn_blocking(
        move || -> Result<ParsedPackage, super::DebRepositoryError> {
            let parsed = parse_deb_package(bytes)?;
            Ok(parsed.into())
        },
    )
    .await??;
    Ok(parsed)
}

async fn build_packages_index_bytes(
    body: Vec<u8>,
) -> Result<PackagesIndex, super::DebRepositoryError> {
    let index = spawn_blocking(move || PackagesIndex::new(body)).await??;
    Ok(index)
}

fn compute_hashes(bytes: &[u8]) -> HashSummary {
    let mut md5 = Md5::new();
    md5.update(bytes);
    let mut sha1 = Sha1::new();
    sha1.update(bytes);
    let mut sha256 = Sha256::new();
    sha256.update(bytes);
    HashSummary {
        size: bytes.len() as u64,
        md5: format!("{:x}", md5.finalize()),
        sha1: format!("{:x}", sha1.finalize()),
        sha256: format!("{:x}", sha256.finalize()),
    }
}

fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

fn compress_xz(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = XzEncoder::new(Vec::new(), 6);
    encoder.write_all(data)?;
    encoder.finish()
}

fn build_pool_path(component: &str, package: &str, file_name: &str) -> StoragePath {
    let mut prefix = package.chars().next().unwrap_or('_');
    if !prefix.is_ascii_alphanumeric() {
        prefix = '_';
    }
    let mut path = StoragePath::from(format!(
        "pool/{}/{}/{}",
        component,
        prefix.to_ascii_lowercase(),
        package
    ));
    if !file_name.is_empty() {
        path.push_mut(file_name);
    }
    path
}

fn not_found() -> RepoResponse {
    RepoResponse::basic_text_response(StatusCode::NOT_FOUND, "Not Found")
}
