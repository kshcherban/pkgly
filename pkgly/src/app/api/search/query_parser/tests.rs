#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use crate::app::api::search::version_constraint::VersionConstraint;

#[test]
fn parse_simple_term() {
    let result = parse_search_query("gin").expect("query should parse");
    assert_eq!(result.terms, vec!["gin".to_string()]);
    assert!(result.package_filter.is_none());
    assert!(result.version_constraint.is_none());
}

#[test]
fn parse_package_filter() {
    let result = parse_search_query("package:gin").expect("query should parse");
    assert_eq!(
        result.package_filter,
        Some((Operator::Contains, "gin".to_string()))
    );
    assert!(result.terms.is_empty());
}

#[test]
fn parse_package_filter_with_whitespace_after_colon() {
    let result = parse_search_query("pkg: hello-pkg").expect("query should parse");
    assert_eq!(
        result.package_filter,
        Some((Operator::Contains, "hello-pkg".to_string()))
    );
    assert!(result.terms.is_empty());
}

#[test]
fn parse_package_filter_with_explicit_equals_operator() {
    let result = parse_search_query("pkg:=hello").expect("query should parse");
    assert_eq!(
        result.package_filter,
        Some((Operator::Equals, "hello".to_string()))
    );
}

#[test]
fn parse_version_constraint_greater_than() {
    let result = parse_search_query("version:>1.2.3").expect("query should parse");
    assert_eq!(
        result.version_constraint,
        Some(VersionConstraint::Range {
            op: Operator::GreaterThan,
            version: "1.2.3".to_string()
        })
    );
}

#[test]
fn parse_combined_query() {
    let query = parse_search_query("package:~express version:>=4.0.0 type:npm redis").expect("ok");
    assert_eq!(
        query.package_filter,
        Some((Operator::Contains, "express".to_string()))
    );
    assert_eq!(
        query.version_constraint,
        Some(VersionConstraint::Range {
            op: Operator::GreaterOrEqual,
            version: "4.0.0".to_string()
        })
    );
    assert_eq!(query.type_filter, Some("npm".to_string()));
    assert_eq!(query.terms, vec!["redis".to_string()]);
}

#[test]
fn parse_semver_requirement() {
    let query = parse_search_query("version:^1.5").expect("query should parse");
    assert_eq!(
        query.version_constraint,
        Some(VersionConstraint::Semver(
            VersionReq::parse("^1.5").expect("valid semver requirement")
        ))
    );
}

#[test]
fn simple_term_matches_partial_values() {
    let query = parse_search_query("hello").expect("query should parse");
    assert!(query.matches_terms(&["hello"]));
    assert!(query.matches_terms(&["hello-world"]));
}

#[test]
fn rejects_invalid_operator_for_repository() {
    let err = parse_search_query("repo:~staging").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidOperator(_, Field::Repository)
    ));
}

#[test]
fn parse_multiple_terms_and_filters() {
    let query = parse_search_query("pkg:\"my pkg\" version:=2.0 storage:primary latest").unwrap();
    assert_eq!(
        query.package_filter,
        Some((Operator::Equals, "my pkg".to_string()))
    );
    assert_eq!(
        query.version_constraint,
        Some(VersionConstraint::Exact("2.0".to_string()))
    );
    assert_eq!(query.storage_filter, Some("primary".to_string()));
    assert_eq!(query.terms, vec!["latest".to_string()]);
}

#[test]
fn parse_digest_filter_with_alias() {
    let query = parse_search_query("digest:sha256:deadbeef").expect("query should parse");
    assert_eq!(
        query.digest_filter,
        Some((Operator::Equals, "sha256:deadbeef".to_string()))
    );
}

#[test]
fn parse_hash_filter_defaults_to_contains() {
    let query = parse_search_query("hash:deadbeef").expect("query should parse");
    assert_eq!(
        query.digest_filter,
        Some((Operator::Contains, "deadbeef".to_string()))
    );
}

#[test]
fn rejects_range_operator_for_digest_filter() {
    let err = parse_search_query("digest:>sha256:deadbeef").unwrap_err();
    assert!(matches!(err, ParseError::InvalidOperator(_, Field::Digest)));
}
