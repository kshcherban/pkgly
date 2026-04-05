use std::fmt;

use semver::VersionReq;
use thiserror::Error;

use super::version_constraint::VersionConstraint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Equals,
    Contains,
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Package,
    Version,
    Repository,
    Type,
    Storage,
    Digest,
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Field::Package => "package",
            Field::Version => "version",
            Field::Repository => "repository",
            Field::Type => "type",
            Field::Storage => "storage",
            Field::Digest => "digest",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("search query cannot be empty")]
    Empty,
    #[error("unknown field `{0}`")]
    UnknownField(String),
    #[error("invalid operator `{0}` for field {1}")]
    InvalidOperator(String, Field),
    #[error("missing value for field {0}")]
    MissingValue(Field),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub terms: Vec<String>,
    pub package_filter: Option<(Operator, String)>,
    pub digest_filter: Option<(Operator, String)>,
    pub version_constraint: Option<VersionConstraint>,
    pub repository_filter: Option<String>,
    pub type_filter: Option<String>,
    pub storage_filter: Option<String>,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            terms: Vec::new(),
            package_filter: None,
            digest_filter: None,
            version_constraint: None,
            repository_filter: None,
            type_filter: None,
            storage_filter: None,
        }
    }
}

impl SearchQuery {
    #[must_use]
    pub fn has_filters(&self) -> bool {
        self.package_filter.is_some()
            || self.digest_filter.is_some()
            || self.version_constraint.is_some()
            || self.repository_filter.is_some()
            || self.type_filter.is_some()
            || self.storage_filter.is_some()
    }

    #[must_use]
    pub fn matches_repository(
        &self,
        repository_name: &str,
        storage_name: &str,
        repository_type: &str,
    ) -> bool {
        if let Some(repo) = &self.repository_filter {
            if !repository_name.eq_ignore_ascii_case(repo) {
                return false;
            }
        }
        if let Some(storage) = &self.storage_filter {
            if !storage_name.eq_ignore_ascii_case(storage) {
                return false;
            }
        }
        if let Some(repo_type) = &self.type_filter {
            if !repository_type.eq_ignore_ascii_case(repo_type) {
                return false;
            }
        }
        true
    }

    #[must_use]
    pub fn matches_package_names(&self, candidates: &[&str]) -> bool {
        if let Some((operator, filter)) = &self.package_filter {
            return candidates
                .iter()
                .any(|candidate| apply_string_operator(*operator, candidate, filter));
        }
        if self.terms.is_empty() {
            return true;
        }
        let lowered: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.to_lowercase())
            .collect();
        self.terms
            .iter()
            .all(|term| lowered.iter().any(|value| value.contains(term)))
    }

    #[must_use]
    pub fn matches_terms(&self, fields: &[&str]) -> bool {
        if self.terms.is_empty() {
            return true;
        }
        let lowered: Vec<String> = fields.iter().map(|value| value.to_lowercase()).collect();
        let comparator: Box<dyn Fn(&String, &String) -> bool> =
            Box::new(|value, term| value.contains(term));

        self.terms
            .iter()
            .all(|term| lowered.iter().any(|value| comparator(value, term)))
    }

    #[must_use]
    pub fn matches_version(&self, candidate: &str) -> bool {
        if let Some(constraint) = &self.version_constraint {
            constraint.matches(candidate)
        } else {
            true
        }
    }
}

