use std::{
    io,
    sync::{Arc, Mutex},
};

use tracing_subscriber::{Layer, Registry, filter::Targets, layer::SubscriberExt};

use super::parse_pom_bytes;

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

#[test]
fn parse_pom_does_not_log_entire_body() {
    let writer = BufferWriter::default();
    let targets: Targets = Targets::new().with_default(tracing::level_filters::LevelFilter::TRACE);
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(writer.clone())
        .with_ansi(false)
        .json()
        .without_time()
        .with_current_span(false)
        .with_span_list(false)
        .with_filter(targets);
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        // A "POM" that is really a giant HTML page (common upstream failure mode).
        let html = "<html><head><title>oops</title></head><body>ERR</body></html>";
        let body = format!("{html}{html}{html}{html}{html}");
        let _ = parse_pom_bytes(body.into_bytes());
    });

    let output = read_buffer(&writer);
    assert!(
        !output.contains("<html"),
        "unexpected HTML found in logs: {output}"
    );
    assert!(output.contains("\"pom.size\":"), "output was: {output}");
}
