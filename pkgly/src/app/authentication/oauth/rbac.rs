use ahash::{HashSet, HashSetExt};

use anyhow::Context;
use casbin::{CoreApi, DefaultModel, Enforcer, MemoryAdapter, MgmtApi, RbacApi};
use tokio::sync::RwLock;
use tracing::warn;

use crate::app::config::OAuth2CasbinConfig;

pub struct OAuth2Rbac {
    enforcer: RwLock<Enforcer>,
}

impl OAuth2Rbac {
    pub async fn from_config(config: &OAuth2CasbinConfig) -> anyhow::Result<Self> {
        let model = DefaultModel::from_str(config.model.as_str())
            .await
            .context("Failed to parse Casbin model")?;
        let adapter = MemoryAdapter::default();
        let mut enforcer = Enforcer::new(model, adapter)
            .await
            .context("Failed to initialize Casbin enforcer")?;
        load_inline_policy(&mut enforcer, &config.policy).await?;
        Ok(Self {
            enforcer: RwLock::new(enforcer),
        })
    }

    pub async fn set_roles_for_user(&self, subject: &str, roles: &[String]) -> anyhow::Result<()> {
        let mut enforcer = self.enforcer.write().await;
        enforcer
            .delete_roles_for_user(subject, None)
            .await
            .with_context(|| format!("Failed to clear roles for subject '{subject}'"))?;

        let mut unique_roles = HashSet::new();
        for role in roles {
            if role.is_empty() || !unique_roles.insert(role.to_string()) {
                continue;
            }
            enforcer
                .add_role_for_user(subject, role, None)
                .await
                .with_context(|| format!("Failed to add role '{role}' for subject '{subject}'"))?;
        }

        Ok(())
    }

    pub async fn enforce(&self, subject: &str, object: &str, action: &str) -> anyhow::Result<bool> {
        let enforcer = self.enforcer.read().await;
        enforcer
            .enforce((subject, object, action))
            .with_context(|| {
                format!(
                    "Failed to evaluate RBAC policy for subject='{subject}', object='{object}', action='{action}'"
                )
            })
    }

    pub async fn roles_for_user(&self, subject: &str) -> anyhow::Result<Vec<String>> {
        let enforcer = self.enforcer.read().await;
        Ok(enforcer.get_roles_for_user(subject, None))
    }
}

async fn load_inline_policy(enforcer: &mut Enforcer, policy: &str) -> anyhow::Result<()> {
    for (index, line) in policy.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let segments: Vec<String> = trimmed
            .split(',')
            .map(|segment| segment.trim().to_string())
            .filter(|segment| !segment.is_empty())
            .collect();
        if let Some((rule_type, values)) = segments.split_first() {
            match rule_type.as_str() {
                "p" => {
                    enforcer
                        .add_policy(values.to_vec())
                        .await
                        .with_context(|| format!("Failed to add policy on line {}", index + 1))?;
                }
                "g" => {
                    enforcer
                        .add_grouping_policy(values.to_vec())
                        .await
                        .with_context(|| {
                            format!("Failed to add grouping policy on line {}", index + 1)
                        })?;
                }
                other => {
                    warn!(line = index + 1, rule_type = %other, "Unknown Casbin policy row");
                }
            }
        }
    }
    Ok(())
}
