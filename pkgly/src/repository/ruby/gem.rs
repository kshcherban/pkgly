use std::{io::Read, path::Path};

use flate2::read::GzDecoder;
use nr_core::repository::project::RubyDependencyMetadata;
use serde_yaml::{Mapping, Value};
use tar::Archive;
use thurgood::rc::{RbAny, RbObject, RbRef};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedGemSpec {
    pub name: String,
    pub version: String,
    pub platform: Option<String>,
    pub dependencies: Vec<RubyDependencyMetadata>,
    pub required_ruby: Option<String>,
    pub required_rubygems: Option<String>,
}

const MAX_METADATA_GZIP_BYTES: u64 = 1024 * 1024;
const MAX_METADATA_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum GemParseError {
    #[error("Failed to read gem file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid gem file: {0}")]
    Invalid(String),
}

pub fn parse_gemspec_from_gem_path(path: &Path) -> Result<ParsedGemSpec, GemParseError> {
    let file = std::fs::File::open(path)?;
    parse_gemspec_from_reader(file)
}

fn parse_gemspec_from_reader<R: Read>(reader: R) -> Result<ParsedGemSpec, GemParseError> {
    let mut archive = Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|err| GemParseError::Invalid(format!("Invalid gem archive: {err}")))?;

    for entry in entries {
        let entry = entry
            .map_err(|err| GemParseError::Invalid(format!("Invalid gem archive entry: {err}")))?;
        let path = entry.path().map_err(|err| {
            GemParseError::Invalid(format!("Invalid gem archive entry path: {err}"))
        })?;
        if path.as_os_str() != "metadata.gz" {
            continue;
        }

        let mut compressed = Vec::new();
        let mut limited = entry.take(MAX_METADATA_GZIP_BYTES + 1);
        limited.read_to_end(&mut compressed)?;
        if compressed.len() as u64 > MAX_METADATA_GZIP_BYTES {
            return Err(GemParseError::Invalid(
                "Gem metadata is too large".to_string(),
            ));
        }

        let decoder = GzDecoder::new(&compressed[..]);
        let mut metadata = Vec::new();
        decoder
            .take((MAX_METADATA_BYTES as u64) + 1)
            .read_to_end(&mut metadata)
            .map_err(|err| {
                GemParseError::Invalid(format!("Failed to decompress gem metadata: {err}"))
            })?;
        if metadata.len() > MAX_METADATA_BYTES {
            return Err(GemParseError::Invalid(
                "Gem metadata is too large".to_string(),
            ));
        }

        return parse_gemspec_metadata(&metadata);
    }

    Err(GemParseError::Invalid(
        "Missing metadata.gz in gem archive".to_string(),
    ))
}

fn parse_gemspec_metadata(metadata: &[u8]) -> Result<ParsedGemSpec, GemParseError> {
    if metadata.starts_with(b"---") {
        return parse_yaml_gemspec(metadata);
    }

    match thurgood::rc::from_reader(std::io::Cursor::new(metadata)) {
        Ok(gemspec) => extract_gemspec(gemspec).map_err(GemParseError::Invalid),
        Err(err) => {
            if let Ok(parsed) = parse_yaml_gemspec(metadata) {
                return Ok(parsed);
            }
            Err(GemParseError::Invalid(format!(
                "Failed to parse gemspec metadata: {err}"
            )))
        }
    }
}

fn extract_gemspec(gemspec: RbAny) -> Result<ParsedGemSpec, String> {
    let object = gemspec
        .as_object()
        .ok_or_else(|| "Gemspec metadata is not an object".to_string())?;

    let name =
        get_string_field(object, "@name").ok_or_else(|| "Gemspec missing @name".to_string())?;

    let version_value = object
        .get("@version")
        .ok_or_else(|| "Gemspec missing @version".to_string())?;
    let version = extract_gem_version(version_value)
        .ok_or_else(|| "Gemspec @version is invalid".to_string())?;

    let platform = object
        .get("@platform")
        .and_then(extract_platform)
        .filter(|platform| platform != "ruby");

    let dependencies = object
        .get("@dependencies")
        .and_then(RbAny::as_array)
        .map(|values| extract_dependencies(values))
        .transpose()
        .map_err(|err| err.to_string())?
        .unwrap_or_default();

    let required_ruby = object
        .get("@required_ruby_version")
        .and_then(extract_requirement);

    let required_rubygems = object
        .get("@required_rubygems_version")
        .and_then(extract_requirement);

    Ok(ParsedGemSpec {
        name,
        version,
        platform,
        dependencies,
        required_ruby,
        required_rubygems,
    })
}

fn extract_dependencies(values: &[RbAny]) -> Result<Vec<RubyDependencyMetadata>, GemParseError> {
    let mut dependencies = Vec::new();
    for dep in values {
        let Some(dep_obj) = dep.as_object() else {
            continue;
        };

        let dep_type = dep_obj.get("@type").and_then(extract_symbol_or_string);
        if matches!(dep_type.as_deref(), Some("development")) {
            continue;
        }

        let Some(name) = get_string_field(dep_obj, "@name") else {
            continue;
        };
        let requirement = dep_obj
            .get("@requirement")
            .and_then(extract_requirement_constraints)
            .unwrap_or_default();
        dependencies.push(RubyDependencyMetadata {
            name,
            requirements: requirement,
        });
    }

    Ok(dependencies)
}

