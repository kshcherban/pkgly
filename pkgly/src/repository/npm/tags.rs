use ahash::HashMap;
use nr_core::database::entities::project::npm::DBNpmProjectMetadata;
use sqlx::PgPool;
use uuid::Uuid;

use super::NPMRegistryError;
use crate::{app::Pkgly, repository::repo_http::RepositoryAuthentication};
use nr_core::{repository::Visibility, user::permissions::RepositoryActions};

/// Checks read permission and token scope for distribution-tag endpoints.
pub(super) async fn can_read_tags(
    authentication: &RepositoryAuthentication,
    require_auth: bool,
    visibility: Visibility,
    repository: Uuid,
    site: &Pkgly,
) -> Result<bool, NPMRegistryError> {
    // The virtual repository checks authorization before it delegates reads.
    if authentication.is_virtual_repository() {
        return Ok(true);
    }
    if !require_auth && matches!(visibility, Visibility::Public) {
        return Ok(true);
    }
    Ok(authentication
        .can_access_repository(RepositoryActions::Read, repository, &site.database)
        .await?)
}

pub(super) fn publish_tags(
    mut tags: HashMap<String, String>,
    version: &str,
) -> Result<HashMap<String, String>, NPMRegistryError> {
    if tags.is_empty() {
        tags.insert("latest".into(), version.into());
    }
    if tags
        .iter()
        .any(|(tag, target)| !valid_tag(tag) || target != version)
    {
        return Err(NPMRegistryError::InvalidDistTags);
    }
    Ok(tags)
}

pub(super) fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
        })
        && semver::VersionReq::parse(tag.strip_prefix('v').unwrap_or(tag)).is_err()
}

pub(super) fn legacy_dist_tags<'a>(
    versions: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, String> {
    let mut tags = HashMap::default();
    if let Some(version) = versions
        .into_iter()
        .find(|version| semver::Version::parse(version).is_ok_and(|parsed| parsed.pre.is_empty()))
    {
        tags.insert("latest".into(), version.into());
    }
    tags
}

pub(super) fn parse_tag_path(path: &str) -> Option<(String, Option<String>)> {
    let mut components = path.strip_prefix("-/package/")?.split('/');
    let first = components.next()?;
    let name = if first.starts_with('@') {
        format!("{first}/{}", components.next()?)
    } else {
        first.to_owned()
    };
    if name.is_empty() || components.next()? != "dist-tags" {
        return None;
    }
    let tag = components.next().map(str::to_owned);
    if components.next().is_some() || tag.as_deref().is_some_and(|tag| !valid_tag(tag)) {
        return None;
    }
    Some((name, tag))
}

pub(super) async fn load_tags(
    project: Uuid,
    database: &PgPool,
) -> Result<HashMap<String, String>, NPMRegistryError> {
    if let Some(tags) = DBNpmProjectMetadata::load_tags(project, database).await? {
        return Ok(tags);
    }
    let versions = DBNpmProjectMetadata::stable_version_candidates(project, database).await?;
    Ok(legacy_dist_tags(versions.iter().map(String::as_str)))
}

pub(super) async fn save_tags(
    project: Uuid,
    tags: HashMap<String, String>,
    fallback: HashMap<String, String>,
    database: &PgPool,
) -> Result<(), NPMRegistryError> {
    Ok(DBNpmProjectMetadata::merge_tags(project, tags, fallback, database).await?)
}

pub(super) async fn remove_tag(
    project: Uuid,
    tag: &str,
    database: &PgPool,
) -> Result<(), NPMRegistryError> {
    let fallback = load_tags(project, database).await?;
    Ok(DBNpmProjectMetadata::remove_tag(project, tag, fallback, database).await?)
}

#[cfg(test)]
mod tests;
