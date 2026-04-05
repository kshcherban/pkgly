#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn version_constraint_matches_semver() {
    let req = VersionReq::parse(">=1.2.0").unwrap();
    let constraint = VersionConstraint::Semver(req);
    assert!(constraint.matches("1.3.0"));
    assert!(!constraint.matches("1.1.9"));
}

#[test]
fn version_constraint_matches_string_comparison() {
    let constraint = VersionConstraint::Range {
        op: Operator::GreaterOrEqual,
        version: "2024.10".to_string(),
    };
    assert!(constraint.matches("2024.11"));
    assert!(!constraint.matches("2024.09"));
}

#[test]
fn version_constraint_contains_handles_substrings() {
    let constraint = VersionConstraint::Range {
        op: Operator::Contains,
        version: "beta".to_string(),
    };
    assert!(constraint.matches("1.0.0-beta.1"));
    assert!(!constraint.matches("1.0.0"));
}

#[test]
fn version_constraint_exact_handles_leading_v() {
    let constraint = VersionConstraint::Exact("v1.2.3".to_string());
    assert!(constraint.matches("1.2.3"));
    assert!(constraint.matches("v1.2.3"));
    assert!(!constraint.matches("1.2.4"));
}

#[test]
fn parse_debian_semver_converts_revision() {
    let parsed = super::parse_debian_semver("547.0.0-0").expect("parse debian version");
    assert_eq!(parsed.to_string(), "547.0.0");
}

#[test]
fn semver_constraint_matches_debian_version() {
    let req = VersionReq::parse("^547.0").unwrap();
    let constraint = VersionConstraint::Semver(req);
    assert!(constraint.matches("547.0.0-0"));
}

#[test]
fn range_constraint_handles_debian_version_without_patch() {
    let constraint = VersionConstraint::Range {
        op: Operator::GreaterThan,
        version: "545.0".to_string(),
    };
    assert!(constraint.matches("547.0.0-1"));
    assert!(!constraint.matches("544.9.0-1"));
}
