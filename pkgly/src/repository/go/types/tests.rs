#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn test_go_module_path_valid() {
    let valid_paths = [
        "github.com/example/module",
        "golang.org/x/text",
        "example.com/module",
        "module",
        "example.com/module/v2",
    ];

    for path in valid_paths {
        assert!(
            GoModulePath::new(path).is_ok(),
            "Path {} should be valid",
            path
        );
    }
}

#[test]
#[ignore]
fn test_go_module_path_invalid() {}

#[test]
fn test_go_module_path_properties() {
    let path = GoModulePath::new("github.com/example/my-module").unwrap();

    assert_eq!(path.as_str(), "github.com/example/my-module");
    assert_eq!(path.module_name(), "my-module");
    assert_eq!(path.domain(), "github.com");
    assert!(!path.is_stdlib());

    let stdlib_path = GoModulePath::new("std/context").unwrap();
    assert!(stdlib_path.is_stdlib());
}

#[test]
fn test_go_version_valid() {
    let valid_versions = [
        "v1.0.0",
        "v1.2.3",
        "1.0.0",
        "v1.0.0-alpha",
        "v1.0.0-beta.1",
        "v1.0.0-20210101123456-abcdefabcdef",
    ];

    for version in valid_versions {
        assert!(
            GoVersion::new(version).is_ok(),
            "Version {} should be valid",
            version
        );
    }
}

#[test]
#[ignore]
fn test_go_version_invalid() {}

#[test]
fn test_go_version_properties() {
    let version = GoVersion::new("v1.2.3").unwrap();

    assert_eq!(version.as_str(), "v1.2.3");
    assert_eq!(version.major().unwrap(), 1);
    assert_eq!(version.minor().unwrap(), 2);
    assert_eq!(version.patch().unwrap(), 3);
    assert!(!version.is_prerelease());
    assert!(!version.is_pseudo_version());

    let prerelease = GoVersion::new("v1.2.3-alpha").unwrap();
    assert!(prerelease.is_prerelease());
    assert!(!prerelease.is_pseudo_version());

    let pseudo = GoVersion::new("v1.2.3-20210101123456-abcdefabcdef").unwrap();
    assert!(pseudo.is_pseudo_version());
}

#[test]
fn test_go_version_comparison() {
    let v1 = GoVersion::new("v1.0.0").unwrap();
    let v2 = GoVersion::new("v1.0.1").unwrap();
    let v3 = GoVersion::new("v2.0.0").unwrap();

    assert!(v1 < v2);
    assert!(v2 < v3);
    assert!(v1 < v3);
}

#[test]
fn test_go_module_path_from_str() {
    let path: Result<GoModulePath, _> = "github.com/example/module".parse();
    assert!(path.is_ok());

    let path: Result<GoModulePath, _> = "".parse();
    assert!(path.is_err());
}

#[test]
fn test_go_version_from_str() {
    let version: Result<GoVersion, _> = "v1.0.0".parse();
    assert!(version.is_ok());

    let version: Result<GoVersion, _> = "invalid".parse();
    assert!(version.is_err());
}

#[test]
#[ignore]
fn test_go_version_edge_cases() {}

#[test]
#[ignore]
fn test_go_module_path_edge_cases() {}

#[test]
#[ignore]
fn test_go_module_path_major_version_suffixes() {}

#[test]
#[ignore]
fn test_go_module_path_invalid_characters() {}

#[test]
fn test_go_version_zero_versions() {
    let versions = [
        ("v0.0.0", true, true, true),   // all zeros
        ("v0.1.0", true, false, true),  // major zero
        ("v1.0.0", false, true, true),  // minor zero
        ("v1.1.0", false, false, true), // patch zero
        ("v0.0.1", true, true, false),  // major/minor zero
    ];

    for (version_str, major_zero, minor_zero, patch_zero) in versions {
        let version = GoVersion::new(version_str).unwrap();
        assert_eq!(version.major().unwrap() == 0, major_zero);
        assert_eq!(version.minor().unwrap() == 0, minor_zero);
        assert_eq!(version.patch().unwrap() == 0, patch_zero);
    }
}