pub fn parse_search_query(input: &str) -> Result<SearchQuery, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let mut query = SearchQuery::default();
    let mut tokens = tokenize(trimmed).into_iter().peekable();

    while let Some(token) = tokens.next() {
        if let Some(index) = token.find(':') {
            let (field_str, raw_value) = token.split_at(index);
            let field = parse_field(field_str)?;
            let mut value_segment = raw_value[1..].trim().to_string();

            if value_segment.is_empty() {
                if let Some(next_token) = tokens.next() {
                    value_segment = next_token.trim().to_string();
                }
            }

            if value_segment.is_empty() {
                return Err(ParseError::MissingValue(field));
            }

            if field == Field::Version
                && (value_segment.starts_with('^') || value_segment.starts_with('~'))
            {
                if let Ok(req) = VersionReq::parse(&value_segment) {
                    query.version_constraint = Some(VersionConstraint::Semver(req));
                    continue;
                }
            }

            let (operator, remainder) = extract_operator(&value_segment);
            let mut value = remainder.trim().to_string();

            if value.is_empty() {
                if let Some(next_token) = tokens.next() {
                    value = next_token.trim().to_string();
                }
            }

            if value.is_empty() {
                return Err(ParseError::MissingValue(field));
            }
            match field {
                Field::Package => {
                    let value_owned = value;
                    let inferred_exact = value_owned.chars().any(char::is_whitespace);
                    let op = operator.unwrap_or_else(|| {
                        if inferred_exact {
                            Operator::Equals
                        } else {
                            Operator::Contains
                        }
                    });
                    if !matches!(op, Operator::Equals | Operator::Contains) {
                        return Err(ParseError::InvalidOperator(operator_to_string(op), field));
                    }
                    query.package_filter = Some((op, value_owned.to_lowercase()));
                }
                Field::Repository => {
                    validate_string_operator(operator, field)?;
                    query.repository_filter = Some(value.to_lowercase());
                }
                Field::Type => {
                    validate_string_operator(operator, field)?;
                    query.type_filter = Some(value.to_lowercase());
                }
                Field::Storage => {
                    validate_string_operator(operator, field)?;
                    query.storage_filter = Some(value.to_lowercase());
                }
                Field::Version => {
                    if let Some(op) = operator {
                        match op {
                            Operator::Equals => {
                                query.version_constraint =
                                    Some(VersionConstraint::Exact(value.to_string()));
                            }
                            Operator::Contains => {
                                query.version_constraint = Some(VersionConstraint::Range {
                                    op,
                                    version: value.to_lowercase(),
                                });
                            }
                            Operator::GreaterThan
                            | Operator::GreaterOrEqual
                            | Operator::LessThan
                            | Operator::LessOrEqual => {
                                query.version_constraint = Some(VersionConstraint::Range {
                                    op,
                                    version: value.to_string(),
                                });
                            }
                        }
                    } else if let Ok(req) = VersionReq::parse(&value) {
                        query.version_constraint = Some(VersionConstraint::Semver(req));
                    } else {
                        query.version_constraint =
                            Some(VersionConstraint::Exact(value.to_string()));
                    }
                }
                Field::Digest => {
                    let inferred_exact = value.contains(':');
                    let op = operator.unwrap_or_else(|| {
                        if inferred_exact {
                            Operator::Equals
                        } else {
                            Operator::Contains
                        }
                    });
                    if !matches!(op, Operator::Equals | Operator::Contains) {
                        return Err(ParseError::InvalidOperator(operator_to_string(op), field));
                    }
                    query.digest_filter = Some((op, value.to_lowercase()));
                }
            }
        } else {
            query.terms.push(token.to_lowercase());
        }
    }

    Ok(query)
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';

    for ch in input.chars() {
        match ch {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = ch;
            }
            _ if in_quotes && ch == quote_char => {
                in_quotes = false;
            }
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_field(value: &str) -> Result<Field, ParseError> {
    match value.to_lowercase().as_str() {
        "package" | "pkg" => Ok(Field::Package),
        "version" | "v" => Ok(Field::Version),
        "repository" | "repo" => Ok(Field::Repository),
        "type" => Ok(Field::Type),
        "storage" => Ok(Field::Storage),
        "digest" | "hash" => Ok(Field::Digest),
        other => Err(ParseError::UnknownField(other.to_string())),
    }
}

fn extract_operator(value: &str) -> (Option<Operator>, &str) {
    let value = value.trim_start();
    if let Some(rest) = value.strip_prefix(">=") {
        return (Some(Operator::GreaterOrEqual), rest);
    }
    if let Some(rest) = value.strip_prefix("<=") {
        return (Some(Operator::LessOrEqual), rest);
    }
    if let Some(rest) = value.strip_prefix('>') {
        return (Some(Operator::GreaterThan), rest);
    }
    if let Some(rest) = value.strip_prefix('<') {
        return (Some(Operator::LessThan), rest);
    }
    if let Some(rest) = value.strip_prefix('=') {
        return (Some(Operator::Equals), rest);
    }
    if let Some(rest) = value.strip_prefix('~') {
        return (Some(Operator::Contains), rest);
    }
    (None, value)
}

fn validate_string_operator(operator: Option<Operator>, field: Field) -> Result<(), ParseError> {
    if let Some(op) = operator {
        if !matches!(op, Operator::Equals) {
            return Err(ParseError::InvalidOperator(operator_to_string(op), field));
        }
    }
    Ok(())
}

fn apply_string_operator(operator: Operator, candidate: &str, needle: &str) -> bool {
    let candidate_lower = candidate.to_lowercase();
    match operator {
        Operator::Equals => candidate_lower == needle,
        Operator::Contains => candidate_lower.contains(needle),
        _ => false,
    }
}

fn operator_to_string(operator: Operator) -> String {
    match operator {
        Operator::Equals => "=".to_string(),
        Operator::Contains => "~".to_string(),
        Operator::GreaterThan => ">".to_string(),
        Operator::GreaterOrEqual => ">=".to_string(),
        Operator::LessThan => "<".to_string(),
        Operator::LessOrEqual => "<=".to_string(),
    }
}

#[cfg(test)]
mod tests;
