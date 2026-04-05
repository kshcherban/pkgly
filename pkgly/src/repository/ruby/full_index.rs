use flate2::{
    Compression,
    write::{GzEncoder, ZlibEncoder},
};
use nr_core::repository::project::RubyDependencyMetadata;
use thurgood::rc::{RbAny, RbObject, RbRef, RbSymbol, to_writer};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecsIndexEntry {
    pub name: String,
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GemSpecEntry {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub dependencies: Vec<RubyDependencyMetadata>,
    pub required_ruby: Option<String>,
    pub required_rubygems: Option<String>,
}

pub fn build_specs_gz(entries: &[SpecsIndexEntry]) -> Result<Vec<u8>, String> {
    let mut tuples = Vec::with_capacity(entries.len());
    for entry in entries {
        tuples.push(RbAny::from(vec![
            entry.name.as_str().into(),
            gem_version(&entry.version),
            entry.platform.as_str().into(),
        ]));
    }

    let marshal = marshal_rbany(&RbAny::from(tuples))?;
    gzip_bytes(&marshal)
}

pub fn build_empty_specs_gz() -> Result<Vec<u8>, String> {
    build_specs_gz(&[])
}

pub fn build_gemspec_rz(spec: &GemSpecEntry) -> Result<Vec<u8>, String> {
    let required_ruby = build_requirement(spec.required_ruby.as_deref());
    let required_rubygems = build_requirement(spec.required_rubygems.as_deref());

    let mut deps = Vec::with_capacity(spec.dependencies.len());
    for dep in &spec.dependencies {
        deps.push(build_dependency(dep));
    }

    let gemspec = RbObject::new_from_slice(
        "Gem::Specification",
        &[
            ("@name", spec.name.as_str().into()),
            ("@version", gem_version(&spec.version)),
            ("@platform", spec.platform.as_str().into()),
            ("@new_platform", spec.platform.as_str().into()),
            ("@original_platform", spec.platform.as_str().into()),
            ("@dependencies", RbAny::from(deps)),
            ("@required_ruby_version", required_ruby),
            ("@required_rubygems_version", required_rubygems),
            ("@specification_version", RbAny::Int(4)),
        ],
    )
    .into_object()
    .into();

    let marshal = marshal_rbany(&gemspec)?;
    zlib_bytes(&marshal)
}

fn marshal_rbany(value: &RbAny) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    to_writer(&mut out, value)
        .map_err(|err| format!("Failed to serialize Ruby marshal data: {err}"))?;
    Ok(out)
}

fn gzip_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, bytes)
        .map_err(|err| format!("Failed to gzip Ruby index payload: {err}"))?;
    encoder
        .finish()
        .map_err(|err| format!("Failed to finalize gzip Ruby index payload: {err}"))
}

fn zlib_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, bytes)
        .map_err(|err| format!("Failed to compress Ruby gemspec payload: {err}"))?;
    encoder
        .finish()
        .map_err(|err| format!("Failed to finalize Ruby gemspec payload: {err}"))
}

fn gem_version(version: &str) -> RbAny {
    RbObject::new_from_slice("Gem::Version", &[("@version", version.into())])
        .into_object()
        .into()
}

fn build_requirement(raw: Option<&str>) -> RbAny {
    let constraints = parse_requirement_constraints(raw);
    let mut out = Vec::with_capacity(constraints.len());
    for (op, version) in constraints {
        out.push(RbAny::from(vec![op.as_str().into(), gem_version(&version)]));
    }
    RbObject::new_from_slice("Gem::Requirement", &[("@requirements", RbAny::from(out))])
        .into_object()
        .into()
}

fn parse_requirement_constraints(raw: Option<&str>) -> Vec<(String, String)> {
    let Some(raw) = raw else {
        return vec![(">=".to_string(), "0".to_string())];
    };

    let mut out = Vec::new();
    for part in raw.split('&') {
        if let Some(parsed) = parse_single_constraint(part) {
            out.push(parsed);
        }
    }

    if out.is_empty() {
        vec![(">=".to_string(), "0".to_string())]
    } else {
        out
    }
}

fn parse_single_constraint(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        [single] => {
            if single
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_digit())
            {
                Some(("=".to_string(), (*single).to_string()))
            } else {
                None
            }
        }
        [op, version, ..] => Some(((*op).to_string(), (*version).to_string())),
        _ => None,
    }
}

fn build_dependency(dep: &RubyDependencyMetadata) -> RbAny {
    let mut constraints = Vec::new();
    for requirement in &dep.requirements {
        if let Some(parsed) = parse_single_constraint(requirement) {
            constraints.push(parsed);
        }
    }

    let raw = constraints
        .iter()
        .map(|(op, version)| format!("{op} {version}"))
        .collect::<Vec<String>>()
        .join("&");
    let requirement = build_requirement(Some(&raw));
    let type_symbol: RbSymbol = "runtime".into();

    RbRef::new_object(
        "Gem::Dependency",
        &vec![
            ("@name".into(), dep.name.as_str().into()),
            ("@prerelease".into(), RbAny::False),
            ("@requirement".into(), requirement.clone()),
            ("@version_requirements".into(), requirement),
            ("@type".into(), type_symbol.as_any()),
        ],
    )
    .into_any()
}
