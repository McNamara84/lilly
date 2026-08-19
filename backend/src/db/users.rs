use sqlx::MySqlPool;

use crate::models::user::User;

#[derive(Debug, sqlx::FromRow)]
pub struct AuthStateRow {
    pub account_state: String,
    pub session_version: u32,
}

pub async fn find_auth_state(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Option<AuthStateRow>, sqlx::Error> {
    sqlx::query_as("SELECT account_state, session_version FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn find_user_by_email(
    pool: &MySqlPool,
    email: &str,
) -> Result<Option<User>, sqlx::Error> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name, role, email_verified, \
                account_state, session_version, erasure_subject \
         FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_user(
    pool: &MySqlPool,
    email: &str,
    password_hash: &str,
    display_name: &str,
    verification_token: &str,
    verification_expires_at: chrono::NaiveDateTime,
    privacy_consent_at: chrono::NaiveDateTime,
    privacy_policy_version: &str,
) -> Result<u32, sqlx::Error> {
    let mut transaction = pool.begin().await?;
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
    .execute(&mut *transaction)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    let user_id = result.last_insert_id() as u32;
    crate::db::privacy_consents::insert_on_transaction(
        &mut transaction,
        user_id,
        privacy_policy_version,
        privacy_consent_at,
        "password",
    )
    .await?;
    transaction.commit().await?;
    Ok(user_id)
}

pub async fn find_user_by_verification_token(
    pool: &MySqlPool,
    token: &str,
) -> Result<Option<User>, sqlx::Error> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name, role, email_verified, \
                account_state, session_version, erasure_subject \
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
        "SELECT id, email, password_hash, display_name, role, email_verified, \
                account_state, session_version, erasure_subject \
         FROM users WHERE id = ?",
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
    use super::{create_user, e2e_worker_email};
    use sqlx::mysql::MySqlPoolOptions;

    #[test]
    fn test_e2e_worker_email_is_deterministic() {
        assert_eq!(e2e_worker_email(0), "e2e-worker-0@lilly.app");
        assert_eq!(e2e_worker_email(12), "e2e-worker-12@lilly.app");
    }

    #[tokio::test]
    async fn password_user_and_versioned_consent_are_created_atomically() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let email = format!("password-consent-{suffix}@example.test");
        let now = chrono::Utc::now().naive_utc();

        let user_id = create_user(
            &pool,
            &email,
            "argon2-test-hash",
            "Password Collector",
            &format!("verification-token-hash-{suffix}"),
            now + chrono::Duration::hours(24),
            now,
            "policy-password-v1",
        )
        .await
        .unwrap();
        let consent: (String, String) = sqlx::query_as(
            "SELECT policy_version, registration_method FROM privacy_consents WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            consent,
            ("policy-password-v1".to_string(), "password".to_string())
        );

        assert!(
            create_user(
                &pool,
                &email,
                "different-hash",
                "Duplicate Collector",
                "second-token",
                now + chrono::Duration::hours(24),
                now,
                "policy-password-v2",
            )
            .await
            .is_err()
        );
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), (SELECT COUNT(*) FROM privacy_consents WHERE user_id = ?) \
             FROM users WHERE email = ?",
        )
        .bind(user_id)
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 1));

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
