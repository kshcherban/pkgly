use nr_core::storage::StoragePath;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::{fs::File, path::Path};
use zip::ZipArchive;

use super::PhpRepositoryError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposerRootIndex {
    #[serde(rename = "metadata-url")]
    pub metadata_url: String,
    pub packages: Vec<Value>,
}

impl ComposerRootIndex {
    pub fn new(storage: &str, repository: &str) -> Self {
        let metadata_url = format!(
            "/repositories/{storage}/{repository}/p2/%package%.json",
            storage = storage,
            repository = repository
        );
        Self {
            metadata_url,
            packages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposerPackage {
    pub name: String,
    pub version: String,
    pub raw: Value,
}

impl ComposerPackage {
    pub fn new(name: String, version: String, raw: Value) -> Self {
        Self { name, version, raw }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposerMetadataDocument {
    pub packages: HashMap<String, Vec<Value>>,
}

impl ComposerMetadataDocument {
    pub fn empty() -> Self {
        Self {
            packages: HashMap::default(),
        }
    }

    pub fn with_version(
        package: &ComposerPackage,
        dist_url: String,
        sha256: Option<String>,
    ) -> Self {
        let mut doc = Self::empty();
        doc.add_version(package, dist_url, sha256);
        doc
    }

    pub fn add_version(
        &mut self,
        package: &ComposerPackage,
        dist_url: String,
        sha256: Option<String>,
    ) {
        let mut version_entry = match package.raw.as_object() {
            Some(map) => map.clone(),
            None => Map::default(),
        };
        version_entry.insert(
            "name".into(),
            Value::String(package.name.to_ascii_lowercase()),
        );
        version_entry.insert("version".into(), Value::String(package.version.clone()));

        let mut dist = Map::new();
        dist.insert("type".into(), Value::String("zip".into()));
        dist.insert("url".into(), Value::String(dist_url));
        if let Some(sha) = sha256 {
            dist.insert("shasum".into(), Value::String(sha));
        }
        version_entry.insert("dist".into(), Value::Object(dist));

        let entry = self
            .packages
            .entry(package.name.to_ascii_lowercase())
            .or_default();
        entry.push(Value::Object(version_entry));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerDistPath {
    pub vendor: String,
    pub package: String,
    pub version: String,
    pub filename: String,
}

impl TryFrom<&StoragePath> for ComposerDistPath {
    type Error = PhpRepositoryError;

    fn try_from(path: &StoragePath) -> Result<Self, Self::Error> {
        let mut components: Vec<String> = path.clone().into_iter().map(|c| c.to_string()).collect();
        if components.first().map(|c| c.as_str()) == Some("dist") {
            components.remove(0);
        }

        // Accept both shapes:
        // 1) dist/<vendor>/<package>/<version>.zip           (common Composer upload)
        // 2) dist/<vendor>/<package>/<version>/<filename>.zip (more explicit)
        match components.len() {
            3 => {
                let vendor = components[0].clone();
                let package = components[1].clone();
                let filename = components[2].clone();
                let version = filename
                    .strip_suffix(".zip")
                    .ok_or_else(|| {
                        PhpRepositoryError::InvalidPath(format!(
                            "{path} is invalid; expected dist/<vendor>/<package>/<version>.zip"
                        ))
                    })?
                    .to_string();
                Ok(Self {
                    vendor,
                    package,
                    version,
                    filename,
                })
            }
            len if len >= 4 => {
                let vendor = components[0].clone();
                let package = components[1].clone();
                let version = components[2].clone();
                let filename = components.last().cloned().unwrap_or_default();
                Ok(Self {
                    vendor,
                    package,
                    version,
                    filename,
                })
            }
            _ => Err(PhpRepositoryError::InvalidPath(format!(
                "{path} is invalid; expected dist/<vendor>/<package>/<version>.zip"
            ))),
        }
    }
}

pub fn validate_package_against_path(
    package: &ComposerPackage,
    path: &ComposerDistPath,
) -> Result<(), PhpRepositoryError> {
    let expected_name = format!("{}/{}", path.vendor, path.package);
    if package.name.to_ascii_lowercase() != expected_name.to_ascii_lowercase() {
        return Err(PhpRepositoryError::InvalidComposer(format!(
            "name/version mismatch: composer.json has {}, path has {}",
            package.name, expected_name
        )));
    }
    if package.version != path.version {
        return Err(PhpRepositoryError::InvalidComposer(format!(
            "name/version mismatch: composer.json has {}, path has {}",
            package.version, path.version
        )));
    }
    Ok(())
}

pub fn extract_composer_from_zip(path: &Path) -> Result<ComposerPackage, PhpRepositoryError> {
    let file = File::open(path).map_err(|err| {
        PhpRepositoryError::InvalidComposer(format!("failed to open upload: {err}"))
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        PhpRepositoryError::InvalidComposer(format!("invalid zip archive: {err}"))
    })?;

    let mut composer_file = None;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|err| {
            PhpRepositoryError::InvalidComposer(format!("zip entry error: {err}"))
        })?;
        let name = file.name().to_string();
        if name.ends_with("composer.json") {
            let mut buf = String::new();
            use std::io::Read;
            file.read_to_string(&mut buf).map_err(|err| {
                PhpRepositoryError::InvalidComposer(format!("composer.json unreadable: {err}"))
            })?;
            composer_file = Some(buf);
            break;
        }
    }

    let composer_contents = composer_file.ok_or_else(|| {
        PhpRepositoryError::InvalidComposer("composer.json not found in archive".into())
    })?;
    let raw: Value = serde_json::from_str(&composer_contents).map_err(|err| {
        PhpRepositoryError::InvalidComposer(format!("composer.json is invalid JSON: {err}"))
    })?;
    let name = raw
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| PhpRepositoryError::InvalidComposer("composer.json missing name".into()))?
        .to_string();
    let version = raw
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| PhpRepositoryError::InvalidComposer("composer.json missing version".into()))?
        .to_string();

    Ok(ComposerPackage::new(name, version, raw))
}
