#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn parses_php_path() {
    let path = StoragePath::from("acme/example/1.0.0/example-1.0.0.zip");
    let info = PhpPackagePathInfo::try_from(&path).unwrap();
    assert_eq!(info.vendor, "acme");
    assert_eq!(info.package, "example");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(info.package_name(), "acme/example");
    assert_eq!(info.version_storage_path(), "acme/example/1.0.0");
}

#[test]
fn rejects_short_path() {
    let path = StoragePath::from("acme/example.zip");
    assert!(PhpPackagePathInfo::try_from(&path).is_err());
}
