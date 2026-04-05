use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema, PartialEq, Eq)]
pub struct VirtualRepositoryConfig {
    #[serde(default)]
    pub member_repositories: Vec<VirtualRepositoryMemberConfig>,
    #[serde(default)]
    pub resolution_order: VirtualResolutionOrder,
    #[serde(default = "default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub publish_to: Option<Uuid>,
}

impl Default for VirtualRepositoryConfig {
    fn default() -> Self {
        Self {
            member_repositories: Vec::new(),
            resolution_order: VirtualResolutionOrder::default(),
            cache_ttl_seconds: default_cache_ttl_seconds(),
            publish_to: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema, PartialEq, Eq)]
pub struct VirtualRepositoryMemberConfig {
    #[schemars(with = "String")]
    #[schema(value_type = String, format = "uuid")]
    pub repository_id: Uuid,
    pub repository_name: String,
    #[serde(default)]
    pub priority: u32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema, PartialEq, Eq, Default)]
pub enum VirtualResolutionOrder {
    #[default]
    Priority,
}

const fn default_enabled() -> bool {
    true
}

const fn default_cache_ttl_seconds() -> u64 {
    60
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtualConfigValidationError {
    EmptyMembers,
    DuplicateMember(Uuid),
    EmptyMemberName(Uuid),
    InvalidCacheTtlSeconds,
}

impl std::fmt::Display for VirtualConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMembers => write!(f, "Virtual repository requires at least one member"),
            Self::DuplicateMember(id) => write!(f, "Duplicate member repository {id}"),
            Self::EmptyMemberName(id) => write!(f, "Member repository {id} name cannot be empty"),
            Self::InvalidCacheTtlSeconds => {
                write!(f, "cache_ttl_seconds must be greater than zero")
            }
        }
    }
}

pub fn validate_virtual_repository_config(
    config: &VirtualRepositoryConfig,
) -> Result<(), VirtualConfigValidationError> {
    if config.member_repositories.is_empty() {
        return Err(VirtualConfigValidationError::EmptyMembers);
    }

    let mut seen = HashSet::new();
    for member in &config.member_repositories {
        if !seen.insert(member.repository_id) {
            return Err(VirtualConfigValidationError::DuplicateMember(
                member.repository_id,
            ));
        }
        if member.repository_name.trim().is_empty() {
            return Err(VirtualConfigValidationError::EmptyMemberName(
                member.repository_id,
            ));
        }
    }

    if config.cache_ttl_seconds == 0 {
        return Err(VirtualConfigValidationError::InvalidCacheTtlSeconds);
    }

    Ok(())
}
