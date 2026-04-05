use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::{Component, Path},
};

use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tar::Archive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartApiVersion {
    V1,
    V2,
}

impl ChartApiVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChartApiVersion::V1 => "v1",
            ChartApiVersion::V2 => "v2",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartType {
    Application,
    Library,
    Other(String),
}

impl ChartType {
    pub fn as_str(&self) -> &str {
        match self {
            ChartType::Application => "application",
            ChartType::Library => "library",
            ChartType::Other(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartMaintainer {
    pub name: String,
    pub email: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartDependency {
    pub name: String,
    pub version: String,
    pub repository: Option<String>,
    pub condition: Option<String>,
    pub tags: Vec<String>,
    pub enabled: Option<bool>,
    pub import_values: Vec<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelmChartMetadata {
    pub name: String,
    pub version: Version,
    pub description: Option<String>,
    pub app_version: Option<String>,
    pub kube_version: Option<String>,
    pub home: Option<String>,
    pub sources: Vec<String>,
    pub keywords: Vec<String>,
    pub maintainers: Vec<ChartMaintainer>,
    pub engine: Option<String>,
    pub icon: Option<String>,
    pub annotations: BTreeMap<String, String>,
    pub dependencies: Vec<ChartDependency>,
    pub created: DateTime<Utc>,
    pub api_version: ChartApiVersion,
    pub chart_type: ChartType,
    pub tiller_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartProvenanceState {
    Missing,
    Present,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedChartArchive {
    pub metadata: HelmChartMetadata,
    pub digest: String,
    pub size_bytes: u64,
    pub provenance: ChartProvenanceState,
    pub archive_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartValidationOptions {
    pub max_chart_size: usize,
    pub max_file_count: usize,
}

impl Default for ChartValidationOptions {
    fn default() -> Self {
        Self {
            max_chart_size: 10 * 1024 * 1024,
            max_file_count: 1024,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChartParseError {
    #[error("Chart archive exceeds configured size limit ({limit} bytes)")]
    ChartTooLarge { limit: usize },
    #[error("Chart archive is missing Chart.yaml")]
    MissingChartYaml,
    #[error("Chart archive contains an invalid Chart.yaml: {0}")]
    InvalidChartYaml(String),
    #[error("Chart archive entry has an invalid path: {0}")]
    InvalidPath(String),
    #[error("Chart archive contains too many files (limit {limit})")]
    TooManyFiles { limit: usize },
    #[error("Chart archive is not a valid gzip tarball: {0}")]
    InvalidArchive(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawDependency {
    name: String,
    version: Option<String>,
    repository: Option<String>,
    condition: Option<String>,
    tags: Option<Vec<String>>,
    enabled: Option<bool>,
    #[serde(default)]
    import_values: Vec<String>,
    alias: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawMaintainer {
    name: String,
    email: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawChartYaml {
    #[serde(rename = "apiVersion")]
    api_version: String,
    name: String,
    version: String,
    description: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    engine: Option<String>,
    home: Option<String>,
    icon: Option<String>,
    keywords: Option<Vec<String>>,
    sources: Option<Vec<String>>,
    maintainers: Option<Vec<RawMaintainer>>,
    annotations: Option<BTreeMap<String, String>>,
    dependencies: Option<Vec<RawDependency>>,
    #[serde(rename = "appVersion")]
    app_version: Option<String>,
    #[serde(rename = "kubeVersion")]
    kube_version: Option<String>,
    #[serde(rename = "tillerVersion")]
    tiller_version: Option<String>,
}

pub fn parse_chart_archive(
    bytes: &[u8],
    options: &ChartValidationOptions,
) -> Result<ParsedChartArchive, ChartParseError> {
    if bytes.len() > options.max_chart_size {
        return Err(ChartParseError::ChartTooLarge {
            limit: options.max_chart_size,
        });
    }

    let mut decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(&mut decoder);

    let mut chart_yaml_bytes: Option<Vec<u8>> = None;
    let mut file_count = 0usize;

    let mut entries = archive
        .entries()
        .map_err(|err| ChartParseError::InvalidArchive(err.to_string()))?;

    while let Some(entry_result) = entries.next() {
        let mut entry =
            entry_result.map_err(|err| ChartParseError::InvalidArchive(err.to_string()))?;
        let path = entry
            .path()
            .map_err(|err| ChartParseError::InvalidArchive(err.to_string()))?
            .into_owned();

        validate_entry_path(&path)?;

        if entry.header().entry_type().is_dir() {
            continue;
        }

        file_count += 1;
        if file_count > options.max_file_count {
            return Err(ChartParseError::TooManyFiles {
                limit: options.max_file_count,
            });
        }

        let file_name = path.file_name().and_then(|name| name.to_str());
        if let Some(file_name) = file_name {
            if file_name.eq_ignore_ascii_case("Chart.yaml")
                && !path
                    .components()
                    .any(|component| component.as_os_str() == "charts")
            {
                let mut buffer = Vec::new();
                entry
                    .read_to_end(&mut buffer)
                    .map_err(|err| ChartParseError::InvalidArchive(err.to_string()))?;
                chart_yaml_bytes = Some(buffer);
                break;
            }
        }
    }

    let chart_yaml_bytes = chart_yaml_bytes.ok_or(ChartParseError::MissingChartYaml)?;

    let raw_chart: RawChartYaml = serde_yaml::from_slice(&chart_yaml_bytes)
        .map_err(|err| ChartParseError::InvalidChartYaml(err.to_string()))?;

    let api_version = match raw_chart.api_version.as_str() {
        "v1" => ChartApiVersion::V1,
        "v2" => ChartApiVersion::V2,
        other => {
            return Err(ChartParseError::InvalidChartYaml(format!(
                "unsupported apiVersion: {other}"
            )));
        }
    };

    let version = Version::parse(&raw_chart.version)
        .map_err(|err| ChartParseError::InvalidChartYaml(format!("invalid version: {err}")))?;

    let maintainers = raw_chart
        .maintainers
        .unwrap_or_default()
        .into_iter()
        .map(|maintainer| ChartMaintainer {
            name: maintainer.name,
            email: maintainer.email,
            url: maintainer.url,
        })
        .collect::<Vec<_>>();

    let dependencies = raw_chart
        .dependencies
        .unwrap_or_default()
        .into_iter()
        .map(|dependency| ChartDependency {
            name: dependency.name,
            version: dependency.version.unwrap_or_default(),
            repository: dependency.repository,
            condition: dependency.condition,
            tags: dependency.tags.unwrap_or_default(),
            enabled: dependency.enabled,
            import_values: dependency.import_values,
            alias: dependency.alias,
        })
        .collect::<Vec<_>>();

    let annotations = raw_chart.annotations.unwrap_or_default();

    let chart_type = raw_chart
        .kind
        .as_deref()
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "application" => ChartType::Application,
            "library" => ChartType::Library,
            other => ChartType::Other(other.to_string()),
        })
        .unwrap_or(ChartType::Application);

    let metadata = HelmChartMetadata {
        name: raw_chart.name,
        version,
        description: raw_chart.description,
        app_version: raw_chart.app_version,
        kube_version: raw_chart.kube_version,
        home: raw_chart.home,
        sources: raw_chart.sources.unwrap_or_default(),
        keywords: raw_chart.keywords.unwrap_or_default(),
        maintainers,
        engine: raw_chart.engine,
        icon: raw_chart.icon,
        annotations,
        dependencies,
        created: Utc::now(),
        api_version,
        chart_type,
        tiller_version: raw_chart.tiller_version,
    };

    let digest = format!("sha256:{:x}", sha2::Sha256::digest(bytes));

    Ok(ParsedChartArchive {
        metadata,
        digest,
        size_bytes: bytes.len() as u64,
        provenance: ChartProvenanceState::Missing,
        archive_bytes: bytes.to_vec(),
    })
}

fn validate_entry_path(path: &Path) -> Result<(), ChartParseError> {
    for component in path.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ChartParseError::InvalidPath(path.display().to_string()));
            }
            Component::Normal(segment) => {
                if segment == "" {
                    return Err(ChartParseError::InvalidPath(path.display().to_string()));
                }
            }
            Component::CurDir => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
