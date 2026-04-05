#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

use crate::repository::test_helpers::test_storage;
use async_trait::async_trait;
use axum::{Router, routing::get};
use bytes::Bytes;
use nr_core::{repository::proxy_url::ProxyURL, storage::StoragePath};
use nr_storage::Storage;
use sha2::Digest;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

async fn start_upstream_server(
    release: &'static [u8],
    in_release: &'static [u8],
    packages: Bytes,
    deb_path: String,
    deb_bytes: Bytes,
    deb_counter: Arc<AtomicUsize>,
) -> anyhow::Result<(ProxyURL, JoinHandle<()>)> {
    let packages_bytes = packages.clone();
    let deb_bytes_route = deb_bytes.clone();

    let app = Router::new()
        .route("/dists/stable/Release", get(move || async move { release }))
        .route(
            "/dists/stable/InRelease",
            get(move || async move { in_release }),
        )
        .route(
            "/dists/stable/main/binary-amd64/Packages",
            get(move || {
                let packages_bytes = packages_bytes.clone();
                async move { packages_bytes }
            }),
        )
        .route(
            &format!("/{deb_path}"),
            get(move || {
                let deb_counter = deb_counter.clone();
                let deb_bytes_route = deb_bytes_route.clone();
                async move {
                    deb_counter.fetch_add(1, Ordering::SeqCst);
                    deb_bytes_route
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("upstream server error: {err}");
        }
    });
    let url = ProxyURL::try_from(format!("http://{addr}"))?;
    Ok((url, server))
}

async fn start_upstream_server_with_prefix(
    prefix: &'static str,
    release: &'static [u8],
    in_release: &'static [u8],
    packages: Bytes,
    deb_path: String,
    deb_bytes: Bytes,
    deb_counter: Arc<AtomicUsize>,
) -> anyhow::Result<(ProxyURL, JoinHandle<()>)> {
    let packages_bytes = packages.clone();
    let deb_bytes_route = deb_bytes.clone();
    let release_bytes: Bytes = Bytes::from_static(release);
    let in_release_bytes: Bytes = Bytes::from_static(in_release);

    let release_path = format!("/{prefix}/dists/stable/Release");
    let in_release_path = format!("/{prefix}/dists/stable/InRelease");
    let packages_path = format!("/{prefix}/dists/stable/main/binary-amd64/Packages");
    let deb_full_path = format!("/{prefix}/{deb_path}");

    let app = Router::new()
        .route(
            release_path.as_str(),
            get(move || {
                let release_bytes = release_bytes.clone();
                async move { release_bytes }
            }),
        )
        .route(
            in_release_path.as_str(),
            get(move || {
                let in_release_bytes = in_release_bytes.clone();
                async move { in_release_bytes }
            }),
        )
        .route(
            packages_path.as_str(),
            get(move || {
                let packages_bytes = packages_bytes.clone();
                async move { packages_bytes }
            }),
        )
        .route(
            deb_full_path.as_str(),
            get(move || {
                let deb_counter = deb_counter.clone();
                let deb_bytes_route = deb_bytes_route.clone();
                async move {
                    deb_counter.fetch_add(1, Ordering::SeqCst);
                    deb_bytes_route
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("upstream server error: {err}");
        }
    });
    let url = ProxyURL::try_from(format!("http://{addr}/{prefix}"))?;
    Ok((url, server))
}

async fn start_flat_upstream_server(
    packages: Bytes,
    packages_gz: Bytes,
    deb_path: String,
    deb_bytes: Bytes,
    deb_counter: Arc<AtomicUsize>,
    packages_path: &'static str,
    packages_gz_path: &'static str,
) -> anyhow::Result<(ProxyURL, JoinHandle<()>)> {
    let packages_bytes = packages.clone();
    let packages_gz_bytes = packages_gz.clone();
    let deb_bytes_route = deb_bytes.clone();

    let app = Router::new()
        .route(
            packages_path,
            get(move || {
                let packages_bytes = packages_bytes.clone();
                async move { packages_bytes }
            }),
        )
        .route(
            packages_gz_path,
            get(move || {
                let packages_gz_bytes = packages_gz_bytes.clone();
                async move { packages_gz_bytes }
            }),
        )
        .route(
            &format!("/{deb_path}"),
            get(move || {
                let deb_counter = deb_counter.clone();
                let deb_bytes_route = deb_bytes_route.clone();
                async move {
                    deb_counter.fetch_add(1, Ordering::SeqCst);
                    deb_bytes_route
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("upstream server error: {err}");
        }
    });
    let url = ProxyURL::try_from(format!("http://{addr}"))?;
    Ok((url, server))
}

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

    let mut deb_bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut deb_bytes);
        let mut builder = ar::Builder::new(cursor);
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
            .append(&ar::Header::new(b"data.tar.gz".to_vec(), 0), &[][..])
            .expect("append data");
    }
    deb_bytes
}

