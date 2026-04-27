#![allow(clippy::expect_used)]
use super::*;

use crate::repository::npm::NPMRegistryConfigType;
use nr_core::repository::config::RepositoryConfigType;

#[test]
fn package_retention_actor_is_system_scoped() {
    let actor = package_retention_actor();

    assert!(actor.user_id.is_none());
    assert_eq!(actor.username.as_deref(), Some("package-retention"));
}

#[test]
fn config_key_is_stable() {
    assert_eq!(
        config::PackageRetentionConfigType::get_type_static(),
        "package_retention"
    );
    assert_ne!(
        config::PackageRetentionConfigType::get_type_static(),
        NPMRegistryConfigType::get_type_static()
    );
}
