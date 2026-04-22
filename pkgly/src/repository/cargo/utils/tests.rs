#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use serde_json::json;

#[test]
fn normalize_crate_name_lowercases() {
    assert_eq!(normalize_crate_name("Serde"), "serde");
    assert_eq!(normalize_crate_name("my-crate"), "my-crate");
}

#[test]
fn crate_index_path_follows_cargo_rules() {
    let cases = [
        ("a", "1/a"),
        ("ab", "2/ab"),
        ("abc", "3/a/abc"),
        ("serde", "se/rd/serde"),
        ("serde_json", "se/rd/serde_json"),
        ("MyCrate", "my/cr/mycrate"),
    ];

    for (name, expected) in cases {
        let path = crate_index_relative_path(name).expect("path");
        assert_eq!(path, expected, "crate {name}");
    }
}

#[test]
fn crate_index_path_rejects_invalid_name() {
    let err = crate_index_relative_path("bad space").expect_err("invalid name");
    assert!(matches!(err, CargoUtilError::InvalidCrateName(_)));
}

#[test]
fn archive_storage_path_places_crates_under_version_directory() {
    let path = crate_archive_storage_path(
        "serde",
        &semver::Version::parse("1.2.3").expect("parse version"),
    );
    assert_eq!(path.to_string(), "crates/serde/1.2.3/serde-1.2.3.crate");
}

#[test]
fn sparse_index_storage_path_matches_index_relative_path() {
    let path = sparse_index_storage_path("serde").expect("path");
    assert_eq!(path.to_string(), "index/se/rd/serde");
}

#[test]
fn parse_publish_payload_splits_metadata_and_archive() {
    let metadata = json!({
        "name": "serde",
        "vers": "1.0.0",
        "deps": [],
        "features": {},
        "authors": ["Serde Developers"],
        "description": "Serde description",
        "documentation": null,
        "homepage": null,
        "repository": null,
        "keywords": [],
        "categories": [],
        "license": "MIT",
        "readme": null,
        "readme_file": null,
        "badges": {},
        "links": null
    });
    let metadata_bytes = serde_json::to_vec(&metadata).unwrap();
    let crate_bytes = vec![1, 2, 3, 4, 5];

    let mut body = Vec::new();
    body.extend(&(metadata_bytes.len() as u32).to_le_bytes());
    body.extend(&metadata_bytes);
    body.extend(&(crate_bytes.len() as u32).to_le_bytes());
    body.extend(&crate_bytes);

    let parsed = parse_publish_payload(&body).expect("parse payload");

    assert_eq!(parsed.metadata.name, "serde");
    assert_eq!(
        parsed.metadata.vers,
        semver::Version::parse("1.0.0").unwrap()
    );
    assert_eq!(parsed.crate_archive, crate_bytes);
}

#[test]
fn parse_publish_payload_rejects_truncated_body() {
    let body = vec![0, 1, 2];
    let err = parse_publish_payload(&body).expect_err("truncated payload");
    assert_eq!(err, CargoUtilError::TruncatedPayload);
}

#[test]
fn parse_publish_payload_rejects_bad_lengths() {
    let metadata_bytes = br#"{"name":"serde","vers":"1.0.0","deps":[],"features":{},"authors":[],"keywords":[],"categories":[],"badges":{}}"#;
    let mut body = Vec::new();
    body.extend(&(metadata_bytes.len() as u32 + 10).to_le_bytes());
    body.extend(metadata_bytes);
    body.extend(&0u32.to_le_bytes());

    let err = parse_publish_payload(&body).expect_err("length mismatch");
    assert_eq!(err, CargoUtilError::MetadataLengthMismatch);
}

#[test]
fn build_index_entry_sets_checksum_and_metadata() {
    let metadata = PublishMetadata {
        name: "serde".into(),
        vers: semver::Version::parse("1.0.0").unwrap(),
        deps: vec![PublishDependency {
            name: "serde_derive".into(),
            vers: Some("^1".into()),
            optional: false,
            default_features: true,
            features: vec![],
            target: None,
            kind: None,
            registry: None,
            package: None,
        }],
        features: Default::default(),
        authors: vec!["Serde Developers".into()],
        description: Some("Serde description".into()),
        documentation: Some("https://docs.rs/serde".into()),
        homepage: None,
        repository: Some("https://github.com/serde-rs/serde".into()),
        keywords: vec!["serde".into()],
        categories: vec!["parsing".into()],
        license: Some("MIT OR Apache-2.0".into()),
        license_file: None,
        readme: None,
        readme_file: None,
        badges: Default::default(),
        links: None,
        v: Some(2),
    };
    let entry = build_index_entry(
        &metadata,
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    );

    assert_eq!(entry["name"], "serde");
    assert_eq!(entry["vers"], "1.0.0");
    assert_eq!(
        entry["cksum"],
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );
    assert!(entry["deps"].is_array());
}

#[test]
fn build_config_json_points_to_download_and_api() {
    let base = "https://example.com".parse::<Uri>().unwrap();
    let json = build_config_json(&base, "main", "crates", false);

    assert_eq!(
        json["dl"],
        "https://example.com/repositories/main/crates/api/v1/crates"
    );
    assert_eq!(json["api"], "https://example.com/repositories/main/crates");
    assert_eq!(
        json["index"],
        "sparse+https://example.com/repositories/main/crates/index"
    );
    assert_eq!(json["auth-required"], false);
}

#[test]
fn build_config_json_marks_auth_required() {
    let base = "https://example.com".parse::<Uri>().unwrap();
    let json = build_config_json(&base, "secure", "cargo", true);
    assert_eq!(json["auth-required"], true);
}

#[test]
fn build_login_response_points_to_web_ui() {
    let base = "https://example.com".parse::<Uri>().unwrap();
    let json = build_login_response(&base);
    assert_eq!(json["message"], "Use Pkgly UI to generate an API token.");
    assert_eq!(
        json["token_help_url"],
        "https://example.com/app/settings/tokens"
    );
}
