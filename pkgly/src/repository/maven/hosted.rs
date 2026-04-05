use std::sync::{
    Arc,
    atomic::{self, AtomicBool},
};

use derive_more::derive::Deref;
use digest::Digest;
use futures::StreamExt;
use maven_rs::pom::Pom;
use nr_core::storage::FileHashes;
use nr_core::{
    database::entities::{
        project::{DBProject, ProjectDBType, info::ProjectInfo, versions::DBProjectVersion},
        repository::DBRepository,
    },
    repository::{
        Visibility,
        config::{
            RepositoryConfigType, get_repository_config_or_default,
        },
        project::ProjectResolution,
    },
    storage::StoragePath,
    user::permissions::{HasPermissions, RepositoryActions},
    utils::base64_utils,
};
use nr_storage::{DynStorage, Storage, StorageFile, local::LocalStorage};
use parking_lot::RwLock;
use tokio::io::{AsyncWriteExt, BufWriter};
use tracing::{debug, error, event, info, instrument};
use uuid::Uuid;

use super::{
    MavenError, REPOSITORY_TYPE_ID, RepoResponse, RepositoryRequest, configs::MavenPushRules,
    utils::MavenRepositoryExt,
};
use crate::{
    app::Pkgly,
    repository::{
        Repository, RepositoryAuthConfigType, RepositoryFactoryError,
        maven::{MavenRepositoryConfigType, configs::MavenPushRulesConfigType},
        utils::RepositoryExt,
    },
};
#[derive(derive_more::Debug)]
pub struct MavenHostedInner {
    pub id: Uuid,
    pub name: String,
    pub active: AtomicBool,
    pub visibility: RwLock<Visibility>,
    pub push_rules: RwLock<MavenPushRules>,
    #[debug(skip)]
    pub storage: DynStorage,
    #[debug(skip)]
    pub site: Pkgly,
}
impl MavenHostedInner {}
#[derive(Debug, Clone, Deref)]
pub struct MavenHosted(Arc<MavenHostedInner>);
impl MavenRepositoryExt for MavenHosted {}
impl RepositoryExt for MavenHosted {}
impl MavenHosted {
    async fn stream_body_to_file_and_hashes(
        body: crate::repository::repo_http::RepositoryRequestBody,
        file: tokio::fs::File,
    ) -> Result<(u64, FileHashes), MavenError> {
        let mut writer = BufWriter::new(file);
        let mut stream = body.into_byte_stream();

        let mut md5 = md5::Md5::new();
        let mut sha1 = sha1::Sha1::new();
        let mut sha2_256 = sha2::Sha256::new();
        let mut sha3_256 = sha3::Sha3_256::new();

        let mut total_bytes = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(MavenError::from)?;
            if chunk.is_empty() {
                continue;
            }
            writer.write_all(&chunk).await.map_err(MavenError::from)?;
            total_bytes += chunk.len() as u64;

            md5.update(chunk.as_ref());
            sha1.update(chunk.as_ref());
            sha2_256.update(chunk.as_ref());
            sha3_256.update(chunk.as_ref());
        }
        writer.flush().await.map_err(MavenError::from)?;

        let hashes = FileHashes {
            md5: Some(base64_utils::encode(md5.finalize())),
            sha1: Some(base64_utils::encode(sha1.finalize())),
            sha2_256: Some(base64_utils::encode(sha2_256.finalize())),
            sha3_256: Some(base64_utils::encode(sha3_256.finalize())),
        };

