#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use std::{
    io,
    sync::{Arc, Mutex},
};

use tracing_subscriber::{Layer, Registry, filter::Targets, layer::SubscriberExt};

use super::{
    AuditActor, AuditMetadata, AuditOutcome, classify_api_action, emit_http_audit_log,
    should_emit_audit,
};

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
fn classify_api_action_covers_expected_routes() {
    assert_eq!(classify_api_action("POST", "/api/install"), Some("system.install"));
    assert_eq!(classify_api_action("GET", "/api/user/me"), Some("auth.me"));
    assert_eq!(
        classify_api_action("POST", "/api/user/password-reset/request"),
        Some("auth.password_reset.request")
    );
    assert_eq!(
        classify_api_action("GET", "/api/repository/list"),
        Some("repository.list")
    );
    assert_eq!(
        classify_api_action("DELETE", "/api/repository/{repository_id}/packages"),
        Some("repository.package.delete")
    );
    assert_eq!(
        classify_api_action("GET", "/api/search/packages"),
        Some("package.search")
    );
    assert_eq!(classify_api_action("GET", "/api/info"), None);
}

#[test]
fn should_emit_audit_only_for_success_and_denied() {
    assert_eq!(should_emit_audit(200), Some(AuditOutcome::Success));
    assert_eq!(should_emit_audit(204), Some(AuditOutcome::Success));
    assert_eq!(should_emit_audit(401), Some(AuditOutcome::Denied));
    assert_eq!(should_emit_audit(403), Some(AuditOutcome::Denied));
    assert_eq!(should_emit_audit(400), None);
    assert_eq!(should_emit_audit(404), None);
    assert_eq!(should_emit_audit(500), None);
}

#[test]
fn emit_http_audit_log_uses_structured_fields() {
    let writer = BufferWriter::default();
    let layer = json_layer_for_target(writer.clone(), "pkgly::audit");
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("audit_test");
        let _guard = span.enter();

        emit_http_audit_log(
            &span,
            "GET",
            "/api/repository/list",
            "/api/repository/list",
            200,
            &AuditMetadata {
                actor: AuditActor {
                    username: "alice".to_string(),
                    user_id: Some(42),
                },
                action: Some("repository.list".to_string()),
                resource_kind: Some("repository".to_string()),
                resource_id: Some("repo-1".to_string()),
                resource_name: Some("primary".to_string()),
                repository_id: Some("repo-1".to_string()),
                storage_id: Some("storage-1".to_string()),
                target_user_id: Some(99),
                token_id: Some(7),
                path: Some("/packages".to_string()),
                query: Some("serde".to_string()),
            },
        );
    });

    let output = read_buffer(&writer);
    assert!(output.contains("\"message\":\"Audit event\""), "output was: {output}");
    assert!(
        output.contains("\"action\":\"repository.list\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"outcome\":\"success\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"actor_username\":\"alice\""),
        "output was: {output}"
    );
    assert!(output.contains("\"actor_id\":42"), "output was: {output}");
    assert!(
        output.contains("\"resource_kind\":\"repository\""),
        "output was: {output}"
    );
    assert!(output.contains("\"resource_id\":\"repo-1\""), "output was: {output}");
    assert!(
        output.contains("\"storage_id\":\"storage-1\""),
        "output was: {output}"
    );
    assert!(output.contains("\"token_id\":7"), "output was: {output}");
    assert!(output.contains("\"trace_id\":\""), "output was: {output}");
}

#[test]
fn emit_http_audit_log_uses_route_classifier_when_no_override_exists() {
    let writer = BufferWriter::default();
    let layer = json_layer_for_target(writer.clone(), "pkgly::audit");
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("audit_test");
        let _guard = span.enter();

        emit_http_audit_log(
            &span,
            "GET",
            "/api/search/packages",
            "/api/search/packages?q=tokio",
            403,
            &AuditMetadata::default(),
        );
    });

    let output = read_buffer(&writer);
    assert!(output.contains("\"action\":\"package.search\""), "output was: {output}");
    assert!(output.contains("\"outcome\":\"denied\""), "output was: {output}");
    assert!(
        output.contains("\"actor_username\":\"anonymous\""),
        "output was: {output}"
    );
}
