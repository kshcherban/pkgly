#![allow(clippy::unwrap_used)]

use super::*;
use serde_json::json;

#[test]
fn package_metadata_exposes_readme_and_public_fields_only() {
    let version: PublishVersion = serde_json::from_value(json!({
        "name": "demo", "version": "1.0.0", "_id": "demo@1.0.0",
        "_nodeVersion": "22", "_npmVersion": "11",
        "dist": {"integrity": "hash", "shasum": "hash", "tarball": "https://registry.test/repositories/store/npm/demo/-/demo-1.0.0.tgz"},
        "readme": "# Demo", "readmeFilename": "README.md", "description": "Demo package",
        "license": "MIT", "keywords": ["demo"], "author": {"name":"Author"},
        "dependencies": {"dependency":"1.0.0"}, "deprecated": "old", "dist-tags": {"latest":"bad"}
    }))
    .unwrap();
    let fields = package_fields(Some(&version));
    assert_eq!(fields.get("readme"), Some(&json!("# Demo")));
    assert_eq!(fields.get("readmeFilename"), Some(&json!("README.md")));
    for key in ["description", "license", "keywords", "author"] {
        assert_eq!(fields.get(key), version.extra.get(key));
    }
    for key in ["dependencies", "deprecated", "dist-tags", "_id", "dist"] {
        assert!(!fields.contains_key(key));
    }
    assert!(package_fields(None).is_empty());
}

#[test]
fn publish_readme_falls_back_to_root_fields_without_replacing_version_fields() {
    let mut version: PublishVersion = serde_json::from_value(json!({
        "name": "demo", "version": "1.0.0", "_id": "demo@1.0.0",
        "_nodeVersion": "22", "_npmVersion": "11",
        "dist": {"integrity": "hash", "shasum": "hash", "tarball": "https://registry.test/repositories/store/npm/demo/-/demo-1.0.0.tgz"}
    }))
    .unwrap();
    let root = HashMap::from_iter([
        ("readme".into(), json!("# Root README")),
        ("readmeFilename".into(), json!("README.md")),
    ]);
    normalize_readme(&mut version, &root);
    assert_eq!(version.readme, "# Root README");
    assert_eq!(version.readme_file_name, "README.md");
    version.readme = "# Version README".into();
    version.readme_file_name = "README.txt".into();
    normalize_readme(&mut version, &root);
    assert_eq!(version.readme, "# Version README");
    assert_eq!(version.readme_file_name, "README.txt");
    let invalid = HashMap::from_iter([("readme".into(), json!(123))]);
    version.readme.clear();
    normalize_readme(&mut version, &invalid);
    assert_eq!(version.readme, "");
}

#[test]
fn deprecation_request_accepts_empty_messages_and_ignores_other_fields() {
    let body = json!({"name":"demo", "readme":"ignored", "dist-tags":{"latest":"wrong"},
        "versions":{"1.0.0":{"deprecated":"old", "description":"ignored"},
                    "2.0.0-beta.1":{"deprecated":""}, "2.0.0":{}}});
    let updates = deprecation_updates(&body, "demo").unwrap();
    assert_eq!(updates.len(), 2);
    assert_eq!(updates.get("1.0.0").map(String::as_str), Some("old"));
    assert_eq!(updates.get("2.0.0-beta.1").map(String::as_str), Some(""));
}

#[test]
fn deprecation_request_rejects_malformed_or_mismatched_documents() {
    for body in [
        json!({"name":"other", "versions":{"1.0.0":{"deprecated":"old"}}}),
        json!({"name":"demo"}),
        json!({"name":"demo", "versions":{}}),
        json!({"name":"demo", "versions":{"bad":{"deprecated":"old"}}}),
        json!({"name":"demo", "versions":{"1.0.0":{"deprecated":null}}}),
        json!({"name":"demo", "versions":{"1.0.0":{"deprecated":42}}}),
        json!({"name":"demo", "versions":{"1.0.0":null}}),
        json!({"name":"demo", "versions":{"1.0.0":{}}}),
    ] {
        assert!(deprecation_updates(&body, "demo").is_err(), "{body}");
    }
}