        Ok((total_bytes, hashes))
    }

    async fn stream_upload_to_local_storage(
        &self,
        local: &LocalStorage,
        repository_id: Uuid,
        path: &StoragePath,
        body: crate::repository::repo_http::RepositoryRequestBody,
    ) -> Result<(u64, bool), MavenError> {
        let created = !local
            .file_exists(repository_id, path)
            .await
            .map_err(nr_storage::StorageError::from)?;
        let temp_path = StoragePath::from(format!("{path}.nr-upload-{}", Uuid::new_v4()));

        let (file, _) = local
            .open_append_handle(repository_id, &temp_path)
            .await
            .map_err(nr_storage::StorageError::from)?;

        let (total_bytes, hashes) = match Self::stream_body_to_file_and_hashes(body, file).await {
            Ok(ok) => ok,
            Err(err) => {
                let _ = local
                    .delete_file(repository_id, &temp_path)
                    .await
                    .map_err(nr_storage::StorageError::from);
                return Err(err);
            }
        };
        local.register_precomputed_hash(repository_id, path, hashes);

        let moved = local
            .move_file(repository_id, &temp_path, path)
            .await
            .map_err(nr_storage::StorageError::from)?;
        if !moved {
            let _ = local
                .delete_file(repository_id, &temp_path)
                .await
                .map_err(nr_storage::StorageError::from);
            return Err(std::io::Error::other("Temp file disappeared before rename").into());
        }

        Ok((total_bytes, created))
    }

    #[instrument(skip(self))]
    pub async fn standard_maven_deploy(
        &self,
        RepositoryRequest {
            parts: _,
            body,
            path,
            authentication,
            trace,
            ..
        }: RepositoryRequest,
    ) -> Result<RepoResponse, MavenError> {
        let user_id = if let Some(user) = authentication.get_user() {
            user.id
        } else {
            return Ok(RepoResponse::unauthorized());
        };

        {
            let push_rules = self.push_rules.read();
            if push_rules.require_pkgly_deploy {
                return Ok(RepoResponse::require_pkgly_deploy());
            }
        }
        let parent_path = path.clone().parent();
        if let Some(meta) = self
            .storage
            .get_repository_meta(self.id, &parent_path)
            .await?
        {
            let project_info = if let Some(version_id) = meta.project_version_id {
                ProjectInfo::query_from_version_id(version_id, self.site.as_ref()).await?
            } else if let Some(project_id) = meta.project_id {
                ProjectInfo::query_from_project_id(project_id, self.site.as_ref()).await?
            } else {
                None
            };
            if let Some(project) = project_info {
                trace.set_project(
                    project.project_scope,
                    project.project_name,
                    project.project_key,
                    project.project_version,
                );
            }
        };
        info!("Saving File: {}", path);

        // TODO: Validate Against Push Rules
        let is_pom = path.has_extension("pom");

        let (created, pom) = if is_pom {
            let body = body.body_as_bytes().await?;
            trace.metrics.project_write_bytes(body.len() as u64);
            let pom: Pom = self.parse_pom(body.to_vec())?;

            if let DynStorage::Local(local) = &self.storage {
                let hashes = nr_storage::generate_from_bytes(body.as_ref());
                local.register_precomputed_hash(self.id, &path, hashes);
            }

            let (_size, created) = self.storage.save_file(self.id, body.into(), &path).await?;
            (created, Some(pom))
        } else if let DynStorage::Local(local) = &self.storage {
            let (bytes_written, created) = self
                .stream_upload_to_local_storage(local, self.id, &path, body)
                .await?;
            trace.metrics.project_write_bytes(bytes_written);
            (created, None)
        } else {
            let body = body.body_as_bytes().await?;
            trace.metrics.project_write_bytes(body.len() as u64);
            let (_size, created) = self.storage.save_file(self.id, body.into(), &path).await?;
            (created, None)
        };

        // Trigger Push Event if it is the .pom file
        let save_path = format!(
            "/repositories/{}/{}/{}",
            self.storage.storage_config().storage_config.storage_name,
            self.name,
            path
        );
        if let Some(pom) = pom {
            debug!(?pom, "Parsed POM File");
            self.post_pom_upload(path.clone(), Some(user_id), pom).await;
        };
        Ok(RepoResponse::put_response(created, save_path))
    }
    pub async fn load(
        repository: DBRepository,
        storage: DynStorage,
        site: Pkgly,
    ) -> Result<Self, RepositoryFactoryError> {
        let push_rules_db = get_repository_config_or_default::<
            MavenPushRulesConfigType,
            MavenPushRules,
        >(repository.id, site.as_ref())
        .await?;
        debug!("Loaded Push Rules Config: {:?}", push_rules_db);

        let active = AtomicBool::new(repository.active);
        let inner = MavenHostedInner {
            id: repository.id,
            name: repository.name.into(),
            active,
            visibility: RwLock::new(repository.visibility),
            push_rules: RwLock::new(push_rules_db.value.0),
            storage,
            site,
        };
        Ok(Self(Arc::new(inner)))
    }
}
impl Repository for MavenHosted {
    type Error = MavenError;
    #[inline(always)]
    fn site(&self) -> Pkgly {
        self.0.site.clone()
    }
    #[inline(always)]
    fn get_storage(&self) -> nr_storage::DynStorage {
        self.0.storage.clone()
    }
    #[inline(always)]
    fn visibility(&self) -> Visibility {
        *self.visibility.read()
    }
    #[inline(always)]
    fn get_type(&self) -> &'static str {
        REPOSITORY_TYPE_ID
    }
    fn full_type(&self) -> &'static str {
        "maven/hosted"
    }
    #[inline(always)]
    fn name(&self) -> String {
        self.0.name.clone()
    }
    #[inline(always)]
    fn id(&self) -> Uuid {
        self.0.id
    }
    #[inline(always)]
    fn is_active(&self) -> bool {
        self.active.load(atomic::Ordering::Relaxed)
    }

    fn config_types(&self) -> Vec<&str> {
        vec![
            MavenPushRulesConfigType::get_type_static(),
            MavenRepositoryConfigType::get_type_static(),
            RepositoryAuthConfigType::get_type_static(),
        ]
    }
    #[instrument(fields(repository_type = "maven/hosted"))]
    async fn reload(&self) -> Result<(), RepositoryFactoryError> {
        let Some(is_active) = DBRepository::get_active_by_id(self.id, self.site.as_ref()).await?
        else {
            error!("Failed to get repository");
            self.0.active.store(false, atomic::Ordering::Relaxed);
            return Ok(());
        };
        self.0.active.store(is_active, atomic::Ordering::Relaxed);

        let push_rules_db = get_repository_config_or_default::<
            MavenPushRulesConfigType,
            MavenPushRules,
        >(self.id, self.site.as_ref())
        .await?;

        {
            let mut push_rules = self.push_rules.write();
            *push_rules = push_rules_db.value.0;
        }

        Ok(())
    }
    async fn handle_get(
        &self,
        RepositoryRequest {
            parts: _,
            path,
            authentication,
            trace,
            ..
        }: RepositoryRequest,
    ) -> Result<RepoResponse, MavenError> {
        if let Some(err) = self.check_read(&authentication).await? {
            return Ok(err);
        }
        let file = self.0.storage.open_file(self.id, &path).await?;
        if let Some(StorageFile::File { meta, .. }) = &file {
            trace.metrics.project_access_bytes(meta.file_type.file_size);
            let parent = path.parent();
            let meta = self
                .0
                .storage
                .get_repository_meta(self.id, &parent)
                .await?
                .unwrap_or_default();
            let project_info = if let Some(version_id) = meta.project_version_id {
                ProjectInfo::query_from_version_id(version_id, self.site.as_ref()).await?
            } else if let Some(project_id) = meta.project_id {
                ProjectInfo::query_from_project_id(project_id, self.site.as_ref()).await?
            } else {
                None
            };
            if let Some(project) = project_info {
                trace.set_project(
                    project.project_scope,
                    project.project_name,
                    project.project_key,
                    project.project_version,
                );
            }
        }
        return self.indexing_check_option(file, &authentication).await;
    }
    async fn handle_head(
        &self,
        RepositoryRequest {
            parts: _,
            path,
            authentication,
            ..
        }: RepositoryRequest,
    ) -> Result<RepoResponse, MavenError> {
        if let Some(err) = self.check_read(&authentication).await? {
            return Ok(err);
        }
        let file = self.storage.get_file_information(self.id, &path).await?;
        return self.indexing_check_option(file, &authentication).await;
    }
    async fn handle_put(&self, request: RepositoryRequest) -> Result<RepoResponse, MavenError> {
        info!("Handling PUT Request for Repository: {}", self.id);
        {
            let push_rules = self.push_rules.read();
            if push_rules.must_use_auth_token_for_push && !request.authentication.has_auth_token() {
                info!("Repository requires an auth token for push");
                return Ok(RepoResponse::require_auth_token());
            }
        }

        let Some(user) = request
            .authentication
            .get_user_if_has_action(RepositoryActions::Write, self.id, self.site.as_ref())
            .await?
        else {
            info!("No acceptable user authentication provided");
            return Ok(RepoResponse::unauthorized());
        };
        if !user
            .has_action(RepositoryActions::Write, self.id, self.site.as_ref())
            .await?
        {
            info!(?self.id, ?user, "User does not have write permissions");
            return Ok(RepoResponse::forbidden());
        }

        let Some(pkgly_deploy_version) = request.get_pkgly_deploy_header()? else {
            return self.standard_maven_deploy(request).await;
        };
        info!(?pkgly_deploy_version, "Handling Pkgly Deploy Version");

        Ok(RepoResponse::unsupported_method_response(
            request.parts.method,
            self.get_type(),
        ))
    }
    async fn handle_post(&self, request: RepositoryRequest) -> Result<RepoResponse, MavenError> {
        let Some(pkgly_deploy_version) = request.get_pkgly_deploy_header()? else {
            return Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ));
        };
        info!(?pkgly_deploy_version, "Handling Pkgly Deploy Version");
        todo!()
    }
    #[instrument(fields(repository_type = "maven/hosted"))]
    async fn resolve_project_and_version_for_path(
        &self,
        path: &StoragePath,
    ) -> Result<ProjectResolution, MavenError> {
        let path_as_string = path.to_string();
        event!(
            tracing::Level::DEBUG,
            "Resolving Project and Version for Path: {}",
            path_as_string
        );
        let Some(meta) = self.storage.get_repository_meta(self.id, path).await? else {
            return Ok(ProjectResolution::default());
        };
        if let Some(project_id) = meta.project_id {
            let version_id = meta.project_version_id;
            event!(
                tracing::Level::DEBUG,
                ?project_id,
                ?version_id,
                "Found Project ID in Meta"
            );

            return Ok(ProjectResolution {
                project_id: Some(project_id),
                version_id,
            });
        }
        event!(
            tracing::Level::DEBUG,
            "No Project ID in Meta looking project dirs in DB"
        );
        let version =
            DBProjectVersion::find_ids_by_version_dir(&path_as_string, self.id, self.site.as_ref())
                .await?;
        if let Some(version) = version {
            event!(
                tracing::Level::DEBUG,
                "Found Project Version in DB Versions: {:?}",
                version
            );
            return Ok(version.into());
        }
        event!(
            tracing::Level::DEBUG,
            "No Project Version in DB looking for Project dir"
        );
        if let Some(project) =
            DBProject::find_by_project_directory(&path_as_string, self.id, self.site.as_ref())
                .await?
        {
            return Ok(ProjectResolution {
                project_id: Some(project.id),
                version_id: None,
            });
        }

        Ok(ProjectResolution::default())
    }
}
