#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};

use crate::storage::StoragePath;
#[test]
fn prefix_slash() {
    let path = StoragePath::from("/test");
    assert_eq!(path.to_string(), "test");
    let path = StoragePath::from("/test/test2");
    assert_eq!(path.to_string(), "test/test2");
    let path = StoragePath::from("/test/test2/");
    assert_eq!(path.to_string(), "test/test2/");
    let path = StoragePath::from("/test/test2/test3");
    assert_eq!(path.to_string(), "test/test2/test3");
    let path = StoragePath::from("/test/test2/test3/");
    assert_eq!(path.to_string(), "test/test2/test3/");
}
#[test]
fn test_from_and_into() {
    let path = StoragePath::from("test/test2");
    assert_eq!(path.to_string(), "test/test2");
    let path = StoragePath::from("test/test2/");
    assert_eq!(path.to_string(), "test/test2/");
    let path = StoragePath::from("test/test2/test3");
    assert_eq!(path.to_string(), "test/test2/test3");
    let path = StoragePath::from("test/test2/test3/");
    assert_eq!(path.to_string(), "test/test2/test3/");
}
#[test]
fn double_slash() {
    let path = StoragePath::from("test//test2");
    assert_eq!(path.to_string(), "test/test2");
    let path = StoragePath::from("test//test2/");
    assert_eq!(path.to_string(), "test/test2/");
    let path = StoragePath::from("test/test2//test3");
    assert_eq!(path.to_string(), "test/test2/test3");
    let path = StoragePath::from("test/test2//test3/");
    assert_eq!(path.to_string(), "test/test2/test3/");
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Test {
    path: StoragePath,
}
#[test]
fn serde() {
    let paths = vec![
        "test/test2",
        "test/test2/",
        "test/test2/test3",
        "test/test2/test3/",
        "/test/test2",
    ];
    for path in paths {
        let test = Test {
            path: StoragePath::from(path),
        };
        let serialized = serde_json::to_string(&test).unwrap();
        let deserialized: Test = serde_json::from_str(&serialized).unwrap();
        assert_eq!(test, deserialized);
    }
}
