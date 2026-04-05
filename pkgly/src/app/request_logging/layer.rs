use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    body::{Body, HttpBody},
    extract::MatchedPath,
};
use http::{HeaderValue, Request, Response, header::InvalidHeaderValue};
use opentelemetry::KeyValue;
use pin_project::pin_project;
use tower_service::Service;
use tracing::error;

use super::{X_REQUEST_ID, response_body::TraceResponseBody};
use crate::{
    app::{AppMetrics, Pkgly},
    utils::request_logging::{
        access_log::AccessLogContext, request_id::RequestId, request_span::RequestSpan,
    },
};

/// Tracks active requests using an up-down counter. Ensures we always decrement
/// even when a request ends in error or the body is dropped early.
#[derive(Clone)]
pub(crate) struct ActiveRequestGuard {
    metrics: AppMetrics,
    attributes: Vec<KeyValue>,
    finished: bool,
}

impl ActiveRequestGuard {
    pub fn start(metrics: &AppMetrics, attributes: Vec<KeyValue>) -> Self {
        metrics.active_requests.add(1, &attributes);
        Self {
            metrics: metrics.clone(),
            attributes,
            finished: false,
        }
    }

    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.metrics.active_requests.add(-1, &self.attributes);
        self.finished = true;
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Middleware that handles the authentication of the user
#[derive(Debug, Clone)]
pub struct AppTraceMiddleware<S> {
    pub(super) inner: S,
    pub(super) site: Pkgly,
}

impl<S> Service<Request<Body>> for AppTraceMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + Sync + Clone + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Display + 'static,
{
    type Response = Response<TraceResponseBody>;
    type Error = S::Error;
    //type Future = BoxFuture<'static, Result<Self::Response, S::Error>>;
    type Future = TraceResponseFuture<S::Future>;
    // Async Stuff we can ignore
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        let path = req
            .extensions()
            .get::<MatchedPath>()
            .map_or(req.uri().path(), |p| p.as_str());
        let http_route = path.to_owned();
        let url_path = req.uri().path().to_string();
        let http_method = req.method().as_str().to_string();
        let request_id = RequestId::new_random();
        let attributes = vec![
            KeyValue::new("http.route", http_route.clone()),
            KeyValue::new("http.request.method", http_method.clone()),
        ];
        let site: Pkgly = self.site.clone();
        let body_size = req.body().size_hint().lower();

        let access_log = AccessLogContext::default();
        req.extensions_mut().insert(access_log.clone());

        // Track active request immediately; guard will decrement at end of stream or on error.
        let active_request = ActiveRequestGuard::start(&site.metrics, attributes.clone());

        // Continue the request
        let mut inner = self.inner.clone();
        let start = std::time::Instant::now();

        let request_span = super::make_span(&req, request_id, &site);
        req.extensions_mut()
            .insert(RequestSpan(request_span.clone()));
        req.extensions_mut().insert(request_id);

        super::on_request(&req, &request_span, Some(body_size));

        let result = request_span.in_scope(|| inner.call(req));
        TraceResponseFuture {
            inner: result,
            instant: start,
            state: site,
            span: request_span,
            request_body_size: body_size,
            attributes,
            request_id,
            active_request: Some(active_request),
            access_log,
            http_route,
            http_method,
            url_path,
        }
    }
}

#[pin_project]
pub struct TraceResponseFuture<F> {
    #[pin]
    inner: F,
    instant: std::time::Instant,
    state: Pkgly,
    attributes: Vec<KeyValue>,
    span: tracing::Span,
    request_body_size: u64,

    request_id: RequestId,
    active_request: Option<ActiveRequestGuard>,
    access_log: AccessLogContext,
    http_route: String,
    http_method: String,
    url_path: String,
}

impl<F, E> Future for TraceResponseFuture<F>
where
    E: std::fmt::Display + 'static,
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<TraceResponseBody>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let span = this.span.clone();

        // Attempt to poll the inner future
        let result = {
            match span.in_scope(|| this.inner.poll(cx)) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(result) => result,
            }
        };
        // One it has completed we can take the span and the classifier
        let _guard = span.enter();

        let duration = this.instant.elapsed();
        let state = this.state.clone();
        let request_body_size = *this.request_body_size;
        let attributes = this.attributes;
        match result {
            Ok(mut response) => {
                let request_id_header: Result<HeaderValue, InvalidHeaderValue> =
                    (*this.request_id).try_into();
                match request_id_header {
                    Ok(header) => {
                        response.headers_mut().insert(X_REQUEST_ID, header);
                    }
                    Err(e) => {
                        error!("Failed to set request id header: {}", e);
                    }
                }
                let status_code = response.status().as_u16();
                attributes.push(KeyValue::new(
                    "http.response.status_code",
                    status_code as i64,
                ));

                super::on_response(&response, duration, &span, Some(request_body_size));
                if response.status().is_server_error() {
                    super::on_failure(&response.status(), duration, &span);
                }

                record_http_metrics(
                    &state.metrics,
                    duration,
                    request_body_size,
                    Some(status_code),
                    attributes,
                );

                let span = span.clone();
                let attributes = std::mem::take(attributes);
                let active_request = this.active_request.take();
                let access_log = this.access_log.clone();
                let http_route = this.http_route.clone();
                let http_method = this.http_method.clone();
                let url_path = this.url_path.clone();
                let request_id = *this.request_id;
                let metrics = state.metrics.clone();
                let res: Response<TraceResponseBody> = response.map(|body| TraceResponseBody {
                    inner: body,
                    request_start: *this.instant,
                    last_polled_at: *this.instant,
                    span,
                    metrics,
                    attributes,
                    active_request,
                    total_bytes: 0,
                    status_code: Some(status_code),
                    http_route,
                    http_method,
                    url_path,
                    access_log,
                    access_logged: false,
                    request_id,
                });

                Poll::Ready(Ok(res))
            }
            Err(err) => {
                super::on_failure(&err, duration, &span);
                record_http_metrics(
                    &state.metrics,
                    duration,
                    request_body_size,
                    None,
                    attributes,
                );
                // Drop guard to ensure the active request counter is decremented for failed calls.
                drop(this.active_request.take());

                super::response_body::emit_access_log(
                    &span,
                    *this.instant,
                    *this.request_id,
                    &*this.http_method,
                    &*this.http_route,
                    &*this.url_path,
                    Some(500),
                    None,
                    &*this.access_log,
                );

                Poll::Ready(Err(err))
            }
        }
    }
}

fn record_http_metrics(
    metrics: &AppMetrics,
    duration: Duration,
    body_size: u64,
    status_code: Option<u16>,
    attrs: &mut Vec<KeyValue>,
) {
    let status = status_code.unwrap_or(500);
    if !attrs
        .iter()
        .any(|attr| attr.key.as_str() == "http.response.status_code")
    {
        attrs.push(KeyValue::new("http.response.status_code", status as i64));
    }

    metrics.request_size_bytes.record(body_size, attrs);
    metrics
        .request_duration
        .record(duration.as_secs_f64(), attrs);
    metrics.request_count.add(1, attrs);
}

#[cfg(test)]
mod tests;