#[derive(Clone, Default)]
struct RecordingIndexer {
    recorded: Arc<tokio::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl super::super::proxy_indexing::DebProxyIndexing for RecordingIndexer {
    async fn record_cached_deb(
        &self,
        record: super::super::proxy_indexing::DebProxyPackageRecord,
    ) -> Result<(), super::super::proxy_indexing::DebProxyIndexingError> {
        self.recorded.lock().await.push(record.package_key);
        Ok(())
    }
}

#[tokio::test]
async fn refresh_downloads_packages_and_creates_by_hash_aliases() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let deb_bytes = Bytes::from(build_minimal_deb("hello", "1.0.0", "amd64"));
    let deb_sha256 = format!("{:x}", sha2::Sha256::digest(&deb_bytes));

    let deb_path = "pool/main/h/hello/hello_1.0.0_amd64.deb".to_string();
    let packages = format!(
        "Package: hello\nVersion: 1.0.0\nArchitecture: amd64\nFilename: {deb_path}\nSize: {}\nSHA256: {deb_sha256}\nDescription: test\n\n",
        deb_bytes.len()
    );
    let packages_bytes = Bytes::from(packages.clone());
    let packages_hash = format!("{:x}", sha2::Sha256::digest(packages_bytes.clone()));

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream, _server) = start_upstream_server(
        b"Release bytes",
        b"InRelease bytes",
        packages_bytes.clone(),
        deb_path.clone(),
        deb_bytes.clone(),
        counter.clone(),
    )
    .await
    .expect("start upstream");

    let config = super::super::configs::DebProxyConfig {
        upstream_url: upstream,
        layout: super::super::configs::DebProxyLayout::Dists(
            super::super::configs::DebProxyDistsLayout {
                distributions: vec!["stable".into()],
                components: vec!["main".into()],
                architectures: vec!["amd64".into()],
            },
        ),
        refresh: None,
    };

    let indexer = RecordingIndexer::default();
    let client = reqwest::Client::new();
    let summary = refresh_deb_proxy_offline_mirror(&client, &storage, repo_id, &config, &indexer)
        .await
        .expect("refresh ok");

    assert_eq!(summary.downloaded_packages, 1);
    assert!(summary.downloaded_files >= 3); // Release, InRelease, Packages, + deb
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    assert!(
        storage
            .get_file_information(repo_id, &StoragePath::from("dists/stable/Release"))
            .await
            .expect("meta")
            .is_some()
    );
    assert!(
        storage
            .get_file_information(repo_id, &StoragePath::from("dists/stable/InRelease"))
            .await
            .expect("meta")
            .is_some()
    );
    assert!(
        storage
            .get_file_information(
                repo_id,
                &StoragePath::from("dists/stable/main/binary-amd64/Packages")
            )
            .await
            .expect("meta")
            .is_some()
    );
    assert!(
        storage
            .get_file_information(
                repo_id,
                &StoragePath::from(format!(
                    "dists/stable/main/binary-amd64/by-hash/SHA256/{packages_hash}"
                ))
            )
            .await
            .expect("meta")
            .is_some()
    );
    assert!(
        storage
            .get_file_information(repo_id, &StoragePath::from(deb_path.clone()))
            .await
            .expect("meta")
            .is_some()
    );

    // Second run should not re-download already cached deb package.
    let _ = refresh_deb_proxy_offline_mirror(&client, &storage, repo_id, &config, &indexer)
        .await
        .expect("refresh ok");
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // Sanity: the saved deb bytes match.
    let file = storage
        .open_file(repo_id, &StoragePath::from(deb_path))
        .await
        .expect("open")
        .expect("exists");
    let nr_storage::StorageFile::File { meta, content } = file else {
        panic!("expected file");
    };
    let nr_storage::FileFileType { file_size, .. } = meta.file_type();
    let len: usize = (*file_size).try_into().unwrap();
    let read_back = content.read_to_vec(len).await.expect("read");
    assert_eq!(read_back, deb_bytes);
}