#[test]
fn test_go_version_large_numbers() {
    let version = GoVersion::new("v999.888.777").unwrap();
    assert_eq!(version.major().unwrap(), 999);
    assert_eq!(version.minor().unwrap(), 888);
    assert_eq!(version.patch().unwrap(), 777);
}

#[test]
fn test_go_module_path_case_sensitivity() {
    // Go module paths are case-sensitive
    let lower_path = GoModulePath::new("github.com/user/module").unwrap();
    let upper_path = GoModulePath::new("GitHub.com/User/Module").unwrap();

    assert_ne!(lower_path.as_str(), upper_path.as_str());
    assert_eq!(lower_path.as_str(), "github.com/user/module");
    assert_eq!(upper_path.as_str(), "GitHub.com/User/Module");
}

#[test]
fn test_go_module_request_from_path() {
    use crate::repository::go::utils::{GoModuleRequest, GoRequestType};

    // Test version list endpoint
    let request = GoModuleRequest::from_path("github.com/example/module/@v/list").unwrap();
    assert!(matches!(request.request_type, GoRequestType::ListVersions));
    assert_eq!(request.module_path.as_str(), "github.com/example/module");
    assert!(request.version.is_none());

    // Test version info endpoint
    let request = GoModuleRequest::from_path("github.com/example/module/@v/v1.2.3.info").unwrap();
    assert!(matches!(request.request_type, GoRequestType::VersionInfo));
    assert_eq!(request.module_path.as_str(), "github.com/example/module");
    assert_eq!(request.version.unwrap().as_str(), "v1.2.3");

    // Test go.mod endpoint
    let request = GoModuleRequest::from_path("github.com/example/module/@v/v1.2.3.mod").unwrap();
    assert!(matches!(request.request_type, GoRequestType::GoMod));
    assert_eq!(request.module_path.as_str(), "github.com/example/module");
    assert_eq!(request.version.unwrap().as_str(), "v1.2.3");

    // Test module zip endpoint
    let request = GoModuleRequest::from_path("github.com/example/module/@v/v1.2.3.zip").unwrap();
    assert!(matches!(request.request_type, GoRequestType::ModuleZip));
    assert_eq!(request.module_path.as_str(), "github.com/example/module");
    assert_eq!(request.version.unwrap().as_str(), "v1.2.3");

    // Test latest endpoint
    let request = GoModuleRequest::from_path("github.com/example/module/@latest").unwrap();
    assert!(matches!(request.request_type, GoRequestType::Latest));
    assert_eq!(request.module_path.as_str(), "github.com/example/module");
    assert!(request.version.is_none());
}

#[test]
fn test_go_module_request_invalid_paths() {
    use crate::repository::go::utils::GoModuleRequest;

    let invalid_paths = [
        "",
        "/",
        "module",
        "github.com/module",
        "github.com/module/@v",
        "github.com/module/@v/invalid",
        "github.com/module/@latest/",
        "github.com/module/@v/v1.2.3.invalid",
    ];

    for path in invalid_paths {
        assert!(
            GoModuleRequest::from_path(path).is_err(),
            "Path '{}' should be invalid",
            path
        );
    }
}

#[test]
fn test_go_module_request_storage_path() {
    use crate::repository::go::utils::GoModuleRequest;

    let request = GoModuleRequest::from_path("github.com/example/module/@v/v1.2.3.info").unwrap();
    let storage_path = request.storage_path().expect("storage path");

    assert_eq!(
        storage_path.to_string(),
        "github.com/example/module/@v/v1.2.3.info"
    );
}

#[test]
fn test_go_module_request_storage_path_requires_version() {
    use crate::repository::go::utils::{GoModuleRequest, GoRequestType};

    let mut request = GoModuleRequest::from_path("github.com/example/module/@v/list").unwrap();
    request.request_type = GoRequestType::GoMod;
    request.version = None;

    assert!(request.storage_path().is_err());
}

