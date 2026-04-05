use serde::{Deserialize, Serialize};

use crate::repository::helm::chart::HelmChartMetadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelmChartVersionExtra {
    pub metadata: HelmChartMetadata,
    pub digest: String,
    pub canonical_path: String,
    pub size_bytes: u64,
    pub provenance: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oci_manifest_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oci_config_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oci_repository: Option<String>,
}
