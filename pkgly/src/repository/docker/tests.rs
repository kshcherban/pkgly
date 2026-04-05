#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::hosted::DockerHosted;

#[test]
fn docker_manifest_catalog_indexing_enabled_only_for_docker_repo_type() {
    assert!(DockerHosted::catalog_indexing_enabled_for_repository_type(
        "docker"
    ));
    assert!(!DockerHosted::catalog_indexing_enabled_for_repository_type(
        "helm"
    ));
    assert!(!DockerHosted::catalog_indexing_enabled_for_repository_type(
        ""
    ));
}
