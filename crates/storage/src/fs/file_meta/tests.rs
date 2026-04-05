#![allow(
    clippy::expect_used,
    clippy::field_reassign_with_default,
    clippy::panic,
    clippy::todo,
    clippy::unwrap_used
)]
use uuid::Uuid;

use super::LocationMeta;
use crate::meta::RepositoryMeta;
use tempfile::tempdir;
fn random_repo_meta() -> RepositoryMeta {
    let mut meta = RepositoryMeta::default();
    meta.project_id = Some(Uuid::new_v4());
    meta.project_version_id = Some(Uuid::new_v4());

    meta.insert("test", "map");

    meta
}
#[test]
pub fn post_card_compatible_meta_directory() {
    let meta = LocationMeta {
        created: chrono::Local::now().fixed_offset(),
        modified: chrono::Local::now().fixed_offset(),
        location_typed_meta: super::LocationTypedMeta::Directory(super::DirectoryMeta {
            number_of_files: 0,
        }),
        repository_meta: random_repo_meta(),
    };

    let bytes = postcard::to_allocvec(&meta).unwrap();

    let from_bytes: LocationMeta = postcard::from_bytes(&bytes).unwrap();

    assert_eq!(meta, from_bytes);
}

#[test]
fn save_meta_replaces_file_via_atomic_rename_on_unix() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("artifact.bin");
        std::fs::write(&file_path, b"payload").unwrap();

        let (meta, _) = LocationMeta::get_or_default_local(&file_path, None).unwrap();
        let meta_path = super::meta_path(&file_path).unwrap();
        let inode_before = std::fs::metadata(&meta_path).unwrap().ino();

        let mut updated = meta.clone();
        updated.repository_meta.insert("updated", "true");
        updated.save_meta(&file_path).unwrap();

        let inode_after = std::fs::metadata(&meta_path).unwrap().ino();
        assert_ne!(
            inode_before, inode_after,
            "atomic replace should swap the underlying file on save"
        );
    }
}

#[test]
fn generate_hashes_from_path_matches_generate_from_bytes() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("hash.bin");
    let payload = b"hello-pkgly";
    std::fs::write(&file_path, payload).unwrap();

    let from_path = super::generate_hashes_from_path(&file_path).unwrap();
    let from_bytes = super::generate_from_bytes(payload);
    assert_eq!(from_path, from_bytes);
}

#[test]
pub fn post_card_compatible_meta_file() {
    let meta = LocationMeta {
        created: chrono::Local::now().fixed_offset(),
        modified: chrono::Local::now().fixed_offset(),
        location_typed_meta: super::LocationTypedMeta::File(super::FileMeta {
            hashes: super::FileHashes {
                md5: Some("md5".to_string()),
                sha1: Some("sha1".to_string()),
                sha2_256: Some("sha2_256".to_string()),
                sha3_256: Some("sha3_256".to_string()),
            },
        }),
        repository_meta: random_repo_meta(),
    };

    let bytes = postcard::to_allocvec(&meta).unwrap();

    let from_bytes: LocationMeta = postcard::from_bytes(&bytes).unwrap();

    assert_eq!(meta, from_bytes);
}
