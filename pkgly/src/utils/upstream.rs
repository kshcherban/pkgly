use std::time::Instant;

use http::HeaderValue;
use opentelemetry::{
    Context as OtelContext, global, propagation::Injector, trace::TraceContextExt as _,
};
use reqwest::header::HeaderMap;
use tracing::{Instrument as _, Span, info_span};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use url::Url;

struct HeaderMapInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderMapInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let Ok(header_value) = HeaderValue::from_str(&value) else {
            return;
        };
        if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
            self.0.insert(header_name, header_value);
        }
    }
}

fn inject_trace_headers(context: &OtelContext, headers: &mut HeaderMap) {
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(context, &mut HeaderMapInjector(headers))
    });
}

fn trace_id_from_span(span: &Span) -> String {
    span.context().span().span_context().trace_id().to_string()
}

fn span_context_is_valid(span: &Span) -> bool {
    span.context().span().span_context().is_valid()
}

pub fn sanitize_url_for_logging(url: &Url) -> String {
    let mut sanitized = url.clone();
    let _ = sanitized.set_username("");
    let _ = sanitized.set_password(None);
    sanitized.set_fragment(None);

    if sanitized.query().is_some() {
        let mut pairs: Vec<(String, String)> = Vec::new();
        if let Some(query) = sanitized.query() {
            for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
                let key = k.to_string();
                let lower = key.to_ascii_lowercase();
                let value = if is_sensitive_query_key(&lower) {
                    "<redacted>".to_string()
                } else {
                    v.to_string()
                };
                pairs.push((key, value));
            }
        }
        sanitized.set_query(None);
        if !pairs.is_empty() {
            sanitized.query_pairs_mut().extend_pairs(pairs);
        }
    }

    let mut out = sanitized.to_string();
    const MAX_LEN: usize = 1024;
    if out.len() > MAX_LEN {
        out.truncate(MAX_LEN);
        out.push_str("…");
    }
    out
}

fn is_sensitive_query_key(key_lower: &str) -> bool {
    matches!(
        key_lower,
        "token"
            | "access_token"
            | "id_token"
            | "auth"
            | "authorization"
            | "password"
            | "signature"
            | "sig"
            | "key"
            | "x-amz-signature"
            | "x-amz-credential"
            | "x-amz-security-token"
    )
}

pub async fn send(
    client: &reqwest::Client,
    builder: reqwest::RequestBuilder,
) -> Result<reqwest::Response, reqwest::Error> {
    let request = builder.build()?;
    execute(client, request).await
}

pub async fn execute(
    client: &reqwest::Client,
    mut request: reqwest::Request,
) -> Result<reqwest::Response, reqwest::Error> {
    let method = request.method().as_str().to_string();
    let url = request.url().clone();
    let sanitized_url = sanitize_url_for_logging(&url);

    let host = url.host_str().unwrap_or("");
    let port = url.port_or_known_default();

    let span = info_span!(
        target: "pkgly::upstream",
        "Upstream HTTP",
        otel.kind = ?opentelemetry::trace::SpanKind::Client,
        http.request.method = %method,
        url.full = %sanitized_url,
        server.address = %host,
        server.port = port.map(|p| p as i64),
        otel.name = %format!("{method} {host}"),
    );

    if span_context_is_valid(&span) {
        inject_trace_headers(&span.context(), request.headers_mut());
    }

    let start = Instant::now();
    let response = client.execute(request).instrument(span.clone()).await;

    match &response {
        Ok(res) => {
            let duration_ms = start.elapsed().as_millis() as i64;
            let trace_id = trace_id_from_span(&span);
            tracing::info!(
                target: "pkgly::upstream_access",
                trace_id = %trace_id,
                http.request.method = %method,
                url.full = %sanitized_url,
                http.response.status_code = res.status().as_u16() as i64,
                duration_ms = duration_ms,
                "Upstream HTTP access"
            );
        }
        Err(err) => {
            let duration_ms = start.elapsed().as_millis() as i64;
            let trace_id = trace_id_from_span(&span);
            tracing::warn!(
                target: "pkgly::upstream_access",
                trace_id = %trace_id,
                http.request.method = %method,
                url.full = %sanitized_url,
                duration_ms = duration_ms,
                error = %err,
                "Upstream HTTP failed"
            );
        }
    }

    response
}

#[cfg(test)]
mod tests;
