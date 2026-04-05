use std::fmt::Display;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

#[derive(Debug, Error)]
#[error("Invalid NPM Package Name: {name} - {reason}")]
pub struct InvalidNPMPackageName {
    pub name: String,
    pub reason: &'static str,
}
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct NPMPackageName {
    pub name: String,
    pub scope: Option<String>,
}
impl Display for NPMPackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.scope {
            Some(scope) => write!(f, "@{}/{}", scope, self.name),
            None => write!(f, "{}", self.name),
        }
    }
}
impl NPMPackageName {
    pub fn validate_name(name: &str) -> Result<(), InvalidNPMPackageName> {
        for c in name.chars() {
            if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
                return Err(InvalidNPMPackageName {
                    name: name.to_owned(),
                    reason: "All characters must be alphanumeric, `_`, or `-`",
                });
            }
            if c.is_alphabetic() && !c.is_ascii_lowercase() {
                return Err(InvalidNPMPackageName {
                    name: name.to_owned(),
                    reason: "All characters must be lowercase",
                });
            }
        }
        Ok(())
    }
}
impl TryFrom<String> for NPMPackageName {
    type Error = InvalidNPMPackageName;
    #[instrument(name = "NPMPackageName::try_from")]
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.starts_with('@') {
            let parts: Vec<_> = value.split('/').collect();
            if parts.len() != 2 {
                return Err(InvalidNPMPackageName {
                    name: value,
                    reason: "Invalid scope format. Must be @scope/name",
                });
            }
            let scope = parts
                .first()
                .map(|s| s.trim_start_matches('@').to_string())
                .ok_or_else(|| InvalidNPMPackageName {
                    name: value.clone(),
                    reason: "Invalid scope format. Must be @scope/name",
                })?;
            if scope.is_empty() {
                return Err(InvalidNPMPackageName {
                    name: value,
                    reason: "Scope cannot be empty",
                });
            }
            let name = parts
                .get(1)
                .map(|s| s.to_string())
                .ok_or(InvalidNPMPackageName {
                    name: "unknown".to_string(),
                    reason: "No package name provided",
                })?;
            NPMPackageName::validate_name(&name)?;
            NPMPackageName::validate_name(&scope)?;
            Ok(NPMPackageName {
                name,
                scope: Some(scope),
            })
        } else {
            NPMPackageName::validate_name(&value)?;
            Ok(NPMPackageName {
                name: value,
                scope: None,
            })
        }
    }
}
impl TryFrom<&str> for NPMPackageName {
    type Error = InvalidNPMPackageName;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        NPMPackageName::try_from(value.to_owned())
    }
}
impl Serialize for NPMPackageName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match &self.scope {
            Some(scope) => serializer.serialize_str(&format!("@{}/{}", scope, self.name)),
            None => serializer.serialize_str(&self.name),
        }
    }
}
impl<'de> Deserialize<'de> for NPMPackageName {
    fn deserialize<D>(deserializer: D) -> Result<NPMPackageName, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        NPMPackageName::try_from(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
pub mod tests;
