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
