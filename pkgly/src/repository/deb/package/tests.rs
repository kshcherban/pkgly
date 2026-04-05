#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write;
use tar::Builder;
use xz2::write::XzEncoder;

#[test]
fn parsing_minimal_deb_extracts_control_fields() {
    let deb_bytes = build_minimal_deb();
    let parsed = parse_deb_package(Bytes::from(deb_bytes)).expect("parse deb");
    assert_eq!(parsed.control.get("Package"), Some("sample"));
}

#[test]
fn parsing_xz_compressed_control_is_supported() {
    let deb_bytes = build_deb_with_compression(ControlCompression::Xz);
    let parsed = parse_deb_package(Bytes::from(deb_bytes)).expect("parse deb");
    assert_eq!(parsed.control.get("Version"), Some("1.0"));
}

fn build_minimal_deb() -> Vec<u8> {
    build_deb_with_compression(ControlCompression::Gzip)
}

enum ControlCompression {
    Gzip,
    Xz,
}

fn build_deb_with_compression(kind: ControlCompression) -> Vec<u8> {
    let mut control_tar = Vec::new();
    {
        let cursor = Cursor::new(&mut control_tar);
        let mut builder = Builder::new(cursor);
        let mut header = tar::Header::new_gnu();
        let contents =
            b"Package: sample\nVersion: 1.0\nArchitecture: amd64\nDescription: summary\n";
        header.set_size(contents.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, "control", &contents[..])
            .expect("append control");
        builder.finish().expect("finish tar");
    }

    let (encoded, name) = match kind {
        ControlCompression::Gzip => {
            let mut encoded = Vec::new();
            {
                let mut encoder = GzEncoder::new(&mut encoded, Compression::default());
                encoder.write_all(&control_tar).expect("write gz");
                encoder.finish().expect("finish gz");
            }
            (encoded, "control.tar.gz")
        }
        ControlCompression::Xz => {
            let mut encoded = Vec::new();
            {
                let mut encoder = XzEncoder::new(&mut encoded, 6);
                encoder.write_all(&control_tar).expect("write xz");
                encoder.finish().expect("finish xz");
            }
            (encoded, "control.tar.xz")
        }
    };

    let mut deb_bytes = Vec::new();
    {
        let cursor = Cursor::new(&mut deb_bytes);
        let mut builder = ar::Builder::new(cursor);
        builder
            .append(
                &ar::Header::new(b"debian-binary".to_vec(), 4),
                &b"2.0\n"[..],
            )
            .expect("append debian-binary");
        builder
            .append(
                &ar::Header::new(name.as_bytes().to_vec(), encoded.len() as u64),
                &encoded[..],
            )
            .expect("append control");
        builder
            .append(&ar::Header::new(b"data.tar.gz".to_vec(), 0), &[][..])
            .expect("append data");
    }
    deb_bytes
}
