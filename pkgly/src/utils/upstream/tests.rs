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