#[tokio::test]
async fn refresh_supports_flat_layout_root_packages() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let deb_bytes = Bytes::from(build_minimal_deb("hello", "1.0.0", "amd64"));
    let deb_sha256 = format!("{:x}", sha2::Sha256::digest(&deb_bytes));

    let deb_path = "hello_1.0.0_amd64.deb".to_string();
    let packages = format!(
        "Package: hello\nVersion: 1.0.0\nArchitecture: amd64\nFilename: {deb_path}\nSize: {}\nSHA256: {deb_sha256}\nDescription: test\n\n",
        deb_bytes.len()
    );
    let packages_bytes = Bytes::from(packages);
    let packages_gz = {
        use std::io::Write;
        let mut gz = Vec::new();
        let mut encoder = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
        encoder
            .write_all(&packages_bytes)
            .expect("write packages gz");
        encoder.finish().expect("finish gz");
        Bytes::from(gz)
    };

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream, _server) = start_flat_upstream_server(
        packages_bytes.clone(),
        packages_gz.clone(),
        deb_path.clone(),
        deb_bytes.clone(),
        counter.clone(),
        "/Packages",
        "/Packages.gz",
    )
    .await
    .expect("start upstream");

    let config = super::super::configs::DebProxyConfig {
        upstream_url: upstream,
        layout: super::super::configs::DebProxyLayout::Flat(
            super::super::configs::DebProxyFlatLayout {
                distribution: "./".into(),
                architectures: vec![],
            },
        ),
        refresh: None,
    };

    let indexer = RecordingIndexer::default();
    let client = reqwest::Client::new();
    let summary = refresh_deb_proxy_offline_mirror(&client, &storage, repo_id, &config, &indexer)
        .await
        .expect("refresh ok");

    assert_eq!(summary.downloaded_packages, 1);
    assert!(summary.downloaded_files >= 2); // Packages + deb
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    assert!(
        storage
            .get_file_information(repo_id, &StoragePath::from("Packages"))
            .await
            .expect("meta")
            .is_some()
    );
    assert!(
        storage
            .get_file_information(repo_id, &StoragePath::from("Packages.gz"))
            .await
            .expect("meta")
            .is_some()
    );
    assert!(
        storage
            .get_file_information(repo_id, &StoragePath::from(deb_path.clone()))
            .await
            .expect("meta")
            .is_some()
    );
}

#[tokio::test]
async fn refresh_supports_upstream_url_with_path_prefix() {
    let storage = test_storage().await;
    let repo_id = Uuid::new_v4();
    let deb_bytes = Bytes::from(build_minimal_deb("hello", "1.0.0", "amd64"));
    let deb_sha256 = format!("{:x}", sha2::Sha256::digest(&deb_bytes));

    let deb_path = "pool/main/h/hello/hello_1.0.0_amd64.deb".to_string();
    let packages = format!(
        "Package: hello\nVersion: 1.0.0\nArchitecture: amd64\nFilename: {deb_path}\nSize: {}\nSHA256: {deb_sha256}\nDescription: test\n\n",
        deb_bytes.len()
    );
    let packages_bytes = Bytes::from(packages);

    let counter = Arc::new(AtomicUsize::new(0));
    let (upstream, _server) = start_upstream_server_with_prefix(
        "packages",
        b"Release bytes",
        b"InRelease bytes",
        packages_bytes.clone(),
        deb_path.clone(),
        deb_bytes.clone(),
        counter.clone(),
    )
    .await
    .expect("start upstream");

    let config = super::super::configs::DebProxyConfig {
        upstream_url: upstream,
        layout: super::super::configs::DebProxyLayout::Dists(
            super::super::configs::DebProxyDistsLayout {
                distributions: vec!["stable".into()],
                components: vec!["main".into()],
                architectures: vec!["amd64".into()],
            },
        ),
        refresh: None,
    };

    let indexer = RecordingIndexer::default();
    let client = reqwest::Client::new();
    let summary = refresh_deb_proxy_offline_mirror(&client, &storage, repo_id, &config, &indexer)
        .await
        .expect("refresh ok");

    assert_eq!(summary.downloaded_packages, 1);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert!(
        storage
            .get_file_information(repo_id, &StoragePath::from("dists/stable/Release"))
            .await
            .expect("meta")
            .is_some()
    );
}
