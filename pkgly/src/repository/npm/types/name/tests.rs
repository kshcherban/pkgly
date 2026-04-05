#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use core::panic;

use pretty_assertions::assert_eq;

use super::NPMPackageName;
#[test]
pub fn valid_packages() {
    let valid = vec![
        (
            "test",
            NPMPackageName {
                name: "test".to_string(),
                scope: None,
            },
        ),
        (
            "test-package",
            NPMPackageName {
                name: "test-package".to_string(),
                scope: None,
            },
        ),
        (
            "test_package",
            NPMPackageName {
                name: "test_package".to_string(),
                scope: None,
            },
        ),
        (
            "@scope/test",
            NPMPackageName {
                name: "test".to_string(),
                scope: Some("scope".to_string()),
            },
        ),
        (
            "@scope/test-package",
            NPMPackageName {
                name: "test-package".to_string(),
                scope: Some("scope".to_string()),
            },
        ),
        (
            "@scope/test_package",
            NPMPackageName {
                name: "test_package".to_string(),
                scope: Some("scope".to_string()),
            },
        ),
    ];
    for (package, expected) in valid {
        match super::NPMPackageName::try_from(package) {
            Ok(ok) => {
                assert_eq!(ok, expected);
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                panic!("Failed to parse package: {} \n error: {err}", package);
            }
        }
    }
}
