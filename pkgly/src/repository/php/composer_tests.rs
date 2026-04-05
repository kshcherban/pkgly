#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::composer::*;
use nr_core::storage::StoragePath;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

fn write_zip_with_composer(name: &str, version: &str) -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let zip_path = dir.path().join("package.zip");
    let file = File::create(&zip_path).expect("zip file create");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    writer
        .start_file("composer.json", options)
        .expect("start file");
    let composer = format!(r#"{{"name":"{name}","version":"{version}","type":"library"}}"#);
    writer
        .write_all(composer.as_bytes())
        .expect("write composer");
    writer.finish().expect("finish zip");
    (dir, zip_path)
}

#[test]
fn root_packages_json_uses_metadata_url() {
    let index = ComposerRootIndex::new("primary", "php-hosted");
    assert_eq!(
        index.metadata_url,
        "/repositories/primary/php-hosted/p2/%package%.json"
    );
    assert!(index.packages.is_empty());

    let serialized = serde_json::to_value(&index).expect("serialize packages.json");
    assert_eq!(serialized.get("packages").unwrap(), &json!([]));
}

#[test]
fn metadata_document_adds_dist_url_and_version() {
    let pkg = ComposerPackage::new(
        "acme/example".into(),
        "1.0.0".into(),
        serde_json::json!({"name":"acme/example","version":"1.0.0","type":"library"}),
    );
    let dist_url = "/repositories/primary/php-hosted/dist/acme/example/1.0.0.zip".to_string();
    let doc = ComposerMetadataDocument::with_version(&pkg, dist_url.clone(), Some("abc".into()));
    let versions = doc.packages.get("acme/example").expect("package entry");
    assert_eq!(versions.len(), 1);
    let first = &versions[0];
    assert_eq!(
        first.get("dist").and_then(|d| d.get("url")).unwrap(),
        dist_url.as_str()
    );
    assert_eq!(first.get("version").unwrap(), "1.0.0");
}

#[test]
fn extract_composer_json_from_zip() {
    let (_dir, zip_path) = write_zip_with_composer("acme/example", "2.4.0");
    let pkg = extract_composer_from_zip(&zip_path).expect("composer.json parsed");
    assert_eq!(pkg.name, "acme/example");
    assert_eq!(pkg.version, "2.4.0");
}

#[test]
fn composer_dist_path_allows_version_filename_layout() {
    let path = StoragePath::from("dist/acme/example/1.2.3.zip");
    let dist = ComposerDistPath::try_from(&path).expect("parse dist path");
    assert_eq!(dist.vendor, "acme");
    assert_eq!(dist.package, "example");
    assert_eq!(dist.version, "1.2.3");
    assert_eq!(dist.filename, "1.2.3.zip");
}

#[test]
fn composer_dist_path_rejects_non_zip_suffix() {
    let path = StoragePath::from("dist/acme/example/1.2.3.tar.gz");
    let err = ComposerDistPath::try_from(&path).expect_err("invalid path rejected");
    assert!(format!("{err:?}").contains("dist/<vendor>/<package>/<version>.zip"));
}

#[test]
fn validate_package_rejects_mismatched_path() {
    let pkg = ComposerPackage::new(
        "acme/example".into(),
        "1.0.0".into(),
        serde_json::json!({"name":"acme/example","version":"1.0.0"}),
    );
    let path = ComposerDistPath {
        vendor: "other".into(),
        package: "pkg".into(),
        version: "1.0.0".into(),
        filename: "pkg-1.0.0.zip".into(),
    };
    let err = validate_package_against_path(&pkg, &path).expect_err("should reject mismatch");
    assert!(format!("{err:?}").contains("name/version mismatch"));
}
