use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OciDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OciManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub config: OciDescriptor,
    pub layers: Vec<OciDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug)]
pub struct HelmOciManifestInput {
    pub chart_digest: String,
    pub chart_size: u64,
    pub chart_name: String,
    pub chart_version: String,
    pub config_digest: String,
    pub config_size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum HelmOciError {
    #[error("invalid Helm OCI manifest parameters: {0}")]
    InvalidInput(String),
}

pub fn build_helm_manifest(_input: HelmOciManifestInput) -> Result<OciManifest, HelmOciError> {
    if _input.chart_digest.is_empty() {
        return Err(HelmOciError::InvalidInput(
            "chart_digest cannot be empty".to_string(),
        ));
    }
    if _input.config_digest.is_empty() {
        return Err(HelmOciError::InvalidInput(
            "config_digest cannot be empty".to_string(),
        ));
    }
    if _input.chart_size == 0 || _input.config_size == 0 {
        return Err(HelmOciError::InvalidInput(
            "descriptor sizes must be greater than zero".to_string(),
        ));
    }

    let mut manifest_annotations = Map::new();
    manifest_annotations.insert(
        "org.opencontainers.image.title".to_string(),
        Value::String(_input.chart_name.clone()),
    );
    manifest_annotations.insert(
        "org.opencontainers.image.version".to_string(),
        Value::String(_input.chart_version.clone()),
    );

    let mut config_annotations = Map::new();
    config_annotations.insert(
        "io.cncf.helm.chart.name".to_string(),
        Value::String(_input.chart_name.clone()),
    );
    config_annotations.insert(
        "io.cncf.helm.chart.version".to_string(),
        Value::String(_input.chart_version.clone()),
    );

    let config_descriptor = OciDescriptor {
        media_type: "application/vnd.cncf.helm.config.v1+json".to_string(),
        digest: _input.config_digest,
        size: _input.config_size,
        annotations: Some(config_annotations),
    };

    let layer_descriptor = OciDescriptor {
        media_type: "application/vnd.cncf.helm.chart.content.v1.tar+gzip".to_string(),
        digest: _input.chart_digest,
        size: _input.chart_size,
        annotations: None,
    };

    Ok(OciManifest {
        schema_version: 2,
        config: config_descriptor,
        layers: vec![layer_descriptor],
        annotations: Some(manifest_annotations),
    })
}

#[cfg(test)]
mod tests;
