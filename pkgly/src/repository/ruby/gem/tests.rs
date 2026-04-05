#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use super::*;

use flate2::{Compression, write::GzEncoder};
use std::io::{Cursor, Write};
use tar::Builder;
use tempfile::NamedTempFile;
use thurgood::rc::{RbAny, RbObject, RbRef, RbSymbol, to_writer};

fn marshal_bytes(value: &RbAny) -> Vec<u8> {
    let mut out = Vec::new();
    to_writer(&mut out, value).unwrap();
    out
}

fn gem_version(version: &str) -> RbAny {
    RbObject::new_from_slice("Gem::Version", &[("@version", version.into())])
        .into_object()
        .into()
}

fn gem_requirement(pairs: &[(&str, &str)]) -> RbAny {
    let mut reqs = Vec::new();
    for (op, version) in pairs {
        reqs.push(RbAny::from(vec![
            RbAny::symbol_from(op),
            gem_version(version),
        ]));
    }
    RbObject::new_from_slice("Gem::Requirement", &[("@requirements", RbAny::from(reqs))])
        .into_object()
        .into()
}

fn dependency(name: &str, kind: &str, req: RbAny) -> RbAny {
    let type_symbol: RbSymbol = kind.into();
    RbRef::new_object(
        "Gem::Dependency",
        &vec![
            ("@name".into(), name.into()),
            ("@requirement".into(), req),
            ("@type".into(), type_symbol.as_any()),
        ],
    )
    .into_any()
}

fn gem_platform(cpu: &str, os: &str, version: Option<&str>) -> RbAny {
    let version_any = match version {
        Some(value) => value.into(),
        None => RbAny::Nil,
    };
    RbObject::new_from_slice(
        "Gem::Platform",
        &[
            ("@cpu", cpu.into()),
            ("@os", os.into()),
            ("@version", version_any),
        ],
    )
    .into_object()
    .into()
}

fn gem_file_bytes(gemspec: &RbAny) -> Vec<u8> {
    let metadata = marshal_bytes(gemspec);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&metadata).unwrap();
    let metadata_gz = encoder.finish().unwrap();

    let mut out = Vec::new();
    {
        let mut builder = Builder::new(&mut out);
        append_file(&mut builder, "metadata.gz", &metadata_gz);
        append_file(&mut builder, "data.tar.gz", &[]);
        builder.finish().unwrap();
    }
    out
}

fn yaml_gem_file_bytes(yaml: &str) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(yaml.as_bytes()).unwrap();
    let metadata_gz = encoder.finish().unwrap();

    let mut out = Vec::new();
    {
        let mut builder = Builder::new(&mut out);
        append_file(&mut builder, "metadata.gz", &metadata_gz);
        append_file(&mut builder, "data.tar.gz", &[]);
        builder.finish().unwrap();
    }
    out
}

fn append_file<W>(builder: &mut Builder<W>, path: &str, contents: &[u8])
where
    W: std::io::Write,
{
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, &mut Cursor::new(contents))
        .unwrap();
}

#[test]
fn parse_gemspec_extracts_expected_fields() {
    let runtime_req = gem_requirement(&[("~>", "1.0"), (">=", "1.2.3")]);
    let dev_req = gem_requirement(&[(">=", "0")]);
    let spec = RbObject::new_from_slice(
        "Gem::Specification",
        &[
            ("@name", "demo".into()),
            ("@version", gem_version("1.2.3")),
            ("@platform", "ruby".into()),
            (
                "@dependencies",
                RbAny::from(vec![
                    dependency("rack", "runtime", runtime_req),
                    dependency("rspec", "development", dev_req),
                ]),
            ),
            (
                "@required_ruby_version",
                gem_requirement(&[(">=", "2.7.0")]),
            ),
            (
                "@required_rubygems_version",
                gem_requirement(&[(">=", "3.0.0")]),
            ),
        ],
    )
    .into_object()
    .into();

    let bytes = gem_file_bytes(&spec);
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&bytes).unwrap();

    let parsed = parse_gemspec_from_gem_path(temp.path()).unwrap();
    assert_eq!(parsed.name, "demo");
    assert_eq!(parsed.version, "1.2.3");
    assert_eq!(parsed.platform, None);
    assert_eq!(parsed.required_ruby.as_deref(), Some(">= 2.7.0"));
    assert_eq!(parsed.required_rubygems.as_deref(), Some(">= 3.0.0"));
    assert_eq!(parsed.dependencies.len(), 1);
    assert_eq!(parsed.dependencies[0].name, "rack");
    assert_eq!(
        parsed.dependencies[0].requirements,
        vec!["~> 1.0".to_string(), ">= 1.2.3".to_string()]
    );
}

#[test]
fn parse_gemspec_preserves_non_ruby_platform() {
    let spec = RbObject::new_from_slice(
        "Gem::Specification",
        &[
            ("@name", "native".into()),
            ("@version", gem_version("0.1.0")),
            ("@platform", "x86_64-linux".into()),
        ],
    )
    .into_object()
    .into();

    let bytes = gem_file_bytes(&spec);
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&bytes).unwrap();

    let parsed = parse_gemspec_from_gem_path(temp.path()).unwrap();
    assert_eq!(parsed.platform.as_deref(), Some("x86_64-linux"));
}

#[test]
fn parse_gemspec_formats_gem_platform_object() {
    let spec = RbObject::new_from_slice(
        "Gem::Specification",
        &[
            ("@name", "native".into()),
            ("@version", gem_version("0.1.0")),
            ("@platform", gem_platform("x86_64", "linux", None)),
        ],
    )
    .into_object()
    .into();

    let bytes = gem_file_bytes(&spec);
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&bytes).unwrap();

    let parsed = parse_gemspec_from_gem_path(temp.path()).unwrap();
    assert_eq!(parsed.platform.as_deref(), Some("x86_64-linux"));
}

#[test]
fn parse_gemspec_supports_yaml_metadata() {
    let yaml = r#"--- !ruby/object:Gem::Specification
name: demo
version: !ruby/object:Gem::Version
  version: 1.2.3
platform: ruby
dependencies:
- !ruby/object:Gem::Dependency
  name: rack
  requirement: !ruby/object:Gem::Requirement
    requirements:
    - - "~>"
      - !ruby/object:Gem::Version
        version: '1.0'
  type: :runtime
- !ruby/object:Gem::Dependency
  name: rspec
  requirement: !ruby/object:Gem::Requirement
    requirements:
    - - ">="
      - !ruby/object:Gem::Version
        version: '3.0'
  type: :development
required_ruby_version: !ruby/object:Gem::Requirement
  requirements:
  - - ">="
    - !ruby/object:Gem::Version
      version: '2.7.0'
required_rubygems_version: !ruby/object:Gem::Requirement
  requirements:
  - - ">="
    - !ruby/object:Gem::Version
      version: '3.0.0'
"#;

    let bytes = yaml_gem_file_bytes(yaml);
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&bytes).unwrap();

    let parsed = parse_gemspec_from_gem_path(temp.path()).unwrap();
    assert_eq!(parsed.name, "demo");
    assert_eq!(parsed.version, "1.2.3");
    assert_eq!(parsed.platform, None);
    assert_eq!(parsed.required_ruby.as_deref(), Some(">= 2.7.0"));
    assert_eq!(parsed.required_rubygems.as_deref(), Some(">= 3.0.0"));
    assert_eq!(parsed.dependencies.len(), 1);
    assert_eq!(parsed.dependencies[0].name, "rack");
    assert_eq!(
        parsed.dependencies[0].requirements,
        vec!["~> 1.0".to_string()]
    );
}
