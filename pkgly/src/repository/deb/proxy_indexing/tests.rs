#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

use crate::repository::deb::package::parse_deb_package;
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::Mutex;

fn build_minimal_deb(package: &str, version: &str, arch: &str) -> Vec<u8> {
    use std::io::Write;

    let control = format!(
        "Package: {package}\nVersion: {version}\nArchitecture: {arch}\nDescription: test package\n"
    );

    let mut control_tar = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut control_tar);
        let mut header = tar::Header::new_gnu();
        header.set_size(control.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "control", control.as_bytes())
            .expect("append control");
        builder.finish().expect("finish tar");
    }

    let mut control_gz = Vec::new();
    {
        let mut encoder =
            flate2::write::GzEncoder::new(&mut control_gz, flate2::Compression::fast());
        encoder.write_all(&control_tar).expect("write tar");
        encoder.finish().expect("finish gz");
    }

    let mut data_tar = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut data_tar);
        builder.finish().expect("finish data tar");
    }

    let mut data_gz = Vec::new();
    {
        let mut encoder = flate2::write::GzEncoder::new(&mut data_gz, flate2::Compression::fast());
        encoder.write_all(&data_tar).expect("write data tar");
        encoder.finish().expect("finish data gz");
    }

    let mut ar_bytes = Vec::new();
    {
        let mut builder = ar::Builder::new(std::io::Cursor::new(&mut ar_bytes));
        builder
            .append(
                &ar::Header::new(b"debian-binary".to_vec(), 4),
                &b"2.0\n"[..],
            )
            .expect("append debian-binary");
        builder
            .append(
                &ar::Header::new(b"control.tar.gz".to_vec(), control_gz.len() as u64),
                &control_gz[..],
            )
            .expect("append control");
        builder
            .append(
                &ar::Header::new(b"data.tar.gz".to_vec(), data_gz.len() as u64),
                &data_gz[..],
            )
            .expect("append data");
    }
    ar_bytes
}

#[test]
fn deb_proxy_record_from_bytes_uses_package_and_arch_in_key() {
    let bytes = Bytes::from(build_minimal_deb("hello", "1.0.0", "amd64"));
    let parsed = parse_deb_package(bytes.clone()).expect("parse deb");
    assert_eq!(parsed.control.get("Package"), Some("hello"));
    assert_eq!(parsed.control.get("Version"), Some("1.0.0"));
    assert_eq!(parsed.control.get("Architecture"), Some("amd64"));

    let path = StoragePath::from("pool/main/h/hello/hello_1.0.0_amd64.deb");
    let record = deb_proxy_record_from_deb_bytes(&path, bytes).expect("record builds");
    let record = record.expect("record present");
    assert_eq!(record.package_name, "hello");
    assert_eq!(record.package_key, "hello:amd64");
    assert_eq!(record.version, "1.0.0");
    assert_eq!(record.metadata.filename, path.to_string());
    assert_eq!(record.metadata.architecture, "amd64");
}

#[tokio::test]
async fn record_deb_proxy_cache_hit_invokes_indexer_for_deb() {
    let bytes = Bytes::from(build_minimal_deb("sample", "2.0.0", "all"));
    let path = StoragePath::from("pool/main/s/sample/sample_2.0.0_all.deb");
    let indexer = RecordingIndexer::default();

    record_deb_proxy_cache_hit(&indexer, &path, bytes, None)
        .await
        .expect("indexing ok");

    let recorded = indexer.recorded().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].package_key, "sample:all");
    assert_eq!(recorded[0].version, "2.0.0");
    assert_eq!(recorded[0].metadata.filename, path.to_string());
}

#[derive(Clone, Default)]
struct RecordingIndexer {
    recorded: Arc<Mutex<Vec<DebProxyPackageRecord>>>,
}

impl RecordingIndexer {
    async fn recorded(&self) -> Vec<DebProxyPackageRecord> {
        self.recorded.lock().await.clone()
    }
}

#[async_trait]
impl DebProxyIndexing for RecordingIndexer {
    async fn record_cached_deb(
        &self,
        record: DebProxyPackageRecord,
    ) -> Result<(), DebProxyIndexingError> {
        self.recorded.lock().await.push(record);
        Ok(())
    }
}
