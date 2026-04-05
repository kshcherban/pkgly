use sqlx::PgPool;

pub async fn is_username_taken(
    username: impl AsRef<str>,
    database: &PgPool,
) -> Result<bool, sqlx::Error> {
    let user = sqlx::query("SELECT id FROM users WHERE username = $1")
        .bind(username.as_ref())
        .fetch_optional(database)
        .await?;
    Ok(user.is_some())
}

pub async fn is_username_taken_by_other(
    username: impl AsRef<str>,
    user_id: i32,
    database: &PgPool,
) -> Result<bool, sqlx::Error> {
    let user = sqlx::query("SELECT id FROM users WHERE username = $1 AND id <> $2")
        .bind(username.as_ref())
        .bind(user_id)
        .fetch_optional(database)
        .await?;
    Ok(user.is_some())
}
pub async fn is_email_taken(
    email: impl AsRef<str>,
    database: &PgPool,
) -> Result<bool, sqlx::Error> {
    let user = sqlx::query("SELECT id FROM users WHERE email = $1")
        .bind(email.as_ref())
        .fetch_optional(database)
        .await?;
    Ok(user.is_some())
}

pub async fn is_email_taken_by_other(
    email: impl AsRef<str>,
    user_id: i32,
    database: &PgPool,
) -> Result<bool, sqlx::Error> {
    let user = sqlx::query("SELECT id FROM users WHERE email = $1 AND id <> $2")
        .bind(email.as_ref())
        .bind(user_id)
        .fetch_optional(database)
        .await?;
    Ok(user.is_some())
}
pub async fn does_user_exist(database: &PgPool) -> Result<bool, sqlx::Error> {
    let user = sqlx::query("SELECT id FROM users WHERE active = true LIMIT 1")
        .fetch_optional(database)
        .await?;
    Ok(user.is_some())
}
