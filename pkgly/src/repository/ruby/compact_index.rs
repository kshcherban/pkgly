use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use md5::Md5;
use nr_core::repository::project::{RubyDependencyMetadata, RubyPackageMetadata};
use sha2::Digest;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq)]
pub struct CompactIndexVersionEntry {
    pub version: String,
    pub dependencies: Vec<RubyDependencyMetadata>,
    pub sha256: String,
    pub required_ruby: Option<String>,
    pub required_rubygems: Option<String>,
}

pub fn build_names_file(names: &[String]) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    for name in names {
        out.push_str(name);
        out.push('\n');
    }
    out
}

pub fn build_info_file(entries: &[CompactIndexVersionEntry]) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    for entry in entries {
        out.push_str(&entry.version);
        out.push(' ');
        out.push_str(&format_dependencies(&entry.dependencies));
        out.push('|');

        let mut requirements = Vec::new();
        requirements.push(format!("checksum:{}", entry.sha256));
        if let Some(value) = entry.required_ruby.as_deref() {
            requirements.push(format!("ruby:{value}"));
        }
        if let Some(value) = entry.required_rubygems.as_deref() {
            requirements.push(format!("rubygems:{value}"));
        }
        out.push_str(&requirements.join(","));
        out.push('\n');
    }
    out
}

pub fn build_versions_file(created_at: DateTime<Utc>, lines: &[VersionsLine]) -> String {
    let mut out = String::new();
    out.push_str("created_at: ");
    out.push_str(&created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    out.push('\n');
    out.push_str("---\n");
    for line in lines {
        out.push_str(&line.gem_name);
        out.push(' ');
        out.push_str(&line.versions.join(","));
        out.push(' ');
        out.push_str(&line.info_md5);
        out.push('\n');
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha2::Sha256::digest(bytes);
    format!("{digest:x}")
}

pub fn md5_hex(bytes: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionsLine {
    pub gem_name: String,
    pub versions: Vec<String>,
    pub info_md5: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RubyCompactIndexRow {
    pub gem_key: String,
    pub gem_name: String,
    pub version: String,
    pub metadata: RubyPackageMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactIndexArtifacts {
    pub names: String,
    pub versions: String,
    pub infos: BTreeMap<String, String>,
}

pub fn build_compact_index_artifacts(
    created_at: DateTime<Utc>,
    rows: &[RubyCompactIndexRow],
) -> Result<CompactIndexArtifacts, String> {
    let mut grouped: BTreeMap<String, (String, Vec<CompactIndexVersionEntry>)> = BTreeMap::new();
    for row in rows {
        let sha256 = row
            .metadata
            .sha256
            .clone()
            .ok_or_else(|| format!("ruby metadata missing sha256 for {}", row.gem_name))?;

        let entry = CompactIndexVersionEntry {
            version: row.version.clone(),
            dependencies: row.metadata.dependencies.clone(),
            sha256,
            required_ruby: row.metadata.required_ruby.clone(),
            required_rubygems: row.metadata.required_rubygems.clone(),
        };

        grouped
            .entry(row.gem_key.clone())
            .and_modify(|(_, versions)| versions.push(entry.clone()))
            .or_insert_with(|| (row.gem_name.clone(), vec![entry]));
    }

    let names: Vec<String> = grouped.keys().cloned().collect();
    let names_file = build_names_file(&names);

    let mut infos = BTreeMap::new();
    let mut version_lines = Vec::new();
    for (gem_key, (gem_name, mut versions)) in grouped {
        versions.sort_by(|a, b| a.version.cmp(&b.version));
        let info_file = build_info_file(&versions);
        let info_md5 = md5_hex(info_file.as_bytes());
        infos.insert(gem_key.clone(), info_file);
        version_lines.push(VersionsLine {
            gem_name,
            versions: versions.iter().map(|v| v.version.clone()).collect(),
            info_md5,
        });
    }
    version_lines.sort_by(|a, b| a.gem_name.cmp(&b.gem_name));

    let versions_file = build_versions_file(created_at, &version_lines);

    Ok(CompactIndexArtifacts {
        names: names_file,
        versions: versions_file,
        infos,
    })
}

fn format_dependencies(dependencies: &[RubyDependencyMetadata]) -> String {
    if dependencies.is_empty() {
        return String::new();
    }
    let mut chunks = Vec::with_capacity(dependencies.len());
    for dep in dependencies {
        let requirements = dep.requirements.join("&");
        chunks.push(format!("{}:{}", dep.name, requirements));
    }
    chunks.join(",")
}