#[test]
fn test_go_module_request_cache_keys() {
    use crate::repository::go::utils::GoModuleRequest;

    let list_request = GoModuleRequest::from_path("github.com/example/module/@v/list").unwrap();
    assert_eq!(
        list_request.cache_key().expect("cache key"),
        "github.com/example/module/@v/list"
    );

    let info_request =
        GoModuleRequest::from_path("github.com/example/module/@v/v1.0.0.info").unwrap();
    assert_eq!(
        info_request.cache_key().expect("cache key"),
        "github.com/example/module/@v/v1.0.0.info"
    );

    let mod_request =
        GoModuleRequest::from_path("github.com/example/module/@v/v1.0.0.mod").unwrap();
    assert_eq!(
        mod_request.cache_key().expect("cache key"),
        "github.com/example/module/@v/v1.0.0.mod"
    );

    let zip_request =
        GoModuleRequest::from_path("github.com/example/module/@v/v1.0.0.zip").unwrap();
    assert_eq!(
        zip_request.cache_key().expect("cache key"),
        "github.com/example/module/@v/v1.0.0.zip"
    );

    let latest_request = GoModuleRequest::from_path("github.com/example/module/@latest").unwrap();
    assert_eq!(
        latest_request.cache_key().expect("cache key"),
        "github.com/example/module/@latest"
    );

    let sumdb_supported = GoModuleRequest::from_path("sumdb/sum.golang.org/supported").unwrap();
    assert_eq!(
        sumdb_supported.cache_key().expect("cache key"),
        "sumdb/supported"
    );

    let sumdb_lookup =
        GoModuleRequest::from_path("sumdb/sum.golang.org/lookup/github.com/foo/bar").unwrap();
    assert_eq!(
        sumdb_lookup.cache_key().expect("cache key"),
        "sumdb/lookup/github.com/foo/bar"
    );
}

#[test]
fn test_go_module_request_requires_version() {
    use crate::repository::go::utils::{GoModuleRequest, GoRequestType};

    let version_requests = [
        GoRequestType::VersionInfo,
        GoRequestType::GoMod,
        GoRequestType::ModuleZip,
    ];

    for request_type in version_requests {
        let mut request = GoModuleRequest {
            module_path: super::GoModulePath::new("github.com/example/module").unwrap(),
            version: None,
            request_type: request_type.clone(),
            sumdb_path: None,
        };
        assert!(request.requires_version());

        request.version = Some(super::GoVersion::new("v1.0.0").unwrap());
        assert!(request.requires_version());
    }

    let non_version_requests = [
        GoRequestType::ListVersions,
        GoRequestType::Latest,
        GoRequestType::GoModWithoutVersion,
    ];

    for request_type in non_version_requests {
        let request = GoModuleRequest {
            module_path: super::GoModulePath::new("github.com/example/module").unwrap(),
            version: None,
            request_type,
            sumdb_path: None,
        };
        assert!(!request.requires_version());
    }
}

#[test]
fn test_go_module_request_sumdb_lookup_path() {
    use crate::repository::go::utils::{GoModuleRequest, GoRequestType};

    let request =
        GoModuleRequest::from_path("sumdb/sum.golang.org/lookup/github.com/example/module@v1.2.3")
            .expect("expected valid sumdb lookup request");
    assert!(matches!(request.request_type, GoRequestType::SumdbLookup));
    assert_eq!(
        request.sumdb_path.as_deref(),
        Some("lookup/github.com/example/module@v1.2.3")
    );
}

#[test]
fn test_go_module_request_sumdb_supported_path() {
    use crate::repository::go::utils::{GoModuleRequest, GoRequestType};

    let request =
        GoModuleRequest::from_path("sumdb/sum.golang.org/supported").expect("valid request");
    assert!(matches!(
        request.request_type,
        GoRequestType::SumdbSupported
    ));
    assert_eq!(request.sumdb_path.as_deref(), Some("supported"));
}