fn extract_platform(value: &RbAny) -> Option<String> {
    extract_symbol_or_string(value).or_else(|| {
        let obj = value.as_object()?;
        let name = obj.name.as_str()?;
        if name != "Gem::Platform" {
            return None;
        }
        let cpu = get_string_field(obj, "@cpu")?;
        let os = get_string_field(obj, "@os")?;
        let version = get_string_field(obj, "@version");
        match version {
            Some(version) if !version.is_empty() => Some(format!("{cpu}-{os}-{version}")),
            _ => Some(format!("{cpu}-{os}")),
        }
    })
}

fn extract_gem_version(value: &RbAny) -> Option<String> {
    extract_symbol_or_string(value).or_else(|| {
        let obj = value.as_object()?;
        let name = obj.name.as_str()?;
        if name != "Gem::Version" {
            return None;
        }
        get_string_field(obj, "@version")
    })
}

fn extract_requirement(value: &RbAny) -> Option<String> {
    let constraints = extract_requirement_constraints(value)?;
    if constraints.is_empty() {
        None
    } else {
        Some(constraints.join("&"))
    }
}

fn extract_requirement_constraints(value: &RbAny) -> Option<Vec<String>> {
    let obj = value.as_object()?;
    let name = obj.name.as_str()?;
    if name != "Gem::Requirement" {
        return None;
    }

    let requirements = obj.get("@requirements")?.as_array()?;
    let mut out = Vec::new();
    for item in requirements {
        let entry = item.as_array()?;
        if entry.len() != 2 {
            continue;
        }
        let op = extract_symbol_or_string(&entry[0])?;
        let version = extract_gem_version(&entry[1])?;
        out.push(format!("{op} {version}"));
    }
    Some(out)
}

fn get_string_field(object: &RbObject, key: &str) -> Option<String> {
    object.get(key).and_then(extract_string)
}

fn extract_symbol_or_string(value: &RbAny) -> Option<String> {
    match value {
        RbAny::Symbol(symbol) => symbol.as_str().map(str::to_string),
        _ => extract_string(value),
    }
}

fn extract_string(value: &RbAny) -> Option<String> {
    if let Some(value) = value.as_string() {
        return Some(value.clone());
    }

    let rbref = value.as_rbref()?;
    if let RbRef::StrI { content, .. } = rbref {
        return std::str::from_utf8(content).ok().map(str::to_string);
    }

    None
}

fn parse_yaml_gemspec(metadata: &[u8]) -> Result<ParsedGemSpec, GemParseError> {
    let spec: Value = serde_yaml::from_slice(metadata)
        .map_err(|err| GemParseError::Invalid(format!("Failed to parse gemspec YAML: {err}")))?;
    let map = spec
        .as_mapping()
        .ok_or_else(|| GemParseError::Invalid("Gemspec YAML is not a mapping".to_string()))?;

    let name = yaml_string(map, "name")
        .ok_or_else(|| GemParseError::Invalid("Gemspec missing name".to_string()))?;
    let version_value = yaml_get(map, "version")
        .ok_or_else(|| GemParseError::Invalid("Gemspec missing version".to_string()))?;
    let version = yaml_version(version_value)
        .ok_or_else(|| GemParseError::Invalid("Gemspec version is invalid".to_string()))?;

    let platform = yaml_get(map, "platform")
        .and_then(yaml_string_value)
        .filter(|platform| platform != "ruby");

    let dependencies = yaml_get(map, "dependencies")
        .and_then(Value::as_sequence)
        .map(|values| extract_yaml_dependencies(values))
        .transpose()?
        .unwrap_or_default();

    let required_ruby = yaml_get(map, "required_ruby_version").and_then(extract_yaml_requirement);
    let required_rubygems =
        yaml_get(map, "required_rubygems_version").and_then(extract_yaml_requirement);

    Ok(ParsedGemSpec {
        name,
        version,
        platform,
        dependencies,
        required_ruby,
        required_rubygems,
    })
}

fn yaml_get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(&Value::String(key.to_string()))
}

fn yaml_string(map: &Mapping, key: &str) -> Option<String> {
    yaml_get(map, key).and_then(yaml_string_value)
}

fn yaml_string_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn yaml_version(value: &Value) -> Option<String> {
    yaml_string_value(value).or_else(|| {
        let map = value.as_mapping()?;
        yaml_string(map, "version")
    })
}

fn extract_yaml_dependencies(
    values: &[Value],
) -> Result<Vec<RubyDependencyMetadata>, GemParseError> {
    let mut dependencies = Vec::new();
    for value in values {
        let Some(map) = value.as_mapping() else {
            continue;
        };

        let dep_type = yaml_string(map, "type")
            .unwrap_or_else(|| "runtime".to_string())
            .trim_start_matches(':')
            .to_string();
        if dep_type == "development" {
            continue;
        }

        let Some(name) = yaml_string(map, "name") else {
            continue;
        };

        let requirements = yaml_get(map, "requirement")
            .and_then(extract_yaml_requirement_constraints)
            .unwrap_or_default();

        dependencies.push(RubyDependencyMetadata { name, requirements });
    }
    Ok(dependencies)
}

fn extract_yaml_requirement(value: &Value) -> Option<String> {
    let constraints = extract_yaml_requirement_constraints(value)?;
    if constraints.is_empty() {
        None
    } else {
        Some(constraints.join("&"))
    }
}

fn extract_yaml_requirement_constraints(value: &Value) -> Option<Vec<String>> {
    let map = value.as_mapping()?;
    let requirements = yaml_get(map, "requirements")?.as_sequence()?;

    let mut out = Vec::new();
    for entry in requirements {
        let pair = entry.as_sequence()?;
        if pair.len() != 2 {
            continue;
        }
        let op = yaml_string_value(pair.first()?)?;
        let version = yaml_version(pair.get(1)?)?;
        out.push(format!("{op} {version}"));
    }
    Some(out)
}
