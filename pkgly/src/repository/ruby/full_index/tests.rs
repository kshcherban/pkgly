#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::{Cursor, Read};

use flate2::read::{GzDecoder, ZlibDecoder};
use thurgood::rc::RbAny;

use super::*;

fn read_marshal_bytes_from_gzip(gzip: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(gzip);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).unwrap();
    out
}

fn read_marshal_bytes_from_zlib(zlib: &[u8]) -> Vec<u8> {
    let mut decoder = ZlibDecoder::new(zlib);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).unwrap();
    out
}

fn gem_version_string(value: &RbAny) -> Option<String> {
    let obj = value.as_object()?;
    if obj.name.as_str()? != "Gem::Version" {
        return None;
    }
    obj.get("@version")?.as_string().cloned()
}

#[test]
fn build_specs_gz_encodes_rubygems_tuples() {
    let bytes = build_specs_gz(&[SpecsIndexEntry {
        name: "demo".to_string(),
        version: "1.2.3".to_string(),
        platform: "ruby".to_string(),
    }])
    .unwrap();

    let marshal = read_marshal_bytes_from_gzip(&bytes);
    let any = thurgood::rc::from_reader(Cursor::new(marshal)).unwrap();
    let array = any.as_array().unwrap();
    assert_eq!(array.len(), 1);

    let tuple = array[0].as_array().unwrap();
    assert_eq!(tuple.len(), 3);
    assert_eq!(tuple[0].as_string().unwrap(), "demo");
    assert_eq!(gem_version_string(&tuple[1]).as_deref(), Some("1.2.3"));
    assert_eq!(tuple[2].as_string().unwrap(), "ruby");
}

#[test]
fn build_gemspec_rz_encodes_minimal_spec() {
    let bytes = build_gemspec_rz(&GemSpecEntry {
        name: "demo".to_string(),
        version: "1.2.3".to_string(),
        platform: "ruby".to_string(),
        dependencies: vec![RubyDependencyMetadata {
            name: "rack".to_string(),
            requirements: vec!["~> 2.0".to_string()],
        }],
        required_ruby: Some(">= 2.7.0".to_string()),
        required_rubygems: None,
    })
    .unwrap();

    let marshal = read_marshal_bytes_from_zlib(&bytes);
    let any = thurgood::rc::from_reader(Cursor::new(marshal)).unwrap();
    let obj = any.as_object().unwrap();
    assert_eq!(obj.name.as_str().unwrap(), "Gem::Specification");
    assert_eq!(obj.get("@name").unwrap().as_string().unwrap(), "demo");
    assert_eq!(
        gem_version_string(obj.get("@version").unwrap()).as_deref(),
        Some("1.2.3")
    );
    assert_eq!(obj.get("@platform").unwrap().as_string().unwrap(), "ruby");

    let deps = obj.get("@dependencies").unwrap().as_array().unwrap();
    assert_eq!(deps.len(), 1);
    let dep_obj = deps[0].as_object().unwrap();
    assert_eq!(dep_obj.name.as_str().unwrap(), "Gem::Dependency");
    assert_eq!(dep_obj.get("@name").unwrap().as_string().unwrap(), "rack");
    assert_eq!(
        dep_obj
            .get("@type")
            .unwrap()
            .as_symbol()
            .unwrap()
            .as_str()
            .unwrap(),
        "runtime"
    );

    let requirement = dep_obj.get("@requirement").unwrap().as_object().unwrap();
    assert_eq!(requirement.name.as_str().unwrap(), "Gem::Requirement");

    let pairs = requirement
        .get("@requirements")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(pairs.len(), 1);
    let pair = pairs[0].as_array().unwrap();
    assert_eq!(pair[0].as_string().unwrap(), "~>");
}
