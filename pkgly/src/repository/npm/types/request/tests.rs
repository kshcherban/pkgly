#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use nr_core::storage::StoragePath;

use super::GetPath;
#[test]
pub fn tests() {
    let tests = vec![
        (
            StoragePath::from("@nr/mylib/-/@nr/mylib-1.0.0.tgz"),
            GetPath::GetTar {
                name: "@nr/mylib".to_string(),
                version: "1.0.0".to_string(),
                file: "mylib-1.0.0.tgz".to_string(),
            },
        ),
        (
            StoragePath::from("mylib/-/mylib-1.0.0.tgz"),
            GetPath::GetTar {
                name: "mylib".to_string(),
                version: "1.0.0".to_string(),
                file: "mylib-1.0.0.tgz".to_string(),
            },
        ),
        (
            StoragePath::from("mylib/1.0.0"),
            GetPath::VersionInfo {
                name: "mylib".to_string(),
                version: "1.0.0".to_string(),
            },
        ),
        (
            StoragePath::from("mylib"),
            GetPath::GetPackageInfo {
                name: "mylib".to_string(),
            },
        ),
        (
            StoragePath::from("npm-check-updates/-/npm-check-updates-11.0.3.tgz"),
            GetPath::GetTar {
                name: "npm-check-updates".to_string(),
                version: "11.0.3".to_string(),
                file: "npm-check-updates-11.0.3.tgz".to_string(),
            },
        ),
    ];
    for (path, expected) in tests {
        let get_path = GetPath::try_from(path).unwrap();
        assert_eq!(get_path, expected);
    }
}

#[test]
fn prerelease_tarball_paths_keep_the_complete_version() {
    for (path, name, file) in [
        (
            "my-lib/-/my-lib-1.2.3-beta.1.tgz",
            "my-lib",
            "my-lib-1.2.3-beta.1.tgz",
        ),
        (
            "@scope/my-lib/-/my-lib-1.2.3-beta.1.tgz",
            "@scope/my-lib",
            "my-lib-1.2.3-beta.1.tgz",
        ),
        (
            "@scope/my-lib/-/@scope/my-lib-1.2.3-beta.1.tgz",
            "@scope/my-lib",
            "my-lib-1.2.3-beta.1.tgz",
        ),
    ] {
        assert_eq!(
            GetPath::try_from(StoragePath::from(path)).unwrap(),
            GetPath::GetTar {
                name: name.into(),
                version: "1.2.3-beta.1".into(),
                file: file.into(),
            }
        );
    }
}

#[test]
fn malformed_scoped_paths_return_errors_without_panicking() {
    assert!(GetPath::try_from(StoragePath::from("@scope")).is_err());
}

#[test]
fn tarball_routes_require_the_marker_and_matching_legacy_scope() {
    for path in [
        "my-lib/not-a-marker/my-lib-1.2.3-beta.1.tgz",
        "@scope/my-lib/not-a-marker/my-lib-1.2.3-beta.1.tgz",
        "@scope/my-lib/-/@other/my-lib-1.2.3-beta.1.tgz",
    ] {
        assert!(
            GetPath::try_from(StoragePath::from(path)).is_err(),
            "{path}"
        );
    }
}

#[test]
fn tarball_versions_preserve_prerelease_and_build_case() {
    for version in ["1.2.3-RC.1+Build.AbC", "1.2.3-rc.1+build.abc"] {
        let path = StoragePath::from(format!("my-lib/-/my-lib-{version}.tgz"));
        let GetPath::GetTar {
            version: actual, ..
        } = GetPath::try_from(path).unwrap()
        else {
            panic!("expected tarball");
        };
        assert_eq!(actual, version);
    }
}

#[test]
fn tarball_validation_matches_case_insensitive_registry_lookup() {
    let dist = super::PublishDist {
        integrity: "hash".into(),
        shasum: "hash".into(),
        tarball: "https://registry.test/repositories/Store/Npm/demo/-/demo-1.0.0.tgz".into(),
        other: Default::default(),
    };
    assert!(dist.validate_tarball("store", "npm").is_ok());
}
