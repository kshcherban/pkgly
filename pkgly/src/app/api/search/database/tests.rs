#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use chrono::{FixedOffset, TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::{
    app::api::search::{
        query_parser::{Operator, SearchQuery},
        version_constraint::VersionConstraint,
    },
    search::query::DatabasePackageRow,
};

fn make_row(name: &str, version: &str) -> DatabasePackageRow {
    DatabasePackageRow {
        package_name: name.to_string(),
        package_key: name.to_string(),
        version: version.to_string(),
        path: format!("{name}/{version}/"),
        extra: Some(json!({ "size": 1024 })),
        updated_at: Utc
            .with_ymd_and_hms(2025, 11, 1, 12, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&FixedOffset::east_opt(0).unwrap()),
    }
}

fn make_deb_row(name: &str, version: &str, arch: &str) -> DatabasePackageRow {
    let filename = format!("pool/main/{name}/{name}_{version}_{arch}.deb");
    DatabasePackageRow {
        package_name: name.to_string(),
        package_key: name.to_string(),
        version: version.to_string(),
        path: filename.clone(),
        extra: Some(json!({
            "distribution": "bookworm",
            "component": "main",
            "architecture": arch,
            "filename": filename,
            "size": 4096,
            "md5": "deadbeef",
            "sha1": "feedface",
            "sha256": "cafebabe",
            "depends": ["libc6 (>= 2.28)"],
            "section": "utils",
            "priority": "optional",
            "description": "Sample package"
        })),
        updated_at: Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&FixedOffset::east_opt(0).unwrap()),
    }
}

#[tokio::test]
async fn filter_database_rows_matches_package_name() {
    let summary = RepositorySummary {
        repository_id: Uuid::new_v4(),
        repository_name: "helm-hosted".into(),
        storage_name: "primary".into(),
        repository_type: "helm".into(),
    };

    let rows = vec![make_row("postgresql", "16.1.0"), make_row("nginx", "2.0.0")];
    let query = SearchQuery {
        package_filter: Some((Operator::Equals, "postgresql".to_string())),
        ..SearchQuery::default()
    };

    let results = filter_database_rows(&summary, rows, &query, 10).expect("query to pass");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_name, "postgresql@16.1.0");
}

#[tokio::test]
async fn filter_database_rows_applies_limit() {
    let summary = RepositorySummary {
        repository_id: Uuid::new_v4(),
        repository_name: "helm-hosted".into(),
        storage_name: "primary".into(),
        repository_type: "helm".into(),
    };

    let rows = vec![
        make_row("chart-a", "1.0.0"),
        make_row("chart-a", "1.1.0"),
        make_row("chart-a", "1.2.0"),
    ];

    let query = SearchQuery::default();
    let results = filter_database_rows(&summary, rows, &query, 2).expect("limit to apply");
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn filter_database_rows_respects_version_constraint() {
    let summary = RepositorySummary {
        repository_id: Uuid::new_v4(),
        repository_name: "helm-hosted".into(),
        storage_name: "primary".into(),
        repository_type: "helm".into(),
    };

    let rows = vec![make_row("chart-a", "1.0.0"), make_row("chart-a", "2.0.0")];
    let query = SearchQuery {
        version_constraint: Some(VersionConstraint::Exact("1.0.0".to_string())),
        ..SearchQuery::default()
    };

    let results = filter_database_rows(&summary, rows, &query, 10).expect("query to pass");
    assert_eq!(results.len(), 1);
    assert!(results[0].file_name.contains("1.0.0"));
}

#[tokio::test]
async fn filter_database_rows_accepts_partial_terms_without_filters() {
    let summary = RepositorySummary {
        repository_id: Uuid::new_v4(),
        repository_name: "helm-hosted".into(),
        storage_name: "primary".into(),
        repository_type: "helm".into(),
    };

    let rows = vec![make_row("chart-a", "1.0.0")];
    let query = SearchQuery {
        terms: vec!["chart".into()],
        ..SearchQuery::default()
    };

    let results = filter_database_rows(&summary, rows, &query, 10).expect("query to pass");
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn filter_database_rows_matches_deb_metadata_terms() {
    let summary = RepositorySummary {
        repository_id: Uuid::new_v4(),
        repository_name: "deb-hosted".into(),
        storage_name: "primary".into(),
        repository_type: "deb".into(),
    };

    let rows = vec![make_deb_row("hello", "2.10", "amd64")];
    let query = SearchQuery {
        terms: vec!["amd64".into()],
        ..SearchQuery::default()
    };

    let results = filter_database_rows(&summary, rows, &query, 10).expect("query to pass");
    assert_eq!(results.len(), 1);
    assert!(results[0].file_name.ends_with(".deb"));
    assert_eq!(
        results[0].cache_path,
        "pool/main/hello/hello_2.10_amd64.deb"
    );
}
