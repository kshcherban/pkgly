use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct DBVirtualRepositoryMember {
    pub id: i32,
    pub virtual_repository_id: Uuid,
    pub member_repository_id: Uuid,
    pub priority: i32,
    pub enabled: bool,
    pub updated_at: DateTime<FixedOffset>,
    pub created_at: DateTime<FixedOffset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewVirtualRepositoryMember {
    pub member_repository_id: Uuid,
    pub priority: i32,
    pub enabled: bool,
}

impl DBVirtualRepositoryMember {
    pub async fn list_for_virtual(
        virtual_repository_id: Uuid,
        database: &PgPool,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM virtual_repository_members
            WHERE virtual_repository_id = $1
            ORDER BY priority ASC, member_repository_id ASC
            "#,
        )
        .bind(virtual_repository_id)
        .fetch_all(database)
        .await
    }

    pub async fn replace_all(
        virtual_repository_id: Uuid,
        members: &[NewVirtualRepositoryMember],
        database: &PgPool,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut transaction = database.begin().await?;
        sqlx::query("DELETE FROM virtual_repository_members WHERE virtual_repository_id = $1")
            .bind(virtual_repository_id)
            .execute(&mut *transaction)
            .await?;

        for member in members {
            sqlx::query(
                r#"
                INSERT INTO virtual_repository_members
                    (virtual_repository_id, member_repository_id, priority, enabled)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(virtual_repository_id)
            .bind(member.member_repository_id)
            .bind(member.priority)
            .bind(member.enabled)
            .execute(&mut *transaction)
            .await?;
        }

        let updated = sqlx::query_as(
            r#"
            SELECT * FROM virtual_repository_members
            WHERE virtual_repository_id = $1
            ORDER BY priority ASC, member_repository_id ASC
            "#,
        )
        .bind(virtual_repository_id)
        .fetch_all(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(updated)
    }
}
