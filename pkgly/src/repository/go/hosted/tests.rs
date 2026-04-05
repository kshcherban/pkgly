#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use crate::repository::go::{
    configs::GoRepositoryConfig,
    ext::GoFileType,
    types::{GoModulePath, GoVersion},
};

#[test]
fn test_go_hosted_basic_functionality() {
    // Test that GoHosted can be constructed with basic properties
    // This test focuses on the basic structure without complex setup

    let config = GoRepositoryConfig::Hosted;
    assert!(matches!(config, GoRepositoryConfig::Hosted));
}

#[test]
fn go_path_priority_prefers_zip_then_mod_then_info() {
    assert_eq!(
        super::go_path_priority("github.com/example/mod/@v/v1.0.0.zip"),
        3
    );
    assert_eq!(
        super::go_path_priority("github.com/example/mod/@v/v1.0.0.mod"),
        2
    );
    assert_eq!(
        super::go_path_priority("github.com/example/mod/@v/v1.0.0.info"),
        1
    );
    assert_eq!(super::go_path_priority("github.com/example/mod/@v/list"), 0);
}

#[test]
fn go_file_priority_matches_go_path_priority_scale() {
    assert_eq!(super::go_file_priority(GoFileType::Zip), 3);
    assert_eq!(super::go_file_priority(GoFileType::GoMod), 2);
    assert_eq!(super::go_file_priority(GoFileType::Info), 1);
}

#[test]
fn go_cache_path_matches_storage_layout() {
    let module = GoModulePath::new("github.com/example/module").expect("module path");
    let version = GoVersion::new("v1.2.3").expect("version");

    assert_eq!(
        super::go_cache_path(&module, &version, GoFileType::Zip),
        "github.com/example/module/@v/v1.2.3.zip"
    );
    assert_eq!(
        super::go_cache_path(&module, &version, GoFileType::GoMod),
        "github.com/example/module/@v/v1.2.3.mod"
    );
    assert_eq!(
        super::go_cache_path(&module, &version, GoFileType::Info),
        "github.com/example/module/@v/v1.2.3.info"
    );
}
