use crate::repository::Repository;
use nr_core::{
    database::entities::project::{DBProject, ProjectDBType, versions::DBProjectVersion},
    storage::StoragePath,
};
use tracing::{info, instrument};

use super::{NPMRegistryError, types::request::PublishVersion};

pub mod npm_time {
    use chrono::{DateTime, FixedOffset};

    pub fn format_date_time(date_time: &DateTime<FixedOffset>) -> String {
        date_time.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string()
    }
}
pub trait NpmRegistryExt: Repository {
    #[instrument(skip(self, release), fields(repository_id = %self.id(), package = %release.name, version = %release.version))]
    async fn get_or_create_project(
        &self,
        save_path: &StoragePath,
        release: &PublishVersion,
    ) -> Result<DBProject, NPMRegistryError> {
        let database = &self.site().database;
        if let Some(project) =
            DBProject::find_by_project_key(&release.name.to_string(), self.id(), database).await?
        {
            return Ok(project);
        }
        let project = release
            .new_project(save_path.to_string(), self.id())?
            .insert(database)
            .await?;
        info!(project_id = %project.id, "Created new project");
        Ok(project)
    }

    #[instrument(skip(self, project, release), fields(repository_id = %self.id(), project_id = %project.id, package = %release.name, version = %release.version))]
    async fn create_version_if_missing(
        &self,
        publisher: i32,
        save_path: &StoragePath,
        project: &DBProject,
        release: &PublishVersion,
    ) -> Result<(), NPMRegistryError> {
        let database = &self.site().database;
        if DBProjectVersion::find_by_version_and_project(&release.version, project.id, database)
            .await?
            .is_none()
        {
            release
                .new_version(project.id, self.id(), save_path.to_string(), publisher)?
                .insert(database)
                .await?;
        }
        Ok(())
    }
}

pub(crate) fn audit_action(
    method: &http::Method,
    path: &StoragePath,
    headers: &http::HeaderMap,
) -> Option<&'static str> {
    if super::tags::parse_tag_path(&path.to_string()).is_some() {
        return match *method {
            http::Method::GET | http::Method::HEAD => Some("package.dist_tag.list"),
            http::Method::PUT => Some("package.dist_tag.set"),
            http::Method::DELETE => Some("package.dist_tag.delete"),
            _ => None,
        };
    }
    if *method == http::Method::PUT
        && headers
            .get(super::types::NPM_COMMAND_HEADER)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|command| matches!(command, "deprecate" | "undeprecate"))
    {
        return Some("package.deprecation.update");
    }
    None
}

/// Extracts a complete semantic version from an npm tarball filename.
pub(crate) fn extract_version_from_file(file: &str, package: &str) -> Option<String> {
    let name = package.rsplit('/').next()?;
    let version = file
        .strip_prefix(name)?
        .strip_prefix('-')?
        .strip_suffix(".tgz")?;
    semver::Version::parse(version)
        .ok()
        .map(|_| version.to_owned())
}

#[cfg(test)]
mod tests;
