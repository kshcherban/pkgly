use std::{
    io,
    sync::{Arc, Mutex},
};

use tracing::info;
use tracing_subscriber::{Registry, filter::Targets, layer::SubscriberExt};

use super::{ConsoleLogFormat, StandardLoggerFmtRules};

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

fn targets_all() -> Targets {
    Targets::new().with_default(tracing::level_filters::LevelFilter::TRACE)
}

fn read_buffer(writer: &BufferWriter) -> String {
    let locked = writer.0.lock().expect("test mutex should not be poisoned");
    String::from_utf8_lossy(&locked).to_string()
}

#[test]
fn include_time_false_removes_timestamp_prefix() {
    let writer = BufferWriter::default();
    let rules = StandardLoggerFmtRules {
        include_time: false,
        include_level: true,
        include_target: false,
        ansi_color: false,
        ..StandardLoggerFmtRules::default()
    };

    let layer = rules.fmt_layer_for_registry_with_writer(
        ConsoleLogFormat::Compact,
        targets_all(),
        writer.clone(),
    );
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        info!("hello");
    });

    let output = read_buffer(&writer);
    let first_token = output.split_whitespace().next().unwrap_or_default();
    assert_eq!(first_token, "INFO");
}

#[test]
fn include_time_true_keeps_non_level_prefix() {
    let writer = BufferWriter::default();
    let rules = StandardLoggerFmtRules {
        include_time: true,
        include_level: true,
        include_target: false,
        ansi_color: false,
        ..StandardLoggerFmtRules::default()
    };

    let layer = rules.fmt_layer_for_registry_with_writer(
        ConsoleLogFormat::Compact,
        targets_all(),
        writer.clone(),
    );
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        info!("hello");
    });

    let output = read_buffer(&writer);
    let first_token = output.split_whitespace().next().unwrap_or_default();
    assert_ne!(first_token, "INFO");
}

#[test]
fn include_span_context_false_hides_span_name_and_fields() {
    let writer = BufferWriter::default();
    let rules = StandardLoggerFmtRules {
        include_span_context: false,
        include_time: false,
        include_level: true,
        include_target: false,
        ansi_color: false,
        ..StandardLoggerFmtRules::default()
    };

    let layer = rules.fmt_layer_for_registry_with_writer(
        ConsoleLogFormat::Compact,
        targets_all(),
        writer.clone(),
    );
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("test_span", big = "BIG_VALUE");
        let _guard = span.enter();
        info!("hello");
    });

    let output = read_buffer(&writer);
    assert!(!output.contains("test_span"), "output was: {output}");
    assert!(!output.contains("BIG_VALUE"), "output was: {output}");
}

#[test]
fn compact_format_never_prints_span_fields() {
    let writer = BufferWriter::default();
    let rules = StandardLoggerFmtRules {
        include_span_context: true,
        include_time: false,
        include_level: true,
        include_target: false,
        ansi_color: false,
        ..StandardLoggerFmtRules::default()
    };

    let layer = rules.fmt_layer_for_registry_with_writer(
        ConsoleLogFormat::Compact,
        targets_all(),
        writer.clone(),
    );
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!(
            "HTTP request",
            site = "Pkgly {",
            auth = "Authentication(",
            password = "supersecret"
        );
        let _guard = span.enter();
        info!("hello");
    });

    let output = read_buffer(&writer);
    assert!(output.contains("HTTP request"), "output was: {output}");
    assert!(!output.contains("Pkgly {"), "output was: {output}");
    assert!(!output.contains("Authentication("), "output was: {output}");
    assert!(!output.contains("supersecret"), "output was: {output}");
}

#[test]
fn json_format_emits_json_object() {
    let writer = BufferWriter::default();
    let rules = StandardLoggerFmtRules {
        include_span_context: false,
        include_time: false,
        include_level: true,
        include_target: false,
        ansi_color: false,
        ..StandardLoggerFmtRules::default()
    };

    let layer = rules.fmt_layer_for_registry_with_writer(
        ConsoleLogFormat::Json,
        targets_all(),
        writer.clone(),
    );
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        info!("hello");
    });

    let output = read_buffer(&writer);
    let trimmed = output.trim_start();
    assert!(trimmed.starts_with('{'), "output was: {output}");
    assert!(output.contains("\"message\""), "output was: {output}");
}

#[test]
fn ansi_color_true_colorizes_level_in_compact_format() {
    let writer = BufferWriter::default();
    let rules = StandardLoggerFmtRules {
        include_time: false,
        include_level: true,
        include_target: false,
        ansi_color: true,
        ..StandardLoggerFmtRules::default()
    };

    let layer = rules.fmt_layer_for_registry_with_writer(
        ConsoleLogFormat::Compact,
        targets_all(),
        writer.clone(),
    );
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        info!("hello");
    });

    let output = read_buffer(&writer);
    assert!(
        output.contains("\u{1b}[32mINFO\u{1b}[0m"),
        "output was: {output}"
    );
}

#[test]
fn ansi_color_true_in_compact_does_not_style_fields() {
    let writer = BufferWriter::default();
    let rules = StandardLoggerFmtRules {
        include_time: false,
        include_level: true,
        include_target: false,
        ansi_color: true,
        ..StandardLoggerFmtRules::default()
    };

    let layer = rules.fmt_layer_for_registry_with_writer(
        ConsoleLogFormat::Compact,
        targets_all(),
        writer.clone(),
    );
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        info!(example_field = "value", "hello");
    });

    let output = read_buffer(&writer);
    assert!(
        output.contains("\u{1b}[32mINFO\u{1b}[0m"),
        "output was: {output}"
    );
    let without_level = output.replace("\u{1b}[32m", "").replace("\u{1b}[0m", "");
    assert!(
        !without_level.contains("\u{1b}["),
        "unexpected ANSI styling in output: {output}"
    );
}
