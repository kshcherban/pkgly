use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::config::{self, Mode, OAuth2Settings, PasswordRules, SsoSettings};

/// Public instance metadata exposed to clients.
#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct Instance {
    pub app_url: String,
    pub name: String,
    pub description: String,
    pub is_https: bool,
    pub is_installed: bool,
    #[schema(value_type = String)]
    pub version: semver::Version,
    pub mode: Mode,
    pub password_rules: Option<PasswordRules>,
    pub sso: Option<InstanceSsoSettings>,
    pub oauth2: Option<InstanceOAuth2Settings>,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct InstanceSsoSettings {
    pub login_path: String,
    pub login_button_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_login_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_redirect_param: Option<String>,
    pub auto_create_users: bool,
}

impl From<&SsoSettings> for InstanceSsoSettings {
    fn from(settings: &SsoSettings) -> Self {
        Self {
            login_path: settings.login_path.clone(),
            login_button_text: settings.login_button_text.clone(),
            provider_login_url: settings.provider_login_url.clone(),
            provider_redirect_param: settings.provider_redirect_param.clone(),
            auto_create_users: settings.auto_create_users,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct InstanceOAuth2Settings {
    pub login_path: String,
    pub callback_path: String,
    pub providers: Vec<InstanceOAuth2Provider>,
    pub auto_create_users: bool,
    pub group_role_mappings: Vec<config::OAuth2GroupRoleMapping>,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct InstanceOAuth2Provider {
    pub provider: String,
    pub redirect_path: Option<String>,
}

impl From<&OAuth2Settings> for InstanceOAuth2Settings {
    fn from(settings: &OAuth2Settings) -> Self {
        let mut providers = Vec::new();
        if settings.google.is_some() {
            providers.push(InstanceOAuth2Provider {
                provider: "google".to_string(),
                redirect_path: settings
                    .google
                    .as_ref()
                    .and_then(|cfg| cfg.redirect_path.clone()),
            });
        }
        if settings.microsoft.is_some() {
            providers.push(InstanceOAuth2Provider {
                provider: "microsoft".to_string(),
                redirect_path: settings
                    .microsoft
                    .as_ref()
                    .and_then(|cfg| cfg.redirect_path.clone()),
            });
        }
        Self {
            login_path: settings.login_path.clone(),
            callback_path: settings.callback_path.clone(),
            providers,
            auto_create_users: settings.auto_create_users,
            group_role_mappings: settings.group_role_mappings.clone(),
        }
    }
}

/// Case-insensitive storage/repository name pair used for lookups.
#[derive(Debug, Clone, Hash, PartialEq, Eq, IntoParams, Deserialize)]
#[into_params(parameter_in = Path)]
pub struct RepositoryStorageName {
    /// The name of the storage
    pub storage_name: String,
    /// The name of the repository
    pub repository_name: String,
}

impl RepositoryStorageName {
    pub async fn query_db(&self, database: &PgPool) -> Result<Option<Uuid>, sqlx::Error> {
        let query: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT repositories.id FROM repositories INNER JOIN storages
                    ON storages.id = repositories.storage_id AND storages.name = $1
                    WHERE repositories.name = $2"#,
        )
        .bind(&self.storage_name)
        .bind(&self.repository_name)
        .fetch_optional(database)
        .await?;
        Ok(query)
    }
}

impl From<(&str, &str)> for RepositoryStorageName {
    fn from((storage_name, repository_name): (&str, &str)) -> Self {
        Self {
            storage_name: storage_name.to_lowercase(),
            repository_name: repository_name.to_lowercase(),
        }
    }
}

impl From<(String, String)> for RepositoryStorageName {
    fn from((storage_name, repository_name): (String, String)) -> Self {
        Self {
            storage_name: storage_name.to_lowercase(),
            repository_name: repository_name.to_lowercase(),
        }
    }
}
