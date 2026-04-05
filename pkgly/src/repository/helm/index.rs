use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::chart::{
    ChartApiVersion, ChartDependency, ChartProvenanceState, ChartType, HelmChartMetadata,
};

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub metadata: HelmChartMetadata,
    pub digest: String,
    pub size_bytes: u64,
    pub urls: Vec<String>,
    pub provenance: ChartProvenanceState,
}

impl IndexEntry {
    pub fn new(
        metadata: HelmChartMetadata,
        digest: String,
        size_bytes: u64,
        urls: Vec<String>,
        provenance: ChartProvenanceState,
    ) -> Self {
        Self {
            metadata,
            digest,
            size_bytes,
            urls,
            provenance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexUrlMode {
    Http,
    Oci,
}

#[derive(Debug, Clone)]
pub struct IndexRenderConfig<'a> {
    pub http_base_url: &'a str,
    pub include_charts_prefix: bool,
    pub mode: IndexUrlMode,
}

impl<'a> IndexRenderConfig<'a> {
    pub fn chart_download_url(&self, name: &str, version: &str) -> String {
        if self.include_charts_prefix {
            format!("{}/charts/{}-{}.tgz", self.http_base_url, name, version)
        } else {
            format!("{}/{}-{}.tgz", self.http_base_url, name, version)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IndexRenderError {
    #[error("failed to render Helm index: {0}")]
    RenderFailed(String),
}

#[derive(Debug, Serialize)]
struct IndexDocument {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    entries: BTreeMap<String, Vec<IndexEntryDocument>>,
    generated: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct IndexEntryDocument {
    name: String,
    version: String,
    digest: String,
    urls: Vec<String>,
    created: DateTime<Utc>,
    #[serde(rename = "appVersion", skip_serializing_if = "Option::is_none")]
    app_version: Option<String>,
    #[serde(rename = "kubeVersion", skip_serializing_if = "Option::is_none")]
    kube_version: Option<String>,
    #[serde(rename = "description", skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(rename = "home", skip_serializing_if = "Option::is_none")]
    home: Option<String>,
    #[serde(rename = "icon", skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(rename = "engine", skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(rename = "tillerVersion", skip_serializing_if = "Option::is_none")]
    tiller_version: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    keywords: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sources: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    maintainers: Vec<MaintainerDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    annotations: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<DependencyDocument>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    chart_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<String>,
    #[serde(rename = "apiVersion", skip_serializing_if = "Option::is_none")]
    chart_api_version: Option<String>,
}

#[derive(Debug, Serialize)]
struct MaintainerDocument {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Debug, Serialize)]
struct DependencyDocument {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    import_values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
}

pub fn render_index_yaml(
    entries: &[IndexEntry],
    config: &IndexRenderConfig,
) -> Result<String, IndexRenderError> {
    let mut grouped: BTreeMap<String, Vec<IndexEntryDocument>> = BTreeMap::new();

    for entry in entries {
        let metadata = &entry.metadata;
        let urls = if entry.urls.is_empty() {
            vec![config.chart_download_url(&metadata.name, &metadata.version.to_string())]
        } else {
            entry.urls.clone()
        };

        let maintainers = metadata
            .maintainers
            .iter()
            .map(|maintainer| MaintainerDocument {
                name: maintainer.name.clone(),
                email: maintainer.email.clone(),
                url: maintainer.url.clone(),
            })
            .collect::<Vec<_>>();

        let dependencies = metadata
            .dependencies
            .iter()
            .map(|dependency| dependency_to_document(dependency))
            .collect::<Vec<_>>();

        let chart_type = match metadata.chart_type {
            ChartType::Application => Some("application".to_string()),
            ChartType::Library => Some("library".to_string()),
            ChartType::Other(ref value) => some_if_not_empty(value),
        };

        let chart_api_version = match metadata.api_version {
            ChartApiVersion::V1 => Some("v1".to_string()),
            ChartApiVersion::V2 => Some("v2".to_string()),
        };

        let tiller_version =
            metadata
                .tiller_version
                .clone()
                .or_else(|| match metadata.api_version {
                    ChartApiVersion::V1 => Some(">=2.0.0".to_string()),
                    ChartApiVersion::V2 => None,
                });

        let engine = metadata
            .engine
            .clone()
            .or_else(|| Some("gotpl".to_string()));

        let annotations = if metadata.annotations.is_empty() {
            None
        } else {
            Some(metadata.annotations.clone())
        };

        let document = IndexEntryDocument {
            name: metadata.name.clone(),
            version: metadata.version.to_string(),
            digest: entry.digest.clone(),
            urls,
            created: metadata.created,
            app_version: metadata.app_version.clone(),
            kube_version: metadata.kube_version.clone(),
            description: metadata.description.clone(),
            home: metadata.home.clone(),
            icon: metadata.icon.clone(),
            engine,
            tiller_version,
            keywords: metadata.keywords.clone(),
            sources: metadata.sources.clone(),
            maintainers,
            annotations,
            dependencies,
            chart_type,
            provenance: provenance_label(entry.provenance),
            chart_api_version,
        };

        grouped
            .entry(metadata.name.clone())
            .or_default()
            .push(document);
    }

    for entry in grouped.values_mut() {
        entry.sort_by(|a, b| b.version.cmp(&a.version));
    }

    let index = IndexDocument {
        api_version: "v1",
        entries: grouped,
        generated: Utc::now(),
    };

    serde_yaml::to_string(&index).map_err(|err| IndexRenderError::RenderFailed(err.to_string()))
}

fn dependency_to_document(dependency: &ChartDependency) -> DependencyDocument {
    DependencyDocument {
        name: dependency.name.clone(),
        version: some_if_not_empty(&dependency.version),
        repository: dependency.repository.clone(),
        condition: dependency.condition.clone(),
        tags: dependency.tags.clone(),
        enabled: dependency.enabled,
        import_values: dependency.import_values.clone(),
        alias: dependency.alias.clone(),
    }
}

fn some_if_not_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn provenance_label(state: ChartProvenanceState) -> Option<String> {
    match state {
        ChartProvenanceState::Missing => None,
        ChartProvenanceState::Present => Some("present".to_string()),
    }
}

#[cfg(test)]
mod tests;
