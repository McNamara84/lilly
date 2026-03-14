use sqlx::MySqlPool;

use crate::models::user::User;

pub async fn find_user_by_email(
    pool: &MySqlPool,
    email: &str,
) -> Result<Option<User>, sqlx::Error> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name, email_verified FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn create_user(
    pool: &MySqlPool,
    email: &str,
    password_hash: &str,
    display_name: &str,
    verification_token: &str,
    verification_expires_at: chrono::NaiveDateTime,
    privacy_consent_at: chrono::NaiveDateTime,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO users (email, password_hash, display_name, verification_token, \
         verification_token_expires_at, privacy_consent_at, email_verified) \
         VALUES (?, ?, ?, ?, ?, ?, FALSE)",
    )
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .bind(verification_token)
    .bind(verification_expires_at)
    .bind(privacy_consent_at)
    .execute(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

pub async fn find_user_by_verification_token(
    pool: &MySqlPool,
    token: &str,
) -> Result<Option<User>, sqlx::Error> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name, email_verified \
         FROM users WHERE verification_token = ?",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn get_verification_token_expiry(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Option<chrono::NaiveDateTime>, sqlx::Error> {
    let row: Option<(Option<chrono::NaiveDateTime>,)> =
        sqlx::query_as("SELECT verification_token_expires_at FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    Ok(row.and_then(|r| r.0))
}

pub async fn verify_user_email(pool: &MySqlPool, user_id: u32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET email_verified = TRUE, verification_token = NULL, \
         verification_token_expires_at = NULL WHERE id = ?",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_verification_token(
    pool: &MySqlPool,
    user_id: u32,
    token: &str,
    expires_at: chrono::NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET verification_token = ?, verification_token_expires_at = ? WHERE id = ?",
    )
    .bind(token)
    .bind(expires_at)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn find_user_by_id(pool: &MySqlPool, user_id: u32) -> Result<Option<User>, sqlx::Error> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name, email_verified FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn seed_demo_user(pool: &MySqlPool) -> Result<(), anyhow::Error> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;

    if count.0 == 0 {
        let password_hash =
            crate::auth::password::hash_password("demo1234").map_err(|e| anyhow::anyhow!("{e}"))?;

        sqlx::query(
            "INSERT INTO users (email, password_hash, display_name, email_verified) \
             VALUES (?, ?, ?, TRUE)",
        )
        .bind("demo@lilly.app")
        .bind(&password_hash)
        .bind("Demo-Sammler")
        .execute(pool)
        .await?;

        tracing::info!("Demo user seeded: demo@lilly.app");
    }

    Ok(())
}
