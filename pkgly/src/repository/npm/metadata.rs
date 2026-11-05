use ahash::HashMap;
use nr_core::database::entities::project::npm::DBNpmProjectMetadata;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::{NPMRegistryError, types::request::PublishVersion};

pub(super) fn normalize_readme(version: &mut PublishVersion, root: &HashMap<String, Value>) {
    for (field, name) in [
        (&mut version.readme, "readme"),
        (&mut version.readme_file_name, "readmeFilename"),
    ] {
        if field.is_empty()
            && let Some(value) = root.get(name).and_then(Value::as_str)
        {
            *field = value.to_owned();
        }
    }
}

pub(super) fn package_fields(version: Option<&PublishVersion>) -> HashMap<String, Value> {
    let Some(version) = version else {
        return HashMap::default();
    };
    let mut fields = HashMap::default();
    for name in [
        "author",
        "bugs",
        "contributors",
        "description",
        "homepage",
        "keywords",
        "license",
        "maintainers",
        "repository",
    ] {
        if let Some(value) = version.extra.get(name) {
            fields.insert(name.into(), value.clone());
        }
    }
    fields.insert("readme".into(), Value::String(version.readme.clone()));
    fields.insert(
        "readmeFilename".into(),
        Value::String(version.readme_file_name.clone()),
    );
    fields
}

pub(super) fn deprecation_updates(
    body: &Value,
    name: &str,
) -> Result<HashMap<String, String>, NPMRegistryError> {
    if body.get("name").and_then(Value::as_str) != Some(name) {
        return Err(NPMRegistryError::InvalidMetadataUpdate);
    }
    let versions = body
        .get("versions")
        .and_then(Value::as_object)
        .ok_or(NPMRegistryError::InvalidMetadataUpdate)?;
    let mut updates = HashMap::default();
    for (version, metadata) in versions {
        if semver::Version::parse(version).is_err() || !metadata.is_object() {
            return Err(NPMRegistryError::InvalidMetadataUpdate);
        }
        if let Some(message) = metadata.get("deprecated") {
            let message = message
                .as_str()
                .ok_or(NPMRegistryError::InvalidMetadataUpdate)?;
            updates.insert(version.clone(), message.to_owned());
        }
    }
    if updates.is_empty() {
        return Err(NPMRegistryError::InvalidMetadataUpdate);
    }
    Ok(updates)
}

pub(super) async fn save_deprecations(
    project: Uuid,
    updates: HashMap<String, String>,
    database: &PgPool,
) -> Result<bool, NPMRegistryError> {
    Ok(DBNpmProjectMetadata::update_deprecations(project, updates, database).await?)
}

#[cfg(test)]
mod tests;
