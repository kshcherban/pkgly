use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Instant,
};

use futures_util::task::noop_waker_ref;
use http_body::Body as _;
use opentelemetry::KeyValue;
use tracing_subscriber::{Layer, Registry, filter::Targets, layer::SubscriberExt};

use super::response_body::TraceResponseBody;
use crate::{
    app::AppMetrics,
    utils::request_logging::{access_log::AccessLogContext, request_id::RequestId},
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
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "poisoned mutex"))?;
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

fn drive_body_to_end(mut body: Pin<&mut TraceResponseBody>) {
    let waker = noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    loop {
        match body.as_mut().poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(_))) => continue,
            Poll::Ready(None) => return,
            Poll::Ready(Some(Err(err))) => panic!("unexpected body error: {err}"),
            Poll::Pending => panic!("unexpected pending from test body"),
        }
    }
}

#[test]
fn access_log_emits_required_fields_once() {
    let writer = BufferWriter::default();
    let layer = json_layer_for_target(writer.clone(), "pkgly::access");
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("test_request");

        let access_log = AccessLogContext::default();
        let repository_id = uuid::Uuid::new_v4();
        access_log.set_repository_id(repository_id);
        access_log.set_user("alice");

        let mut body = Box::pin(TraceResponseBody {
            inner: axum::body::Body::empty(),
            request_start: Instant::now(),
            last_polled_at: Instant::now(),
            span,
            metrics: AppMetrics::default(),
            attributes: Vec::<KeyValue>::new(),
            active_request: None,
            total_bytes: 0,
            status_code: Some(200),
            http_route: "/api/test".to_string(),
            http_method: "GET".to_string(),
            url_path: "/api/test?ignored=true".to_string(),
            access_log,
            access_logged: false,
            request_id: RequestId::new_random(),
        });

        drive_body_to_end(body.as_mut());
        drop(body);
    });

    let output = read_buffer(&writer);
    assert_eq!(
        output.matches("\"message\":\"HTTP access\"").count(),
        1,
        "output was: {output}"
    );
    assert!(output.contains("\"trace_id\":\""), "output was: {output}");
    assert!(
        output.contains("\"http.request.method\":\"GET\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"http.route\":\"/api/test\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"url.path\":\"/api/test?ignored=true\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"http.response.status_code\":200"),
        "output was: {output}"
    );
    assert!(output.contains("\"duration_ms\":"), "output was: {output}");
    assert!(output.contains("\"request_id\":\""), "output was: {output}");
    assert!(
        output.contains("\"repository_id\":\""),
        "output was: {output}"
    );
    assert!(
        output.contains("\"user\":\"alice\""),
        "output was: {output}"
    );
}

#[test]
fn access_log_omits_optional_fields_when_not_present() {
    let writer = BufferWriter::default();
    let layer = json_layer_for_target(writer.clone(), "pkgly::access");
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("test_request");
        let access_log = AccessLogContext::default();

        let mut body = Box::pin(TraceResponseBody {
            inner: axum::body::Body::empty(),
            request_start: Instant::now(),
            last_polled_at: Instant::now(),
            span,
            metrics: AppMetrics::default(),
            attributes: Vec::<KeyValue>::new(),
            active_request: None,
            total_bytes: 0,
            status_code: Some(204),
            http_route: "/api/empty".to_string(),
            http_method: "GET".to_string(),
            url_path: "/api/empty".to_string(),
            access_log,
            access_logged: false,
            request_id: RequestId::new_random(),
        });

        drive_body_to_end(body.as_mut());
        drop(body);
    });

    let output = read_buffer(&writer);
    assert!(
        !output.contains("\"repository_id\""),
        "output was: {output}"
    );
    assert!(!output.contains("\"user\""), "output was: {output}");
}
