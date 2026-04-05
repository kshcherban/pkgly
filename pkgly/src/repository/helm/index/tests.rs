#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::helm::chart::ChartMaintainer;
use chrono::{TimeZone, Utc};
use semver::Version;
use serde_yaml::Value;

fn sample_metadata() -> HelmChartMetadata {
    HelmChartMetadata {
        name: "webapp".to_string(),
        version: Version::parse("1.2.3").unwrap(),
        description: Some("Sample chart".to_string()),
        app_version: Some("1.2.0".to_string()),
        kube_version: Some(">=1.24.0".to_string()),
        home: Some("https://example.com/webapp".to_string()),
        sources: vec!["https://github.com/example/webapp".to_string()],
        keywords: vec!["web".to_string(), "app".to_string()],
        maintainers: vec![ChartMaintainer {
            name: "Ops".to_string(),
            email: Some("ops@example.com".to_string()),
            url: Some("https://example.com/ops".to_string()),
        }],
        engine: Some("gotpl".to_string()),
        icon: Some("https://example.com/icon.png".to_string()),
        annotations: BTreeMap::from([("category".to_string(), "test".to_string())]),
        dependencies: vec![ChartDependency {
            name: "postgres".to_string(),
            version: "12.0.0".to_string(),
            repository: Some("https://charts.example.com".to_string()),
            condition: Some("postgres.enabled".to_string()),
            tags: vec!["database".to_string()],
            enabled: Some(true),
            import_values: vec!["databases.postgres".to_string()],
            alias: None,
        }],
        created: Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap(),
        api_version: ChartApiVersion::V2,
        chart_type: ChartType::Application,
        tiller_version: None,
    }
}

#[test]
fn render_index_contains_expected_metadata_for_v3_chart() {
    let metadata = sample_metadata();
    let digest = "sha256:deadbeef".to_string();
    let config = IndexRenderConfig {
        http_base_url: "https://pkgly.example.com/repositories/default/helm-project",
        include_charts_prefix: true,
        mode: IndexUrlMode::Http,
    };
    let urls = vec![config.chart_download_url(&metadata.name, &metadata.version.to_string())];
    let entry = IndexEntry::new(
        metadata.clone(),
        digest.clone(),
        1024,
        urls.clone(),
        ChartProvenanceState::Missing,
    );

    let yaml = render_index_yaml(&[entry], &config).expect("expected index rendering to succeed");

    let value: Value = serde_yaml::from_str(&yaml).expect("valid YAML");
    assert_eq!(value["apiVersion"].as_str(), Some("v1"));

    let entry = &value["entries"]["webapp"][0];
    assert_eq!(entry["name"].as_str(), Some("webapp"));
    assert_eq!(entry["version"].as_str(), Some("1.2.3"));
    assert_eq!(entry["appVersion"].as_str(), Some("1.2.0"));
    assert_eq!(entry["description"].as_str(), Some("Sample chart"));
    assert_eq!(entry["kubeVersion"].as_str(), Some(">=1.24.0"));
    assert_eq!(
        entry["urls"].as_sequence().unwrap()[0].as_str().unwrap(),
        urls[0].as_str()
    );
    assert_eq!(entry["digest"].as_str(), Some(digest.as_str()));
    assert_eq!(entry["annotations"]["category"].as_str(), Some("test"));
    assert_eq!(
        entry["maintainers"].as_sequence().unwrap()[0]["email"].as_str(),
        Some("ops@example.com")
    );
    assert_eq!(
        entry["dependencies"].as_sequence().unwrap()[0]["repository"].as_str(),
        Some("https://charts.example.com")
    );
    assert_eq!(entry["type"].as_str(), Some("application"));
    assert_eq!(entry["engine"].as_str(), Some("gotpl"));
}
