use semver::{Version, VersionReq};

use super::query_parser::Operator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionConstraint {
    Exact(String),
    Range { op: Operator, version: String },
    Semver(VersionReq),
}

impl VersionConstraint {
    pub fn matches(&self, version: &str) -> bool {
        match self {
            VersionConstraint::Exact(expected) => {
                if let (Some(lhs), Some(rhs)) = (
                    parse_semver_version(version),
                    parse_semver_version(expected),
                ) {
                    lhs == rhs
                } else {
                    version.eq_ignore_ascii_case(expected)
                }
            }
            VersionConstraint::Range {
                op,
                version: target,
            } => match op {
                Operator::Contains => version.to_lowercase().contains(&target.to_lowercase()),
                Operator::Equals => version.eq_ignore_ascii_case(target),
                Operator::GreaterThan
                | Operator::GreaterOrEqual
                | Operator::LessThan
                | Operator::LessOrEqual => compare_range(op, version, target),
            },
            VersionConstraint::Semver(req) => match parse_semver_version(version) {
                Some(parsed) => req.matches(&parsed),
                None => false,
            },
        }
    }
}

fn compare_range(op: &Operator, candidate: &str, target: &str) -> bool {
    if let (Some(lhs), Some(rhs)) = (
        parse_semver_version(candidate),
        parse_semver_version(target),
    ) {
        return match op {
            Operator::GreaterThan => lhs > rhs,
            Operator::GreaterOrEqual => lhs >= rhs,
            Operator::LessThan => lhs < rhs,
            Operator::LessOrEqual => lhs <= rhs,
            Operator::Equals => lhs == rhs,
            Operator::Contains => lhs
                .to_string()
                .to_lowercase()
                .contains(&target.to_lowercase()),
        };
    }

    let lhs = candidate.to_lowercase();
    let rhs = target.to_lowercase();
    match op {
        Operator::GreaterThan => lhs > rhs,
        Operator::GreaterOrEqual => lhs >= rhs,
        Operator::LessThan => lhs < rhs,
        Operator::LessOrEqual => lhs <= rhs,
        Operator::Equals => lhs == rhs,
        Operator::Contains => lhs.contains(&rhs),
    }
}

fn parse_semver_version(value: &str) -> Option<Version> {
    if looks_like_debian_version(value) {
        parse_debian_semver(value).or_else(|| parse_plain_semver(value))
    } else {
        parse_plain_semver(value).or_else(|| parse_debian_semver(value))
    }
}

fn parse_plain_semver(value: &str) -> Option<Version> {
    let trimmed = value.trim();
    let normalized = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(normalized).ok()
}

fn parse_debian_semver(value: &str) -> Option<Version> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip epoch (e.g., 1:2.3.4-1).
    let without_epoch = trimmed
        .rsplit_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    // Separate upstream version from Debian revision (after '-').
    let (upstream, _) = without_epoch.split_once('-').unwrap_or((without_epoch, ""));
    if upstream.is_empty() {
        return None;
    }
    // Handle pre-release marker (~) by turning it into '-' as SemVer expects.
    let (base, prerelease) = upstream
        .split_once('~')
        .map_or((upstream, None), |(b, p)| (b, Some(p)));
    // Keep only numeric and dot separators for the core version.
    let mut numeric_parts: Vec<String> = base
        .split('.')
        .map(|segment| {
            segment
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|segment| !segment.is_empty())
        .collect();
    if numeric_parts.is_empty() {
        return None;
    }
    while numeric_parts.len() < 3 {
        numeric_parts.push("0".into());
    }
    let mut normalized = numeric_parts[..3].join(".");
    if let Some(prerelease) = prerelease {
        let trimmed = prerelease.trim_start_matches(|ch| ch == '-' || ch == '~' || ch == '+');
        let cleaned: String = trimmed
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' {
                    ch
                } else if ch == '~' || ch == '_' {
                    '-'
                } else {
                    ch
                }
            })
            .collect();
        if !cleaned.is_empty() {
            normalized.push('-');
            normalized.push_str(&cleaned);
        }
    }
    Version::parse(&normalized).ok()
}

fn looks_like_debian_version(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains(':') || trimmed.contains('~') || trimmed.contains('+') {
        return true;
    }
    trimmed
        .rsplit_once('-')
        .map(|(_, suffix)| {
            !suffix.is_empty()
                && suffix
                    .chars()
                    .next()
                    .map_or(false, |ch| ch.is_ascii_digit())
                && !suffix.contains('.')
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
