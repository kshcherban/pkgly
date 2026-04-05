#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::repository::helm::chart::{ChartApiVersion, ChartType, HelmChartMetadata};
use chrono::{FixedOffset, TimeZone, Utc};
use nr_core::repository::RepositoryName;
use semver::Version;
use std::collections::BTreeMap;
use uuid::Uuid;

fn sample_repository_record(active: bool, visibility: Visibility) -> DBRepository {
    let offset = FixedOffset::east_opt(0).unwrap();
    DBRepository {
        id: Uuid::new_v4(),
        storage_id: Uuid::new_v4(),
        name: RepositoryName::new("helm-repo".into()).unwrap(),
        repository_type: "helm".into(),
        visibility,
        active,
        updated_at: offset.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        created_at: offset.with_ymd_and_hms(2023, 12, 31, 0, 0, 0).unwrap(),
        storage_usage_bytes: None,
        storage_usage_updated_at: None,
    }
}

fn sample_metadata() -> HelmChartMetadata {
    HelmChartMetadata {
        name: "webapp".to_string(),
        version: Version::parse("1.0.0").unwrap(),
        description: None,
        app_version: None,
        kube_version: None,
        home: None,
        sources: Vec::new(),
        keywords: Vec::new(),
        maintainers: Vec::new(),
        engine: None,
        icon: None,
        annotations: BTreeMap::new(),
        dependencies: Vec::new(),
        created: Utc::now(),
        api_version: ChartApiVersion::V2,
        chart_type: ChartType::Application,
        tiller_version: None,
    }
}

#[test]
fn parse_manifest_path_extracts_repository_and_reference() {
    let parsed =
        super::parse_manifest_request_path("v2/test/storage/helm/manifests/1.2.3").unwrap();
    assert_eq!(parsed.0, "test/storage/helm");
    assert_eq!(parsed.1, "1.2.3");
}

#[test]
fn parse_manifest_path_rejects_invalid_inputs() {
    assert!(super::parse_manifest_request_path("v2/test/manifests").is_none());
    assert!(super::parse_manifest_request_path("invalid").is_none());
}

#[test]
fn parse_chartmuseum_delete_path_accepts_valid_input() {
    let parsed = super::parse_chartmuseum_delete_path("api/charts/webapp/1.0.0")
        .expect("should parse delete path");
    assert_eq!(parsed.0, "webapp");
    assert_eq!(parsed.1, "1.0.0");
}

#[test]
fn parse_chartmuseum_delete_path_rejects_invalid_input() {
    assert!(super::parse_chartmuseum_delete_path("api/charts/webapp").is_none());
    assert!(super::parse_chartmuseum_delete_path("invalid/path").is_none());
}

#[test]
fn parses_root_chart_artifact() {
    let path = StoragePath::from("webapp-1.0.0.tgz");
    let artifact = parse_chart_artifact(&path).expect("should parse chart");
    assert_eq!(artifact.name, "webapp");
    assert_eq!(artifact.version, "1.0.0");
    assert!(!artifact.is_provenance);
    assert!(!artifact.alias);
}

#[test]
fn parses_alias_provenance_artifact() {
    let path = StoragePath::from("charts/webapp-1.0.0.tgz.prov");
    let artifact = parse_chart_artifact(&path).expect("should parse provenance");
    assert_eq!(artifact.name, "webapp");
    assert_eq!(artifact.version, "1.0.0");
    assert!(artifact.is_provenance);
    assert!(artifact.alias);
}

#[test]
fn parses_nested_chart_artifact() {
    let path = StoragePath::from("charts/webapp/webapp-1.0.0.tgz");
    let artifact = parse_chart_artifact(&path).expect("should parse nested chart");
    assert_eq!(artifact.name, "webapp");
    assert_eq!(artifact.version, "1.0.0");
    assert!(!artifact.is_provenance);
    assert!(artifact.alias);
}

#[test]
fn rejects_mismatched_nested_directory() {
    let path = StoragePath::from("charts/other/webapp-1.0.0.tgz");
    assert!(parse_chart_artifact(&path).is_none());
}

#[test]
fn sha256_digest_includes_prefix() {
    let digest = sha256_digest(b"example-bytes");
    assert!(digest.starts_with("sha256:"));
    assert_eq!(
        digest,
        "sha256:ff00188058a5a5549cc34752ee3fee897c4303e8cb44996eab8caa134c5253fa"
    );
}

#[test]
fn update_provenance_keeps_canonical_path() {
    let mut extra = HelmChartVersionExtra {
        metadata: sample_metadata(),
        digest: "sha256:abc".to_string(),
        canonical_path: "charts/webapp/webapp-1.0.0.tgz".to_string(),
        size_bytes: 512,
        provenance: false,
        provenance_path: None,
        oci_manifest_digest: None,
        oci_config_digest: None,
        oci_repository: None,
    };
    let prov_path = StoragePath::from("charts/webapp/webapp-1.0.0.tgz.prov");
    update_provenance_extra(&mut extra, &prov_path);
    assert!(extra.provenance);
    assert_eq!(extra.canonical_path, "charts/webapp/webapp-1.0.0.tgz");
    assert_eq!(
        extra.provenance_path.as_deref(),
        Some("charts/webapp/webapp-1.0.0.tgz.prov")
    );
}

#[test]
fn runtime_state_update_refreshes_config_and_flags() {
    let repository = sample_repository_record(true, Visibility::Private);
    let mut new_repository = repository.clone();
    new_repository.active = false;
    new_repository.visibility = Visibility::Public;

    let config = HelmRepositoryConfig {
        overwrite: false,
        index_cache_ttl: Some(42),
        mode: HelmRepositoryMode::Http,
        public_base_url: None,
        max_chart_size: Some(1_000),
        max_file_count: Some(50),
    };
    let mut updated_config = config.clone();
    updated_config.overwrite = true;
    updated_config.index_cache_ttl = Some(99);

    let auth = RepositoryAuthConfig { enabled: false };
    let mut updated_auth = auth.clone();
    updated_auth.enabled = true;

    let state = HelmRuntimeState::new(repository, config, auth);
    state.update(
        new_repository.clone(),
        updated_config.clone(),
        updated_auth.clone(),
    );

    assert_eq!(
        state.is_active(),
        new_repository.active,
        "active flag should refresh"
    );
    assert_eq!(
        state.visibility(),
        new_repository.visibility,
        "visibility should refresh"
    );
    assert!(
        state.config().overwrite,
        "expected overwrite flag to update from runtime refresh"
    );
    assert_eq!(
        state.config().index_cache_ttl,
        updated_config.index_cache_ttl,
        "expected cache ttl to update"
    );
    assert_eq!(
        state.auth_config().enabled,
        updated_auth.enabled,
        "expected auth config to refresh"
    );
}
