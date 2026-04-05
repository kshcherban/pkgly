use anyhow::Context;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::PgPool;

pub struct ApplicationSettings;

impl ApplicationSettings {
    const TABLE: &'static str = "application_settings";

    pub async fn get<T>(key: &str, database: &PgPool) -> anyhow::Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let value: Option<Value> =
            sqlx::query_scalar(&format!("SELECT value FROM {} WHERE key = $1", Self::TABLE))
                .bind(key)
                .fetch_optional(database)
                .await
                .with_context(|| format!("Failed to fetch application setting `{key}`"))?;

        let Some(value) = value else {
            return Ok(None);
        };

        let settings = serde_json::from_value(value)
            .with_context(|| format!("Failed to deserialize application setting `{key}`"))?;

        Ok(Some(settings))
    }

    pub async fn upsert<T>(key: &str, value: &T, database: &PgPool) -> anyhow::Result<()>
    where
        T: Serialize,
    {
        let value = serde_json::to_value(value)
            .with_context(|| format!("Failed to serialize application setting `{key}`"))?;

        sqlx::query(&format!(
            "INSERT INTO {} (key, value) VALUES ($1, $2) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            Self::TABLE
        ))
        .bind(key)
        .bind(value)
        .execute(database)
        .await
        .with_context(|| format!("Failed to upsert application setting `{key}`"))?;

        Ok(())
    }

    pub async fn delete(key: &str, database: &PgPool) -> anyhow::Result<()> {
        sqlx::query(&format!("DELETE FROM {} WHERE key = $1", Self::TABLE))
            .bind(key)
            .execute(database)
            .await
            .with_context(|| format!("Failed to delete application setting `{key}`"))?;

        Ok(())
    }
}
