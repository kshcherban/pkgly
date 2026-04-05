#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn manifest_uses_expected_media_types() {
    let input = HelmOciManifestInput {
        chart_digest: "sha256:deadbeef".to_string(),
        chart_size: 42,
        chart_name: "webapp".to_string(),
        chart_version: "1.2.3".to_string(),
        config_digest: "sha256:cafebabe".to_string(),
        config_size: 128,
    };

    let manifest = build_helm_manifest(input).expect("manifest creation should succeed");
    assert_eq!(manifest.schema_version, 2);
    assert_eq!(manifest.layers.len(), 1);
    assert_eq!(
        manifest.config.media_type,
        "application/vnd.cncf.helm.config.v1+json"
    );
    assert_eq!(
        manifest.layers[0].media_type,
        "application/vnd.cncf.helm.chart.content.v1.tar+gzip"
    );
    assert_eq!(manifest.layers[0].size, 42);
    assert_eq!(manifest.layers[0].digest, "sha256:deadbeef");
}

#[test]
fn manifest_includes_chart_annotations() {
    let input = HelmOciManifestInput {
        chart_digest: "sha256:feedface".to_string(),
        chart_size: 512,
        chart_name: "metrics".to_string(),
        chart_version: "0.8.0".to_string(),
        config_digest: "sha256:012345".to_string(),
        config_size: 256,
    };

    let manifest = build_helm_manifest(input).expect("manifest creation should succeed");
    let annotations = manifest.annotations.expect("annotations should be present");
    assert_eq!(
        annotations.get("org.opencontainers.image.title").unwrap(),
        &serde_json::Value::String("metrics".to_string())
    );
    assert_eq!(
        annotations.get("org.opencontainers.image.version").unwrap(),
        &serde_json::Value::String("0.8.0".to_string())
    );
}
