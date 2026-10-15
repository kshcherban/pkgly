#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn parses_python_path() {
    let path = StoragePath::from("example_pkg/1.0.0/example_pkg-1.0.0-py3-none-any.whl");
    let info = PythonPackagePathInfo::try_from(&path).unwrap();
    assert_eq!(info.package, "example_pkg");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(
        info.file_name,
        "example_pkg-1.0.0-py3-none-any.whl".to_string()
    );
    assert_eq!(info.project_key(), "example-pkg");
    assert_eq!(info.version_storage_path(), "example-pkg/1.0.0");
}

#[test]
fn normalizes_package_name() {
    assert_eq!(normalize_package_name("Example_Pkg"), "example-pkg");
    assert_eq!(normalize_package_name("Example.Pkg"), "example-pkg");
}

#[test]
fn rejects_short_path() {
    let path = StoragePath::from("example_pkg/file.whl");
    let result = PythonPackagePathInfo::try_from(&path);
    assert!(result.is_err());
}

#[test]
fn redirect_uses_canonical_base_for_root_and_simple_index_paths() {
    for (path, expected_location) in [
        ("", "/repositories/api/python-hosted/"),
        ("simple", "/repositories/api/python-hosted/simple/"),
        (
            "simple/demo",
            "/repositories/api/python-hosted/simple/demo/",
        ),
        (
            "simple/demo/",
            "/repositories/api/python-hosted/simple/demo/",
        ),
    ] {
        let response =
            redirect_to_trailing_slash("/repositories/api/python-hosted", &StoragePath::from(path));

        assert_eq!(response.status(), http::StatusCode::MOVED_PERMANENTLY);
        assert_eq!(
            response.headers()[http::header::LOCATION],
            expected_location
        );
    }
}
