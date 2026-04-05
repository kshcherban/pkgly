#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use super::*;
use chrono::TimeZone;
use nr_core::repository::project::RubyDependencyMetadata;

#[test]
fn build_names_file_includes_header_and_names() {
    let names = vec!["a".to_string(), "b".to_string()];
    let file = build_names_file(&names);
    assert_eq!(file, "---\na\nb\n");
}

#[test]
fn build_info_file_formats_dependencies_and_requirements() {
    let entries = vec![CompactIndexVersionEntry {
        version: "1.2.3".to_string(),
        dependencies: vec![RubyDependencyMetadata {
            name: "rack".to_string(),
            requirements: vec![">= 1.0".to_string(), "< 3.0".to_string()],
        }],
        sha256: "deadbeef".to_string(),
        required_ruby: Some(">= 2.7.0".to_string()),
        required_rubygems: None,
    }];

    let file = build_info_file(&entries);
    assert_eq!(
        file,
        "---\n1.2.3 rack:>= 1.0&< 3.0|checksum:deadbeef,ruby:>= 2.7.0\n"
    );
}

#[test]
fn build_info_file_includes_space_before_pipe_when_no_dependencies() {
    let entries = vec![CompactIndexVersionEntry {
        version: "0.1.0-x86_64-linux".to_string(),
        dependencies: Vec::new(),
        sha256: "00".to_string(),
        required_ruby: None,
        required_rubygems: None,
    }];
    let file = build_info_file(&entries);
    assert_eq!(file, "---\n0.1.0-x86_64-linux |checksum:00\n");
}

#[test]
fn build_versions_file_formats_created_at_and_lines() {
    let created_at = Utc.with_ymd_and_hms(2024, 4, 1, 0, 0, 5).unwrap();
    let lines = vec![VersionsLine {
        gem_name: "rack".to_string(),
        versions: vec!["1.0.0".to_string(), "1.1.0".to_string()],
        info_md5: "abcd".to_string(),
    }];
    let file = build_versions_file(created_at, &lines);
    assert_eq!(
        file,
        "created_at: 2024-04-01T00:00:05Z\n---\nrack 1.0.0,1.1.0 abcd\n"
    );
}

#[test]
fn build_compact_index_artifacts_generates_info_and_versions() {
    let created_at = Utc.with_ymd_and_hms(2024, 4, 1, 0, 0, 5).unwrap();
    let rows = vec![
        RubyCompactIndexRow {
            gem_key: "demo".to_string(),
            gem_name: "demo".to_string(),
            version: "1.0.0".to_string(),
            metadata: nr_core::repository::project::RubyPackageMetadata {
                filename: "gems/demo-1.0.0.gem".to_string(),
                platform: None,
                sha256: Some("aa".to_string()),
                dependencies: vec![RubyDependencyMetadata {
                    name: "rack".to_string(),
                    requirements: vec![">= 1.0".to_string()],
                }],
                required_ruby: None,
                required_rubygems: None,
            },
        },
        RubyCompactIndexRow {
            gem_key: "demo".to_string(),
            gem_name: "demo".to_string(),
            version: "1.1.0".to_string(),
            metadata: nr_core::repository::project::RubyPackageMetadata {
                filename: "gems/demo-1.1.0.gem".to_string(),
                platform: None,
                sha256: Some("bb".to_string()),
                dependencies: Vec::new(),
                required_ruby: None,
                required_rubygems: None,
            },
        },
    ];

    let artifacts = build_compact_index_artifacts(created_at, &rows).expect("build artifacts");
    assert_eq!(artifacts.names, "---\ndemo\n");
    let info = artifacts.infos.get("demo").expect("info");
    assert_eq!(
        info,
        "---\n1.0.0 rack:>= 1.0|checksum:aa\n1.1.0 |checksum:bb\n"
    );
    let expected_md5 = md5_hex(info.as_bytes());
    assert_eq!(
        artifacts.versions,
        format!("created_at: 2024-04-01T00:00:05Z\n---\ndemo 1.0.0,1.1.0 {expected_md5}\n")
    );
}
