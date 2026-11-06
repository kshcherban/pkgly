use super::extract_version_from_file;

#[test]
fn tarball_versions_preserve_identifiers_and_package_prefixes() {
    for package in ["my-lib", "@scope/my-lib", "lib-1.0.0"] {
        let name = package.rsplit('/').next().unwrap_or(package);
        for version in [
            "1.2.3",
            "1.2.3-beta.1",
            "1.2.3-RC.1+Build.X",
            "1.2.3-rc.1+build.x",
        ] {
            assert_eq!(
                extract_version_from_file(&format!("{name}-{version}.tgz"), package),
                Some(version.into())
            );
        }
    }
}

#[test]
fn tarball_versions_reject_invalid_names_and_extensions() {
    for file in [
        "my-lib.tgz",
        "my-lib-beta.1.tgz",
        "my-lib-1.2.3.zip",
        "my-lib-1.2.3.tgz.tgz",
        "other-1.2.3.tgz",
    ] {
        assert_eq!(extract_version_from_file(file, "my-lib"), None);
    }
}

#[test]
fn audit_actions_identify_scoped_tag_operations_and_deprecation() {
    use http::{HeaderMap, HeaderValue, Method};
    use nr_core::storage::StoragePath;
    for package in ["demo", "@scope/demo"] {
        for (method, suffix, expected) in [
            (Method::GET, "", "package.dist_tag.list"),
            (Method::HEAD, "", "package.dist_tag.list"),
            (Method::PUT, "/Beta", "package.dist_tag.set"),
            (Method::DELETE, "/release!", "package.dist_tag.delete"),
        ] {
            assert_eq!(
                super::audit_action(
                    &method,
                    &StoragePath::from(format!("-/package/{package}/dist-tags{suffix}")),
                    &HeaderMap::new()
                ),
                Some(expected)
            );
        }
        for command in ["deprecate", "undeprecate"] {
            let mut headers = HeaderMap::new();
            headers.insert("npm-command", HeaderValue::from_static(command));
            assert_eq!(
                super::audit_action(&Method::PUT, &StoragePath::from(package), &headers),
                Some("package.deprecation.update")
            );
        }
    }
    assert_eq!(
        super::audit_action(
            &Method::GET,
            &StoragePath::from("demo/-/demo-1.0.0.tgz"),
            &HeaderMap::new()
        ),
        None
    );
}
