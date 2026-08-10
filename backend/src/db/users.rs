use sqlx::MySqlPool;

use crate::models::user::User;

pub async fn find_user_by_email(
    pool: &MySqlPool,
    email: &str,
) -> Result<Option<User>, sqlx::Error> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name, role, email_verified FROM users WHERE email = ?",
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
        "SELECT id, email, password_hash, display_name, role, email_verified \
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
        "SELECT id, email, password_hash, display_name, role, email_verified FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn seed_demo_user(pool: &MySqlPool) -> Result<u32, anyhow::Error> {
    let password_hash =
        crate::auth::password::hash_password("demo1234").map_err(|e| anyhow::anyhow!("{e}"))?;

    sqlx::query(
        "INSERT INTO users (email, password_hash, display_name, role, email_verified) \
         VALUES (?, ?, ?, 'admin', TRUE) \
         ON DUPLICATE KEY UPDATE password_hash = VALUES(password_hash), \
         display_name = VALUES(display_name), role = 'admin', email_verified = TRUE",
    )
    .bind("demo@lilly.app")
    .bind(&password_hash)
    .bind("Demo-Sammler")
    .execute(pool)
    .await?;

    let (user_id,): (u32,) = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind("demo@lilly.app")
        .fetch_one(pool)
        .await?;

    tracing::info!("Demo user seeded: demo@lilly.app");
    Ok(user_id)
}

#[must_use]
pub fn e2e_worker_email(index: u16) -> String {
    format!("e2e-worker-{index}@lilly.app")
}

pub async fn seed_e2e_worker_users(
    pool: &MySqlPool,
    count: u16,
) -> Result<Vec<u32>, anyhow::Error> {
    if count == 0 {
        return Ok(Vec::new());
    }

    let password_hash = crate::auth::password::hash_password("e2e-worker-password")
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let mut user_ids = Vec::with_capacity(usize::from(count));

    for index in 0..count {
        let email = e2e_worker_email(index);
        let display_name = format!("E2E Worker {index}");
        sqlx::query(
            "INSERT INTO users (email, password_hash, display_name, role, email_verified) \
             VALUES (?, ?, ?, 'admin', TRUE) \
             ON DUPLICATE KEY UPDATE password_hash = VALUES(password_hash), \
             display_name = VALUES(display_name), role = 'admin', email_verified = TRUE, \
             profile_public = FALSE, collection_public = FALSE",
        )
        .bind(&email)
        .bind(&password_hash)
        .bind(&display_name)
        .execute(pool)
        .await?;

        let (user_id,): (u32,) = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(&email)
            .fetch_one(pool)
            .await?;
        user_ids.push(user_id);
    }

    tracing::info!(count, "E2E worker users seeded");
    Ok(user_ids)
}

#[cfg(test)]
mod tests {
    use super::e2e_worker_email;

    #[test]
    fn test_e2e_worker_email_is_deterministic() {
        assert_eq!(e2e_worker_email(0), "e2e-worker-0@lilly.app");
        assert_eq!(e2e_worker_email(12), "e2e-worker-12@lilly.app");
    }
}
