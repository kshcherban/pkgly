#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

#[test]
fn publish_request_reads_dist_tags() {
    let request: super::super::types::PublishRequest = serde_json::from_value(json!({
        "name": "my-lib", "versions": {}, "_attachments": {},
        "dist-tags": {"beta": "2.0.0-beta.1"}
    }))
    .unwrap();
    assert_eq!(
        request.dist_tags.get("beta").map(String::as_str),
        Some("2.0.0-beta.1")
    );
    assert!(!request.other.contains_key("dist-tags"));
}

#[test]
fn tagged_prerelease_does_not_request_latest() {
    let tags = HashMap::from_iter([("beta".into(), "2.0.0-beta.1".into())]);
    assert_eq!(publish_tags(tags.clone(), "2.0.0-beta.1").unwrap(), tags);
}

#[test]
fn empty_tags_default_to_latest_and_explicit_latest_accepts_prereleases() {
    for version in ["1.0.0", "2.0.0-beta.1"] {
        let tags = publish_tags(HashMap::default(), version).unwrap();
        assert_eq!(tags.get("latest").map(String::as_str), Some(version));
        assert_eq!(publish_tags(tags.clone(), version).unwrap(), tags);
    }
}

#[test]
fn publish_tags_reject_mismatched_targets() {
    let tags = HashMap::from_iter([("beta".into(), "9.0.0".into())]);
    assert!(publish_tags(tags, "2.0.0-beta.1").is_err());
}

#[test]
fn legacy_latest_uses_the_newest_stable_upload() {
    let tags = legacy_dist_tags(["2.0.0-beta.1", "1.0.0", "1.1.0"]);
    assert_eq!(tags.get("latest").map(String::as_str), Some("1.0.0"));
    assert!(legacy_dist_tags(["2.0.0-beta.1", "invalid"]).is_empty());
}

#[test]
fn tag_route_supports_scoped_and_unscoped_packages() {
    for name in ["my-lib", "@scope/my-lib", "dist-tags", "@scope/dist-tags"] {
        assert_eq!(
            parse_tag_path(&format!("-/package/{name}/dist-tags")),
            Some((name.into(), None))
        );
        assert_eq!(
            parse_tag_path(&format!("-/package/{name}/dist-tags/beta")),
            Some((name.into(), Some("beta".into())))
        );
    }
    assert!(parse_tag_path("my-lib").is_none());
    assert!(parse_tag_path("-/package/my-lib/dist-tags/beta/extra").is_none());
}

#[test]
fn tag_names_reject_versions_ranges_and_path_segments() {
    for tag in ["latest", "beta", "next", "release_candidate", "canary.1"] {
        assert!(valid_tag(tag), "{tag}");
    }
    for tag in ["", "1", "v1", "1.2.3", "beta/extra", "beta next", "^1.0.0"] {
        assert!(!valid_tag(tag), "{tag}");
    }
}

#[test]
fn tag_route_can_address_a_tag_named_dist_tags() {
    assert_eq!(
        parse_tag_path("-/package/my-lib/dist-tags/dist-tags"),
        Some(("my-lib".into(), Some("dist-tags".into())))
    );
}

mod concurrency;
mod migration;

#[test]
fn npm_compatible_tag_punctuation_is_accepted() {
    for tag in ["release!", "release(candidate)", "release'candidate"] {
        assert!(valid_tag(tag), "{tag}");
    }
}

#[test]
fn tag_routes_preserve_case() {
    for tag in ["Beta", "beta"] {
        assert_eq!(
            parse_tag_path(&format!("-/package/@scope/my-lib/dist-tags/{tag}")),
            Some(("@scope/my-lib".into(), Some(tag.into())))
        );
    }
}

#[test]
fn tag_names_reject_unicode_and_percent_encoded_input() {
    for tag in [
        "béta",
        "候補",
        "β",
        "release%21",
        "release%2521",
        "release/next",
        "release\n",
    ] {
        assert!(!valid_tag(tag), "{tag}");
    }
    for tag in [
        "release*",
        "release~candidate",
        "Release!",
        "release_candidate",
    ] {
        assert!(valid_tag(tag), "{tag}");
    }
}
