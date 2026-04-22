#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::{
    PackageFileSortBy, SortDirection, file_name_from_path, normalize_digest, sort_expression,
};

#[test]
fn normalize_digest_prefixes_plain_hashes() {
    assert_eq!(
        normalize_digest(Some("deadbeef")).as_deref(),
        Some("sha256:deadbeef")
    );
    assert_eq!(
        normalize_digest(Some("sha256:deadbeef")).as_deref(),
        Some("sha256:deadbeef")
    );
}

#[test]
fn file_name_from_path_uses_last_segment() {
    assert_eq!(
        file_name_from_path("packages/acme/acme-1.0.0.tgz", "fallback"),
        "acme-1.0.0.tgz"
    );
    assert_eq!(file_name_from_path("", "fallback"), "fallback");
}

#[test]
fn sort_expression_covers_all_variants() {
    assert_eq!(sort_expression(PackageFileSortBy::Modified), "modified_at");
    assert_eq!(
        sort_expression(PackageFileSortBy::Package),
        "LOWER(package) COLLATE \"C\""
    );
    assert_eq!(
        sort_expression(PackageFileSortBy::Name),
        "LOWER(name) COLLATE \"C\""
    );
    assert_eq!(sort_expression(PackageFileSortBy::Size), "size_bytes");
    assert_eq!(
        sort_expression(PackageFileSortBy::Path),
        "LOWER(path) COLLATE \"C\""
    );
    assert_eq!(
        sort_expression(PackageFileSortBy::Digest),
        "LOWER(COALESCE(content_digest, upstream_digest, '')) COLLATE \"C\""
    );
}

#[test]
fn sort_direction_defaults_to_desc() {
    assert_eq!(SortDirection::default(), SortDirection::Desc);
}
