// ABOUTME: Tests outbound HTTP tracing, log sanitization, and egress enforcement.
// ABOUTME: Uses a real loopback listener to prove blocked literals are never contacted.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use opentelemetry::trace::TraceContextExt as _;
use opentelemetry::{
    Context as OtelContext, global,
    trace::{SpanContext, TraceId, TraceState},
};
use reqwest::header::HeaderMap;
use url::Url;

use super::{inject_trace_headers, sanitize_url_for_logging};

#[test]
fn sanitize_url_removes_userinfo_and_redacts_sensitive_query_values() {
    let url = Url::parse("https://user:pass@example.com/path?token=abc&ok=1").expect("url");
    let sanitized = sanitize_url_for_logging(&url);
    assert!(!sanitized.contains("user:pass@"), "{sanitized}");
    assert!(
        sanitized.contains("token=%3Credacted%3E") || sanitized.contains("token=<redacted>"),
        "{sanitized}"
    );
    assert!(sanitized.contains("ok=1"), "{sanitized}");
}

#[test]
fn inject_trace_headers_adds_traceparent() {
    global::set_text_map_propagator(opentelemetry_sdk::propagation::TraceContextPropagator::new());

    let span_context = SpanContext::new(
        TraceId::from_u128(0x1234),
        opentelemetry::trace::SpanId::from_u64(0x5678),
        opentelemetry::trace::TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    );
    let cx = OtelContext::new().with_remote_span_context(span_context);
    let mut headers = HeaderMap::new();
    inject_trace_headers(&cx, &mut headers);
    assert!(headers.contains_key("traceparent"));
}

#[tokio::test]
async fn send_blocks_initial_non_global_literal_before_connecting() {
    crate::utils::egress::install(&crate::app::config::EgressSettings::default())
        .expect("default egress policy");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_for_server = Arc::clone(&accepted);
    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            accepted_for_server.fetch_add(1, Ordering::SeqCst);
            use tokio::io::AsyncWriteExt as _;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await;
        }
    });

    let client = super::client_builder().build().expect("egress HTTP client");
    let error = super::send(&client, client.get(format!("http://{address}/")))
        .await
        .expect_err("non-global literal must be blocked");

    server.abort();
    assert!(
        crate::utils::egress::is_egress_blocked(&error),
        "unexpected error chain: {error:?}"
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 0);
}
