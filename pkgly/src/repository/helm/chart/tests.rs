#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use std::io::Cursor;

use flate2::{Compression, write::GzEncoder};
use tar::Builder;

fn build_chart_archive(chart_yaml: &str, extra_files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = Builder::new(&mut encoder);
        append_file(&mut builder, "Chart.yaml", chart_yaml.as_bytes());
        for (name, contents) in extra_files {
            append_file(&mut builder, name, contents);
        }
        builder.finish().unwrap();
    }
    encoder.finish().unwrap()
}

fn append_file<W>(builder: &mut Builder<W>, path: &str, contents: &[u8])
where
    W: std::io::Write,
{
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, &mut Cursor::new(contents))
        .unwrap();
}

fn default_options() -> ChartValidationOptions {
    ChartValidationOptions::default()
}

#[test]
fn parse_chart_archive_with_v2_metadata_extracts_fields() {
    let chart_yaml = r#"apiVersion: v2
name: webapp
version: 1.2.3
description: Sample chart
type: application
appVersion: "1.2.0"
kubeVersion: ">=1.24.0"
home: https://example.com/webapp
icon: https://example.com/icon.png
keywords:
  - web
  - app
sources:
  - https://github.com/example/webapp
maintainers:
  - name: Ops
    email: ops@example.com
    url: https://example.com/ops
annotations:
  category: test
dependencies:
  - name: postgres
    version: "12.0.0"
    repository: https://charts.example.com
    condition: postgres.enabled
    tags:
      - database
    importValues:
      - databases.postgres
"#;

    let archive = build_chart_archive(
        chart_yaml,
        &[
            ("templates/deployment.yaml", b""),
            ("values.yaml", b"replicaCount: 2"),
        ],
    );

    let parsed = parse_chart_archive(&archive, &default_options())
        .expect("expected chart archive to parse successfully");

    assert_eq!(parsed.metadata.name, "webapp");
    assert_eq!(parsed.metadata.version, Version::parse("1.2.3").unwrap());
    assert_eq!(parsed.metadata.api_version, ChartApiVersion::V2);
    assert_eq!(parsed.metadata.chart_type, ChartType::Application);
    assert_eq!(parsed.metadata.app_version.as_deref(), Some("1.2.0"));
    assert_eq!(parsed.metadata.kube_version.as_deref(), Some(">=1.24.0"));
    assert_eq!(
        parsed.metadata.keywords,
        vec!["web".to_string(), "app".to_string()]
    );
    assert_eq!(
        parsed.metadata.sources,
        vec!["https://github.com/example/webapp".to_string()]
    );
    assert_eq!(parsed.metadata.maintainers.len(), 1);
    assert_eq!(parsed.metadata.dependencies.len(), 1);
    assert_eq!(
        parsed.metadata.annotations.get("category"),
        Some(&"test".to_string())
    );
    assert!(parsed.digest.starts_with("sha256:"));
    assert_eq!(parsed.metadata.tiller_version.as_deref(), None);
    assert_eq!(parsed.provenance, ChartProvenanceState::Missing);
    assert_eq!(parsed.archive_bytes, archive);
}

#[test]
fn parse_chart_archive_ignores_dependency_chart_yaml() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = Builder::new(&mut encoder);
        append_file(
            &mut builder,
            "primary/Chart.yaml",
            br#"apiVersion: v2
name: primary
version: 1.2.3
"#,
        );
        append_file(
            &mut builder,
            "primary/charts/common/Chart.yaml",
            br#"apiVersion: v2
name: common
version: 9.9.9
"#,
        );
        builder.finish().unwrap();
    }
    let archive = encoder.finish().unwrap();

    let parsed = parse_chart_archive(&archive, &default_options()).unwrap();
    assert_eq!(parsed.metadata.name, "primary");
    assert_eq!(parsed.metadata.version, Version::parse("1.2.3").unwrap());
}

#[test]
fn parse_chart_archive_without_chart_yaml_returns_error() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = Builder::new(&mut encoder);
        append_file(&mut builder, "values.yaml", b"replicaCount: 1");
        builder.finish().unwrap();
    }
    let archive = encoder.finish().unwrap();

    let error = parse_chart_archive(&archive, &default_options()).unwrap_err();
    assert!(matches!(error, ChartParseError::MissingChartYaml));
}
