#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::collect_directories;
use std::{fs, path::Path};
use tempfile::tempdir;

#[test]
fn collect_directories_lists_child_directories_only() {
    let tmp = tempdir().expect("create temp dir");
    let base = tmp.path();
    let dir_a = base.join("alpha");
    fs::create_dir(&dir_a).expect("create dir");
    fs::write(base.join("file.txt"), b"data").expect("write file");

    let directories = collect_directories(Path::new(base)).expect("collect directories");

    assert_eq!(directories, vec!["alpha".to_string()]);
}
