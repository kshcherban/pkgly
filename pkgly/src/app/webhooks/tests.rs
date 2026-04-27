#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing_subscriber::{Layer, Registry, filter::Targets, layer::SubscriberExt};

use super::*;

#[derive(Clone, Default)]
struct BufferWriter(Arc<Mutex<Vec<u8>>>);

struct BufferGuard(Arc<Mutex<Vec<u8>>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for BufferWriter {
    type Writer = BufferGuard;

    fn make_writer(&'a self) -> Self::Writer {
        BufferGuard(self.0.clone())
    }
}

impl io::Write for BufferGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut locked = self
            .0
            .lock()
            .map_err(|_| io::Error::other("poisoned mutex"))?;
        locked.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn read_buffer(writer: &BufferWriter) -> String {
    let locked = writer.0.lock().expect("test mutex should not be poisoned");
    String::from_utf8_lossy(&locked).to_string()
}

fn json_layer_for_target(
    writer: BufferWriter,
    target: &'static str,
) -> impl tracing_subscriber::Layer<Registry> {
    let targets: Targets = Targets::new()
        .with_default(tracing::level_filters::LevelFilter::OFF)
        .with_target(target, tracing::level_filters::LevelFilter::INFO);

    tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .json()
        .without_time()
        .with_current_span(false)
        .with_span_list(false)
        .with_filter(targets)
}

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

#[test]
fn webhook_delivery_header_names_for_logging_omit_values() {
    let headers = BTreeMap::from([
        (
            "Authorization".to_string(),
            "Bearer secret-token".to_string(),
        ),
        ("X-Token".to_string(), "custom-secret".to_string()),
    ]);

    let names = webhook_delivery_header_names_for_logging(&headers);
    let rendered = format!("{names:?}");

    assert_eq!(
        names,
        vec!["Authorization".to_string(), "X-Token".to_string()]
    );
    assert!(!rendered.contains("secret-token"), "{rendered}");
    assert!(!rendered.contains("custom-secret"), "{rendered}");
}

#[test]
fn webhook_payload_summary_extracts_safe_debug_fields() {
    let payload = json!({
        "event_id": "event-123",
        "body_only_secret": "do-not-log",
        "data": {
            "repository": {
                "id": "repo-123",
                "name": "npm-hosted",
                "storage_name": "primary",
                "format": "npm"
            },
            "package": {
                "key": "@acme/example",
                "name": "example",
                "version": "1.2.3",
                "canonical_path": "packages/acme/example/1.2.3/example.tgz",
                "internal_secret": "do-not-log-either"
            }
        }
    });

    let summary = WebhookPayloadLogSummary::from_payload(&payload);
    let rendered = format!("{summary:?}");

    assert_eq!(summary.event_id.as_deref(), Some("event-123"));
    assert_eq!(summary.repository_id.as_deref(), Some("repo-123"));
    assert_eq!(summary.repository_name.as_deref(), Some("npm-hosted"));
    assert_eq!(summary.repository_storage_name.as_deref(), Some("primary"));
    assert_eq!(summary.repository_format.as_deref(), Some("npm"));
    assert_eq!(summary.package_key.as_deref(), Some("@acme/example"));
    assert_eq!(summary.package_name.as_deref(), Some("example"));
    assert_eq!(summary.package_version.as_deref(), Some("1.2.3"));
    assert_eq!(
        summary.package_canonical_path.as_deref(),
        Some("packages/acme/example/1.2.3/example.tgz")
    );
    assert!(!rendered.contains("do-not-log"), "{rendered}");
    assert!(!rendered.contains("do-not-log-either"), "{rendered}");
}

#[test]
fn webhook_target_summary_uses_sanitized_url_for_logging() {
    let summary = WebhookTargetLogSummary::from_target_url(
        "https://user:pass@example.com:8443/hook?token=abc&safe=ok#fragment",
    );

    assert_eq!(summary.host.as_deref(), Some("example.com"));
    assert_eq!(summary.port, Some(8443));
    assert!(!summary.target_url.contains("user:pass@"), "{summary:?}");
    assert!(!summary.target_url.contains("abc"), "{summary:?}");
    assert!(!summary.target_url.contains("fragment"), "{summary:?}");
    assert!(
        summary.target_url.contains("token=%3Credacted%3E")
            || summary.target_url.contains("token=<redacted>"),
        "{summary:?}"
    );
    assert!(summary.target_url.contains("safe=ok"), "{summary:?}");
}

#[test]
fn webhook_delivery_logs_safe_attempt_and_outcome_fields() {
    let writer = BufferWriter::default();
    let layer = json_layer_for_target(writer.clone(), WEBHOOK_DELIVERY_LOG_TARGET);
    let subscriber = Registry::default().with(layer);
    let delivery = ClaimedDelivery {
        id: 7,
        claim_token: Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("uuid"),
        webhook_id: Some(Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("uuid")),
        webhook_name: "release-webhook".to_string(),
        event_type: "package.published".to_string(),
        subscription_key: "subscription-1".to_string(),
        target_url: "https://example.com/hook?token=secret-query&safe=ok".to_string(),
        headers: BTreeMap::from([
            (
                "Authorization".to_string(),
                "Bearer secret-token".to_string(),
            ),
            ("X-Token".to_string(), "custom-secret".to_string()),
        ]),
        payload: json!({
            "event_id": "event-123",
            "body_only_secret": "request-body-secret",
            "data": {
                "repository": {
                    "id": "repo-123",
                    "name": "npm-hosted",
                    "storage_name": "primary",
                    "format": "npm"
                },
                "package": {
                    "key": "@acme/example",
                    "name": "example",
                    "version": "1.2.3",
                    "canonical_path": "packages/acme/example/1.2.3/example.tgz"
                }
            }
        }),
        attempts: 1,
    };

    tracing::subscriber::with_default(subscriber, || {
        log_webhook_delivery_attempt_started(&delivery, 2);
        log_webhook_delivery_outcome(
            &delivery,
            &DeliveryAttemptOutcome::Delivered {
                http_status: Some(204),
            },
            Duration::from_millis(17),
        );
    });

    let output = read_buffer(&writer);
    assert!(
        output.contains("\"message\":\"Webhook delivery attempt started\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"message\":\"Webhook delivery succeeded\""),
        "output was: {output}"
    );
    assert!(output.contains("\"delivery_id\":7"), "output was: {output}");
    assert!(
        output.contains("\"webhook_id\":\"11111111-1111-1111-1111-111111111111\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"header_names\":\"[\\\"Authorization\\\", \\\"X-Token\\\"]\""),
        "output was: {output}"
    );
    assert!(
        output.contains("token=%3Credacted%3E"),
        "output was: {output}"
    );
    assert!(
        output.contains("\"http.response.status_code\":204"),
        "output was: {output}"
    );
    assert!(!output.contains("secret-token"), "output was: {output}");
    assert!(!output.contains("custom-secret"), "output was: {output}");
    assert!(!output.contains("secret-query"), "output was: {output}");
    assert!(
        !output.contains("request-body-secret"),
        "output was: {output}"
    );
}
