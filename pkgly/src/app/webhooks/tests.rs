#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;

#[test]
fn merge_headers_preserves_existing_secret_when_value_is_omitted() {
    let current = BTreeMap::from([("X-Token".to_string(), "secret-value".to_string())]);
    let headers = vec![WebhookHeaderInput {
        name: "X-Token".into(),
        value: None,
        configured: true,
    }];

    let merged = merge_headers(Some(&current), headers).expect("merge succeeds");

    assert_eq!(
        merged.get("X-Token").map(String::as_str),
        Some("secret-value")
    );
}

#[test]
fn merge_headers_rejects_new_header_without_secret() {
    let headers = vec![WebhookHeaderInput {
        name: "X-Token".into(),
        value: None,
        configured: false,
    }];

    let err = merge_headers(None, headers).expect_err("missing secret must fail");
    assert!(err.to_string().contains("Provide a value"));
}

#[test]
fn next_retry_at_uses_exponential_backoff() {
    let now = DateTime::parse_from_rfc3339("2026-04-22T10:00:00Z")
        .expect("time")
        .with_timezone(&Utc);

    let attempt_one = next_retry_at(now, 1).expect("retry");
    let attempt_two = next_retry_at(now, 2).expect("retry");
    let attempt_four = next_retry_at(now, 4).expect("retry");
    let exhausted = next_retry_at(now, 5);

    assert_eq!(attempt_one, now + chrono::Duration::minutes(1));
    assert_eq!(attempt_two, now + chrono::Duration::minutes(2));
    assert_eq!(attempt_four, now + chrono::Duration::minutes(8));
    assert!(exhausted.is_none());
}

#[test]
fn classify_http_response_retries_server_errors_only() {
    match classify_http_response(1, 503) {
        DeliveryAttemptOutcome::Retryable { http_status, .. } => {
            assert_eq!(http_status, Some(503));
        }
        other => panic!("expected retryable outcome, got {other:?}"),
    }

    match classify_http_response(1, 404) {
        DeliveryAttemptOutcome::Failed { http_status, .. } => {
            assert_eq!(http_status, Some(404));
        }
        other => panic!("expected failed outcome, got {other:?}"),
    }
}

#[test]
fn parse_manifest_reference_path_extracts_repository_and_reference() {
    let parsed = parse_manifest_reference_path("/v2/acme/widgets/manifests/latest")
        .expect("manifest path should parse");

    assert_eq!(parsed.0, "acme/widgets");
    assert_eq!(parsed.1, "latest");
}

#[test]
fn build_delivery_payload_contains_artifactory_style_envelope() {
    let snapshot = PackageWebhookSnapshot {
        event_type: WebhookEventType::PackagePublished,
        occurred_at: DateTime::parse_from_rfc3339("2026-04-22T10:00:00Z")
            .expect("time")
            .with_timezone(&Utc),
        canonical_path: "packages/acme/example/1.2.3/example-1.2.3.tgz".into(),
        actor: PackageWebhookActor {
            user_id: Some(42),
            username: Some("alice".into()),
        },
        repository: RepositorySnapshot {
            id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").expect("uuid"),
            name: "npm-hosted".into(),
            storage_name: "primary".into(),
            repository_type: "npm".into(),
        },
        package: PackageSnapshot {
            scope: Some("acme".into()),
            package_key: Some("@acme/example".into()),
            package_name: Some("example".into()),
            project_path: Some("packages/acme/example".into()),
            version: Some("1.2.3".into()),
            version_path: Some("packages/acme/example/1.2.3".into()),
        },
    };

    let payload = build_delivery_payload(&snapshot, "subscription-1");

    assert_eq!(payload["domain"], "package");
    assert_eq!(payload["event_type"], "package.published");
    assert_eq!(payload["subscription_key"], "subscription-1");
    assert_eq!(payload["data"]["actor"]["username"], "alice");
    assert_eq!(payload["data"]["repository"]["name"], "npm-hosted");
    assert_eq!(payload["data"]["package"]["version"], "1.2.3");
    assert_eq!(
        payload["data"]["package"]["canonical_path"],
        "packages/acme/example/1.2.3/example-1.2.3.tgz"
    );
}
