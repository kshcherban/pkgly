#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{sync::Arc, time::Duration};

use opentelemetry::{KeyValue, Value};
use opentelemetry_sdk::{
    error::OTelSdkResult,
    metrics::{
        InstrumentKind, ManualReader, SdkMeterProvider, Temporality,
        data::{AggregatedMetrics, MetricData, ResourceMetrics, SumDataPoint},
        reader::MetricReader,
    },
};

use super::{ActiveRequestGuard, record_http_metrics};
use crate::app::AppMetrics;

#[derive(Clone, Debug)]
struct SharedReader(Arc<ManualReader>);

impl MetricReader for SharedReader {
    fn register_pipeline(&self, pipeline: std::sync::Weak<opentelemetry_sdk::metrics::Pipeline>) {
        self.0.register_pipeline(pipeline)
    }

    fn collect(&self, rm: &mut ResourceMetrics) -> OTelSdkResult {
        self.0.collect(rm)
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.0.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }

    fn temporality(&self, kind: InstrumentKind) -> Temporality {
        self.0.temporality(kind)
    }
}

fn setup_metrics() -> (SharedReader, AppMetrics, SdkMeterProvider) {
    let reader = SharedReader(Arc::new(ManualReader::builder().build()));
    let provider = SdkMeterProvider::builder()
        .with_reader(reader.clone())
        .build();
    let metrics = AppMetrics::with_meter_provider(&provider);
    (reader, metrics, provider)
}

fn collect(reader: &SharedReader) -> ResourceMetrics {
    let mut rm = ResourceMetrics::default();
    reader
        .collect(&mut rm)
        .expect("manual collect should succeed");
    rm
}

fn base_attrs() -> Vec<KeyValue> {
    vec![
        KeyValue::new("http.route", "/test"),
        KeyValue::new("http.request.method", "GET"),
    ]
}

fn find_sum_u64<'a>(rm: &'a ResourceMetrics, name: &str) -> Option<&'a SumDataPoint<u64>> {
    for scope in rm.scope_metrics() {
        for metric in scope.metrics() {
            if metric.name() == name {
                if let AggregatedMetrics::U64(MetricData::Sum(sum)) = metric.data() {
                    return sum.data_points().next();
                }
            }
        }
    }
    None
}

fn find_sum_i64<'a>(rm: &'a ResourceMetrics, name: &str) -> Option<&'a SumDataPoint<i64>> {
    for scope in rm.scope_metrics() {
        for metric in scope.metrics() {
            if metric.name() == name {
                if let AggregatedMetrics::I64(MetricData::Sum(sum)) = metric.data() {
                    return sum.data_points().next();
                }
            }
        }
    }
    None
}

fn attr_as_i64(attrs: &[KeyValue], key: &str) -> Option<i64> {
    attrs
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .and_then(|kv| {
            if let Value::I64(v) = kv.value {
                Some(v)
            } else {
                None
            }
        })
}

fn attr_as_str<'a>(attrs: &'a [KeyValue], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .and_then(|kv| {
            if let Value::String(ref v) = kv.value {
                Some(v.as_str())
            } else {
                None
            }
        })
}

#[test]
fn response_status_records_status_and_basic_labels() {
    let (reader, metrics, _provider) = setup_metrics();
    let mut attrs = base_attrs();

    record_http_metrics(
        &metrics,
        Duration::from_millis(12),
        128,
        Some(201),
        &mut attrs,
    );

    let rm = collect(&reader);
    let req_dp = find_sum_u64(&rm, "http.server.requests").expect("request counter should exist");
    let attrs: Vec<KeyValue> = req_dp.attributes().cloned().collect();

    assert_eq!(req_dp.value(), 1);
    assert_eq!(attr_as_i64(&attrs, "http.response.status_code"), Some(201));
    assert_eq!(attr_as_str(&attrs, "http.request.method"), Some("GET"));
}

#[test]
fn response_status_defaults_to_internal_error_when_missing() {
    let (reader, metrics, _provider) = setup_metrics();
    let mut attrs = base_attrs();

    record_http_metrics(&metrics, Duration::from_millis(7), 0, None, &mut attrs);

    let rm = collect(&reader);
    let req_dp = find_sum_u64(&rm, "http.server.requests").expect("request counter should exist");
    let attrs: Vec<KeyValue> = req_dp.attributes().cloned().collect();

    assert_eq!(req_dp.value(), 1);
    assert_eq!(attr_as_i64(&attrs, "http.response.status_code"), Some(500));
}

#[test]
fn active_requests_increments_and_decrements() {
    let (reader, metrics, _provider) = setup_metrics();
    let attrs = base_attrs();

    {
        let mut guard = ActiveRequestGuard::start(&metrics, attrs.clone());
        let rm = collect(&reader);
        let active_dp = find_sum_i64(&rm, "http.server.active_requests")
            .expect("active request counter should exist");
        let attrs: Vec<KeyValue> = active_dp.attributes().cloned().collect();

        assert_eq!(active_dp.value(), 1);
        assert_eq!(attr_as_str(&attrs, "http.route"), Some("/test"));
        guard.finish();
    }

    let rm = collect(&reader);
    let active_dp = find_sum_i64(&rm, "http.server.active_requests")
        .expect("active request counter should exist");
    assert_eq!(active_dp.value(), 0);
}

#[test]
fn request_counter_tracks_completed_requests() {
    let (reader, metrics, _provider) = setup_metrics();
    let mut attrs = base_attrs();

    record_http_metrics(
        &metrics,
        Duration::from_millis(33),
        64,
        Some(204),
        &mut attrs,
    );

    let rm = collect(&reader);
    let req_dp = find_sum_u64(&rm, "http.server.requests").expect("request counter exists");
    let attrs: Vec<KeyValue> = req_dp.attributes().cloned().collect();

    assert_eq!(req_dp.value(), 1);
    assert_eq!(attr_as_i64(&attrs, "http.response.status_code"), Some(204));
}
