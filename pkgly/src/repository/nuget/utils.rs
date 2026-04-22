use std::io::Cursor;

use axum::response::Response;
use bytes::Bytes;
use chrono::Utc;
use http::{
    Method,
    header::{CONTENT_TYPE, HOST},
    request::Parts,
};
use nr_core::{
    database::entities::project::{
        DBProject, NewProject, ProjectDBType,
        versions::{DBProjectVersion, NewVersion, UpdateProjectVersion},
    },
    repository::project::{Author, ProjectResolution, ProxyArtifactMeta, ReleaseType, VersionData},
    storage::StoragePath,
    user::permissions::RepositoryActions,
};
use nr_storage::{DynStorage, FileContent, Storage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::warn;
use uuid::Uuid;
use zip::ZipArchive;

use super::NugetError;
use crate::{
    app::Pkgly,
    repository::{
        RepoResponse,
        repo_http::{RepositoryAuthentication, RepositoryRequestBody},
        utils::can_read_repository_with_auth,
    },
    utils::ResponseBuilder,
};

pub const REPOSITORY_TYPE_ID: &str = "nuget";
#[derive(Debug, Clone)]
pub struct ParsedNugetPackage {
    pub package_id: String,
    pub lower_id: String,
    pub version: String,
    pub lower_version: String,
    pub nuspec_xml: String,
    pub metadata: NugetMetadata,
    pub nupkg_bytes: Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NugetMetadata {
    pub id: String,
    pub version: String,
    pub authors: Option<String>,
    pub description: Option<String>,
    pub summary: Option<String>,
    pub title: Option<String>,
    pub project_url: Option<String>,
    pub license_url: Option<String>,
    pub icon_url: Option<String>,
    pub tags: Option<String>,
    pub require_license_acceptance: Option<bool>,
    #[serde(default)]
    pub dependency_groups: Vec<NugetDependencyGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NugetDependencyGroup {
    pub target_framework: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<NugetDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NugetDependency {
    pub id: String,
    pub range: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostedVersionRecord {
    pub version: String,
    pub lower_version: String,
    pub published: chrono::DateTime<chrono::FixedOffset>,
    pub metadata: NugetMetadata,
}

#[derive(Debug, Clone)]
pub struct RegistrationLeaf {
    pub lower_version: String,
    pub published: Option<String>,
    pub package_content: String,
    pub catalog_entry: Value,
}

#[derive(Debug, Deserialize)]
struct NuspecPackage {
    metadata: NuspecMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NuspecMetadata {
    id: String,
    version: String,
    authors: Option<String>,
    description: Option<String>,
    summary: Option<String>,
    title: Option<String>,
    project_url: Option<String>,
    license_url: Option<String>,
    icon_url: Option<String>,
    tags: Option<String>,
    require_license_acceptance: Option<bool>,
    dependencies: Option<NuspecDependencies>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NuspecDependencies {
    Flat {
        #[serde(default)]
        dependency: Vec<NuspecDependency>,
    },
    Grouped {
        #[serde(default)]
        group: Vec<NuspecDependencyGroup>,
    },
}

#[derive(Debug, Deserialize)]
struct NuspecDependencyGroup {
    #[serde(rename = "@targetFramework")]
    target_framework: Option<String>,
    #[serde(default)]
    dependency: Vec<NuspecDependency>,
}

#[derive(Debug, Deserialize)]
struct NuspecDependency {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@version")]
    version: Option<String>,
}

pub fn lower_id(value: &str) -> String {
    value.to_ascii_lowercase()
}

pub fn normalize_version(value: &str) -> String {
    value
        .split('+')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase()
}

pub fn flatcontainer_index_path(package_id: &str) -> StoragePath {
    StoragePath::from(format!(
        "v3/flatcontainer/{}/index.json",
        lower_id(package_id)
    ))
}

pub fn flatcontainer_package_path(package_id: &str, version: &str) -> StoragePath {
    let lower_id = lower_id(package_id);
    let lower_version = normalize_version(version);
    StoragePath::from(format!(
        "v3/flatcontainer/{lower_id}/{lower_version}/{lower_id}.{lower_version}.nupkg"
    ))
}

pub fn flatcontainer_nuspec_path(package_id: &str, version: &str) -> StoragePath {
    let lower_id = lower_id(package_id);
    let lower_version = normalize_version(version);
    StoragePath::from(format!(
        "v3/flatcontainer/{lower_id}/{lower_version}/{lower_id}.nuspec"
    ))
}

pub fn registration_index_path(package_id: &str) -> StoragePath {
    StoragePath::from(format!(
        "v3/registration/{}/index.json",
        lower_id(package_id)
    ))
}

pub fn registration_leaf_path(package_id: &str, version: &str) -> StoragePath {
    StoragePath::from(format!(
        "v3/registration/{}/{}.json",
        lower_id(package_id),
        normalize_version(version)
    ))
}

pub fn base_repository_path(storage_name: &str, repository_name: &str) -> String {
    format!("/repositories/{storage_name}/{repository_name}")
}

pub fn external_repository_base(
    site: &Pkgly,
    parts: Option<&Parts>,
    repository_path: &str,
) -> String {
    let instance = site.inner.instance.lock();
    let scheme = if instance.is_https { "https" } else { "http" };

    if let Some(parts) = parts {
        if let Some(host) = parts
            .headers
            .get(HOST)
            .and_then(|value| value.to_str().ok())
        {
            return format!("{scheme}://{host}{repository_path}");
        }
    }

    if !instance.app_url.is_empty() {
        return format!(
            "{}{}",
            instance.app_url.trim_end_matches('/'),
            repository_path
        );
    }

    format!("{scheme}://localhost:6742{repository_path}")
}

pub fn service_index(base_url: &str, allow_publish: bool) -> Value {
    let mut resources = vec![
        json!({
            "@id": format!("{base_url}/v3/flatcontainer"),
            "@type": "PackageBaseAddress/3.0.0",
        }),
        json!({
            "@id": format!("{base_url}/v3/registration"),
            "@type": "RegistrationsBaseUrl",
        }),
    ];
    if allow_publish {
        resources.push(json!({
            "@id": format!("{base_url}/api/v2/package"),
            "@type": "PackagePublish/2.0.0",
        }));
    }
    json!({
        "version": "3.0.0",
        "resources": resources,
    })
}

pub fn json_response(method: &Method, value: &Value) -> Response {
    if *method == Method::HEAD {
        ResponseBuilder::ok()
            .header(CONTENT_TYPE, "application/json")
            .empty()
    } else {
        ResponseBuilder::ok().json(value)
    }
}

pub fn xml_response(method: &Method, xml: String) -> Response {
    if *method == Method::HEAD {
        ResponseBuilder::ok()
            .header(CONTENT_TYPE, "application/xml")
            .empty()
    } else {
        ResponseBuilder::ok()
            .header(CONTENT_TYPE, "application/xml")
            .body(xml)
    }
}

pub fn build_registration_index(
    base_url: &str,
    package_id: &str,
    leaves: &[RegistrationLeaf],
) -> Value {
    let lower_id = lower_id(package_id);
    let mut items = Vec::new();
    for leaf in leaves {
        items.push(json!({
            "@id": format!("{base_url}/v3/registration/{lower_id}/{}.json", leaf.lower_version),
            "catalogEntry": leaf.catalog_entry,
            "packageContent": leaf.package_content,
        }));
    }

    let lower = leaves
        .first()
        .map(|leaf| leaf.lower_version.clone())
        .unwrap_or_default();
    let upper = leaves
        .last()
        .map(|leaf| leaf.lower_version.clone())
        .unwrap_or_default();

    json!({
        "count": 1,
        "items": [
            {
                "@id": format!("{base_url}/v3/registration/{lower_id}/page/{lower}/{upper}.json"),
                "count": leaves.len(),
                "items": items,
                "lower": lower,
                "upper": upper,
                "parent": format!("{base_url}/v3/registration/{lower_id}/index.json"),
            }
        ]
    })
}

pub fn build_registration_leaf(base_url: &str, package_id: &str, leaf: &RegistrationLeaf) -> Value {
    let lower_id = lower_id(package_id);
    json!({
        "@id": format!("{base_url}/v3/registration/{lower_id}/{}.json", leaf.lower_version),
        "catalogEntry": leaf.catalog_entry,
        "listed": true,
        "packageContent": leaf.package_content,
        "published": leaf.published,
        "registration": format!("{base_url}/v3/registration/{lower_id}/index.json"),
    })
}

pub fn collect_registration_leaves(value: &Value) -> Vec<RegistrationLeaf> {
    let mut leaves = Vec::new();
    collect_registration_leaves_inner(value, &mut leaves);
    leaves.sort_by(|a, b| a.lower_version.cmp(&b.lower_version));
    leaves.dedup_by(|a, b| a.lower_version == b.lower_version);
    leaves
}

fn collect_registration_leaves_inner(value: &Value, leaves: &mut Vec<RegistrationLeaf>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_registration_leaves_inner(item, leaves);
            }
        }
        Value::Object(map) => {
            if let Some(catalog_entry) = map.get("catalogEntry") {
                let package_content = map
                    .get("packageContent")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let version = catalog_entry
                    .get("version")
                    .and_then(Value::as_str)
                    .or_else(|| map.get("version").and_then(Value::as_str))
                    .unwrap_or_default()
                    .to_string();
                if !version.is_empty() && !package_content.is_empty() {
                    leaves.push(RegistrationLeaf {
                        lower_version: normalize_version(&version),
                        published: catalog_entry
                            .get("published")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        package_content,
                        catalog_entry: catalog_entry.clone(),
                    });
                }
            }

            for nested in map.values() {
                collect_registration_leaves_inner(nested, leaves);
            }
        }
        _ => {}
    }
}

pub fn rewrite_upstream_urls(
    value: &mut Value,
    registration_base: &str,
    flatcontainer_base: &str,
    publish_base: Option<&str>,
    local_base: &str,
) {
    match value {
        Value::String(text) => {
            if let Some(stripped) = text.strip_prefix(registration_base) {
                *text = format!("{local_base}/v3/registration{stripped}");
            } else if let Some(stripped) = text.strip_prefix(flatcontainer_base) {
                *text = format!("{local_base}/v3/flatcontainer{stripped}");
            } else if let Some(publish_base) = publish_base {
                if let Some(stripped) = text.strip_prefix(publish_base) {
                    *text = format!("{local_base}/api/v2/package{stripped}");
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_upstream_urls(
                    item,
                    registration_base,
                    flatcontainer_base,
                    publish_base,
                    local_base,
                );
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                rewrite_upstream_urls(
                    item,
                    registration_base,
                    flatcontainer_base,
                    publish_base,
                    local_base,
                );
            }
        }
        _ => {}
    }
}

pub async fn read_storage_bytes(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &StoragePath,
) -> Result<Option<Vec<u8>>, NugetError> {
    let Some(file) = storage.open_file(repository_id, path).await? else {
        return Ok(None);
    };
    let Some((reader, meta)) = file.file() else {
        return Ok(None);
    };
    let size_hint = usize::try_from(meta.file_type().file_size).unwrap_or(0);
    Ok(Some(reader.read_to_vec(size_hint).await?))
}

pub fn parse_published_package(bytes: Bytes) -> Result<ParsedNugetPackage, NugetError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes.clone()))?;
    let mut nuspec_xml: Option<String> = None;

    for idx in 0..archive.len() {
        let mut file = archive.by_index(idx)?;
        let name = file.name().to_string();
        if name.ends_with(".nuspec") {
            let mut xml = String::new();
            std::io::Read::read_to_string(&mut file, &mut xml)?;
            nuspec_xml = Some(xml);
            break;
        }
    }

    let nuspec_xml =
        nuspec_xml.ok_or_else(|| NugetError::InvalidPackage("Missing .nuspec".into()))?;
    let metadata = parse_nuspec(&nuspec_xml)?;

    Ok(ParsedNugetPackage {
        package_id: metadata.id.clone(),
        lower_id: lower_id(&metadata.id),
        version: metadata.version.clone(),
        lower_version: normalize_version(&metadata.version),
        nuspec_xml,
        metadata,
        nupkg_bytes: bytes,
    })
}

pub fn parse_nuspec(xml: &str) -> Result<NugetMetadata, NugetError> {
    let package: NuspecPackage = maven_rs::quick_xml::de::from_str(xml)?;
    let dependencies = match package.metadata.dependencies {
        Some(NuspecDependencies::Flat { dependency }) => vec![NugetDependencyGroup {
            target_framework: None,
            dependencies: dependency
                .into_iter()
                .map(|entry| NugetDependency {
                    id: entry.id,
                    range: entry.version,
                })
                .collect(),
        }],
        Some(NuspecDependencies::Grouped { group }) => group
            .into_iter()
            .map(|entry| NugetDependencyGroup {
                target_framework: entry.target_framework,
                dependencies: entry
                    .dependency
                    .into_iter()
                    .map(|dependency| NugetDependency {
                        id: dependency.id,
                        range: dependency.version,
                    })
                    .collect(),
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(NugetMetadata {
        id: package.metadata.id,
        version: package.metadata.version,
        authors: package.metadata.authors,
        description: package.metadata.description,
        summary: package.metadata.summary,
        title: package.metadata.title,
        project_url: package.metadata.project_url,
        license_url: package.metadata.license_url,
        icon_url: package.metadata.icon_url,
        tags: package.metadata.tags,
        require_license_acceptance: package.metadata.require_license_acceptance,
        dependency_groups: dependencies,
    })
}

pub async fn upsert_hosted_metadata(
    site: &Pkgly,
    repository_id: Uuid,
    package: &ParsedNugetPackage,
    publisher: Option<i32>,
) -> Result<(), NugetError> {
    let project_key = package.package_id.clone();
    let project_path = StoragePath::from(format!("v3/flatcontainer/{}", package.lower_id));
    let project = if let Some(project) =
        DBProject::find_by_project_key(&project_key, repository_id, site.as_ref()).await?
    {
        project
    } else {
        let new_project = NewProject {
            scope: None,
            project_key,
            name: package.package_id.clone(),
            description: package.metadata.description.clone(),
            repository: repository_id,
            storage_path: project_path.to_string(),
        };
        new_project.insert(site.as_ref()).await?
    };

    if DBProjectVersion::find_by_version_and_project(&package.version, project.id, &site.database)
        .await?
        .is_some()
    {
        return Ok(());
    }

    let version_path = StoragePath::from(format!(
        "v3/flatcontainer/{}/{}/",
        package.lower_id, package.lower_version
    ));
    let new_version = NewVersion {
        project_id: project.id,
        repository_id,
        version: package.version.clone(),
        release_type: ReleaseType::release_type_from_version(&package.version),
        version_path: version_path.to_string(),
        publisher,
        version_page: None,
        extra: version_data_from_package(package)?,
    };
    new_version.insert(&site.database).await?;
    Ok(())
}

pub async fn upsert_proxy_metadata(
    site: &Pkgly,
    repository_id: Uuid,
    package: &ParsedNugetPackage,
    cache_path: &StoragePath,
    upstream_url: Option<&str>,
    size: u64,
) -> Result<(), NugetError> {
    let project_key = package.package_id.clone();
    let project_path = StoragePath::from(format!("v3/flatcontainer/{}", package.lower_id));
    let project = if let Some(project) =
        DBProject::find_by_project_key(&project_key, repository_id, site.as_ref()).await?
    {
        project
    } else {
        let new_project = NewProject {
            scope: None,
            project_key,
            name: package.package_id.clone(),
            description: package.metadata.description.clone(),
            repository: repository_id,
            storage_path: project_path.to_string(),
        };
        new_project.insert(site.as_ref()).await?
    };

    let mut version_data = VersionData {
        description: package.metadata.description.clone(),
        website: package.metadata.project_url.clone(),
        authors: nuget_authors(&package.metadata.authors),
        ..Default::default()
    };
    let mut meta = ProxyArtifactMeta::builder(
        package.package_id.clone(),
        package.package_id.clone(),
        cache_path.to_string(),
    )
    .version(package.version.clone())
    .size(size)
    .fetched_at(Utc::now());
    if let Some(upstream_url) = upstream_url {
        meta = meta.upstream_url(upstream_url.to_string());
    }
    version_data.set_proxy_artifact(&meta.build())?;

    if let Some(existing) =
        DBProjectVersion::find_by_version_and_project(&package.version, project.id, &site.database)
            .await?
    {
        UpdateProjectVersion {
            extra: Some(version_data),
            ..Default::default()
        }
        .update(existing.id, &site.database)
        .await
        .map_err(|err| NugetError::Other(Box::new(err)))?;
        return Ok(());
    }

    let new_version = NewVersion {
        project_id: project.id,
        repository_id,
        version: package.version.clone(),
        release_type: ReleaseType::release_type_from_version(&package.version),
        version_path: cache_path.to_string(),
        publisher: None,
        version_page: None,
        extra: version_data,
    };
    new_version.insert(&site.database).await?;
    Ok(())
}

fn version_data_from_package(package: &ParsedNugetPackage) -> Result<VersionData, NugetError> {
    Ok(VersionData {
        description: package.metadata.description.clone(),
        website: package.metadata.project_url.clone(),
        authors: nuget_authors(&package.metadata.authors),
        extra: Some(serde_json::to_value(&package.metadata)?),
        ..Default::default()
    })
}

fn nuget_authors(value: &Option<String>) -> Vec<Author> {
    value
        .clone()
        .map(|authors| {
            authors
                .split(',')
                .map(|entry| Author {
                    name: Some(entry.trim().to_string()),
                    email: None,
                    website: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub async fn list_hosted_versions(
    site: &Pkgly,
    storage: &DynStorage,
    repository_id: Uuid,
    package_id: &str,
) -> Result<Vec<HostedVersionRecord>, NugetError> {
    let Some(project) =
        DBProject::find_by_project_key(package_id, repository_id, site.as_ref()).await?
    else {
        return Ok(Vec::new());
    };
    let versions = DBProjectVersion::get_all_versions(project.id, site.as_ref()).await?;
    let mut out = Vec::with_capacity(versions.len());
    for version in versions {
        let nuspec_path = flatcontainer_nuspec_path(package_id, &version.version);
        let Some(bytes) = read_storage_bytes(storage, repository_id, &nuspec_path).await? else {
            continue;
        };
        let xml = String::from_utf8(bytes)?;
        let metadata = parse_nuspec(&xml)?;
        out.push(HostedVersionRecord {
            lower_version: normalize_version(&version.version),
            version: version.version,
            published: version.created_at,
            metadata,
        });
    }
    out.sort_by(|a, b| a.lower_version.cmp(&b.lower_version));
    Ok(out)
}

pub async fn find_hosted_version(
    site: &Pkgly,
    storage: &DynStorage,
    repository_id: Uuid,
    package_id: &str,
    version: &str,
) -> Result<Option<HostedVersionRecord>, NugetError> {
    let versions = list_hosted_versions(site, storage, repository_id, package_id).await?;
    Ok(versions
        .into_iter()
        .find(|entry| entry.lower_version == normalize_version(version)))
}

pub fn hosted_leaf(
    base_url: &str,
    package_id: &str,
    version: &HostedVersionRecord,
) -> RegistrationLeaf {
    let package_id_lower = lower_id(package_id);
    let package_content = format!(
        "{base_url}/v3/flatcontainer/{package_id_lower}/{}/{package_id_lower}.{}.nupkg",
        version.lower_version, version.lower_version
    );
    let mut catalog_entry = json!({
        "@id": format!("{base_url}/v3/registration/{package_id_lower}/{}.json", version.lower_version),
        "id": version.metadata.id,
        "version": version.version,
        "listed": true,
        "packageContent": package_content,
        "published": version.published.to_rfc3339(),
    });
    if let Some(authors) = &version.metadata.authors {
        catalog_entry["authors"] = json!(authors);
    }
    if let Some(description) = &version.metadata.description {
        catalog_entry["description"] = json!(description);
    }
    if let Some(summary) = &version.metadata.summary {
        catalog_entry["summary"] = json!(summary);
    }
    if let Some(title) = &version.metadata.title {
        catalog_entry["title"] = json!(title);
    }
    if let Some(project_url) = &version.metadata.project_url {
        catalog_entry["projectUrl"] = json!(project_url);
    }
    if let Some(license_url) = &version.metadata.license_url {
        catalog_entry["licenseUrl"] = json!(license_url);
    }
    if let Some(icon_url) = &version.metadata.icon_url {
        catalog_entry["iconUrl"] = json!(icon_url);
    }
    if let Some(tags) = &version.metadata.tags {
        catalog_entry["tags"] = json!(tags);
    }
    if let Some(require_license_acceptance) = version.metadata.require_license_acceptance {
        catalog_entry["requireLicenseAcceptance"] = json!(require_license_acceptance);
    }
    if !version.metadata.dependency_groups.is_empty() {
        catalog_entry["dependencyGroups"] = json!(version.metadata.dependency_groups.iter().map(|group| {
            json!({
                "targetFramework": group.target_framework,
                "dependencies": group.dependencies.iter().map(|dependency| {
                    json!({
                        "id": dependency.id,
                        "range": dependency.range,
                        "registration": format!("{base_url}/v3/registration/{}/index.json", lower_id(&dependency.id)),
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>());
    }
    RegistrationLeaf {
        lower_version: version.lower_version.clone(),
        published: Some(version.published.to_rfc3339()),
        package_content,
        catalog_entry,
    }
}

pub async fn can_read_with_auth(
    authentication: &RepositoryAuthentication,
    visibility: nr_core::repository::Visibility,
    repository_id: Uuid,
    site: &Pkgly,
) -> Result<bool, NugetError> {
    if authentication.is_virtual_repository() {
        return Ok(true);
    }
    Ok(can_read_repository_with_auth(
        authentication,
        visibility,
        repository_id,
        site.as_ref(),
        &site.get_repository_auth_config(repository_id).await?,
    )
    .await?)
}

pub async fn first_multipart_bytes(
    body: RepositoryRequestBody,
    content_type: &str,
) -> Result<Bytes, NugetError> {
    let boundary = multer::parse_boundary(content_type)?;
    let bytes = body.body_as_bytes().await?;
    let stream = futures::stream::once(async move { Ok::<Bytes, multer::Error>(bytes) });
    let mut multipart = multer::Multipart::new(stream, boundary);
    let Some(field) = multipart.next_field().await? else {
        return Err(NugetError::InvalidPackage(
            "Missing multipart payload".into(),
        ));
    };
    Ok(field.bytes().await?)
}

pub async fn push_requires_write(
    authentication: &RepositoryAuthentication,
    repository_id: Uuid,
    site: &Pkgly,
) -> Result<Option<i32>, NugetError> {
    Ok(authentication
        .get_user_if_has_action(RepositoryActions::Write, repository_id, site.as_ref())
        .await?
        .map(|user| user.id))
}

pub async fn resolve_project_version(
    repository_id: Uuid,
    path: &StoragePath,
    database: &sqlx::PgPool,
) -> Result<ProjectResolution, NugetError> {
    let path_str = path.to_string();
    let parts: Vec<_> = path_str.split('/').collect();
    if parts.len() < 5 || parts[0] != "v3" || parts[1] != "flatcontainer" {
        return Ok(ProjectResolution::default());
    }
    let version_dir = format!("v3/flatcontainer/{}/{}/", parts[2], parts[3]);
    let Some(ids) =
        DBProjectVersion::find_ids_by_version_dir(&version_dir, repository_id, database).await?
    else {
        return Ok(ProjectResolution::default());
    };
    Ok(ids.into())
}

pub async fn response_from_storage(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &StoragePath,
) -> Result<RepoResponse, NugetError> {
    Ok(storage.open_file(repository_id, path).await?.into())
}

pub async fn save_json_cache(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &StoragePath,
    value: &Value,
) -> Result<(), NugetError> {
    storage
        .save_file(
            repository_id,
            FileContent::Content(serde_json::to_vec(value)?),
            path,
        )
        .await?;
    Ok(())
}

pub async fn save_text_cache(
    storage: &DynStorage,
    repository_id: Uuid,
    path: &StoragePath,
    value: String,
) -> Result<(), NugetError> {
    storage
        .save_file(
            repository_id,
            FileContent::Content(value.into_bytes()),
            path,
        )
        .await?;
    Ok(())
}

pub fn clone_parts(parts: &Parts) -> Parts {
    let mut builder = http::Request::builder()
        .method(parts.method.clone())
        .uri(parts.uri.clone())
        .version(parts.version);
    if let Some(headers) = builder.headers_mut() {
        *headers = parts.headers.clone();
    }
    builder
        .body(())
        .expect("cloning request parts should be infallible")
        .into_parts()
        .0
}

pub fn parse_flatcontainer_index_versions(value: &Value) -> Vec<String> {
    value
        .get("versions")
        .and_then(Value::as_array)
        .map(|versions| {
            versions
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn warn_nested_virtual(member_id: Uuid) {
    warn!(repository = %member_id, "Ignoring nested NuGet virtual member to prevent recursion");
}
