#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::repository::npm::login::is_npm_login_path;
use nr_core::storage::StoragePath;

#[test]
fn recognizes_login_paths() {
    assert!(is_npm_login_path(&StoragePath::from(
        "-/user/org.couchdb.user:alice"
    )));
    assert!(is_npm_login_path(&StoragePath::from("-/v1/login")));
    assert!(!is_npm_login_path(&StoragePath::from("left-pad")));
    assert!(!is_npm_login_path(&StoragePath::from(
        "packages/left-pad/-/left-pad-1.0.0.tgz"
    )));
}
