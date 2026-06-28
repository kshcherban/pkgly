// ABOUTME: Verifies build metadata normalization for API and startup display.
// ABOUTME: Covers full, short, and missing commit identifiers.
use super::{current_build_info, normalize_commit_id};

#[test]
fn normalizes_full_commit_id_to_short_value() {
    let commit = normalize_commit_id(Some("abcdef1234567890"));

    assert_eq!(commit.as_deref(), Some("abcdef1"));
}

#[test]
fn keeps_short_commit_id() {
    let commit = normalize_commit_id(Some("1234567"));

    assert_eq!(commit.as_deref(), Some("1234567"));
}

#[test]
fn rejects_missing_or_invalid_commit_id() {
    assert_eq!(normalize_commit_id(None), None);
    assert_eq!(normalize_commit_id(Some("")), None);
    assert_eq!(normalize_commit_id(Some("abc")), None);
    assert_eq!(normalize_commit_id(Some("not-sha")), None);
}

#[test]
fn current_build_info_uses_package_version() {
    let info = current_build_info();

    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
}
