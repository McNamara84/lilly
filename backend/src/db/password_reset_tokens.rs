use chrono::NaiveDateTime;
use sqlx::MySqlPool;

#[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
pub struct PasswordResetTarget {
    pub user_id: u32,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumePasswordResetResult {
    Updated,
    Invalid,
}

pub async fn replace_active_token(
    pool: &MySqlPool,
    user_id: u32,
    token_hash: &str,
    created_at: NaiveDateTime,
    expires_at: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    // Serialize replacement requests per account. Without locking the parent
    // row, two transactions could both invalidate the old set before either
    // inserts its new token, leaving two active tokens behind.
    sqlx::query("SELECT id FROM users WHERE id = ? FOR UPDATE")
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await?;
    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = ? \
         WHERE user_id = ? AND used_at IS NULL",
    )
    .bind(created_at)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO password_reset_tokens \
         (user_id, token_hash, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(created_at)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await
}

pub async fn find_valid_target(
    pool: &MySqlPool,
    token_hash: &str,
    now: NaiveDateTime,
) -> Result<Option<PasswordResetTarget>, sqlx::Error> {
    sqlx::query_as::<_, PasswordResetTarget>(
        "SELECT users.id AS user_id, users.email, users.display_name \
         FROM password_reset_tokens \
         INNER JOIN users ON users.id = password_reset_tokens.user_id \
         WHERE password_reset_tokens.token_hash = ? \
           AND password_reset_tokens.used_at IS NULL \
           AND password_reset_tokens.expires_at > ? \
           AND users.email_verified = TRUE \
           AND users.password_hash IS NOT NULL",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await
}

pub async fn consume_and_update_password(
    pool: &MySqlPool,
    token_hash: &str,
    password_hash: &str,
    now: NaiveDateTime,
) -> Result<ConsumePasswordResetResult, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let target = sqlx::query_as::<_, (u32,)>(
        "SELECT user_id FROM password_reset_tokens \
         WHERE token_hash = ? AND used_at IS NULL AND expires_at > ? \
         FOR UPDATE",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((user_id,)) = target else {
        transaction.rollback().await?;
        return Ok(ConsumePasswordResetResult::Invalid);
    };

    let updated = sqlx::query(
        "UPDATE users SET password_hash = ? WHERE id = ? \
         AND email_verified = TRUE AND password_hash IS NOT NULL",
    )
    .bind(password_hash)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() != 1 {
        transaction.rollback().await?;
        return Ok(ConsumePasswordResetResult::Invalid);
    }

    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = ? \
         WHERE user_id = ? AND used_at IS NULL",
    )
    .bind(now)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE refresh_tokens SET revoked = TRUE \
         WHERE user_id = ? AND revoked = FALSE",
    )
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(ConsumePasswordResetResult::Updated)
}

pub async fn delete_expired(pool: &MySqlPool, now: NaiveDateTime) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM password_reset_tokens \
         WHERE expires_at <= ? OR used_at IS NOT NULL",
    )
    .bind(now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::mysql::MySqlPoolOptions;

    async fn test_pool() -> Option<MySqlPool> {
        let database_url = std::env::var("DATABASE_URL").ok()?;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .ok()?;
        crate::db::migrate_test_database(&pool).await.ok()?;
        Some(pool)
    }

    async fn create_user(pool: &MySqlPool, suffix: &str) -> u32 {
        let result = sqlx::query(
            "INSERT INTO users (email, password_hash, display_name, email_verified) \
             VALUES (?, 'old-password-hash', 'Reset Collector', TRUE)",
        )
        .bind(format!("reset-{suffix}@example.test"))
        .execute(pool)
        .await
        .unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let user_id = result.last_insert_id() as u32;
        user_id
    }

    fn unique_token_hash(label: &str, suffix: &str) -> String {
        crate::auth::oauth::hash_secret(&format!("{label}-{suffix}"))
    }

    #[tokio::test]
    async fn replacing_a_token_invalidates_the_previous_token() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = crate::auth::oauth::random_urlsafe_token();
        let user_id = create_user(&pool, &suffix).await;
        let now = chrono::Utc::now().naive_utc();
        let first_hash = unique_token_hash("first", &suffix);
        let second_hash = unique_token_hash("second", &suffix);

        replace_active_token(
            &pool,
            user_id,
            &first_hash,
            now,
            now + chrono::Duration::hours(1),
        )
        .await
        .unwrap();
        replace_active_token(
            &pool,
            user_id,
            &second_hash,
            now + chrono::Duration::seconds(1),
            now + chrono::Duration::hours(1),
        )
        .await
        .unwrap();

        assert!(
            find_valid_target(&pool, &first_hash, now)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            find_valid_target(&pool, &second_hash, now)
                .await
                .unwrap()
                .unwrap()
                .user_id,
            user_id
        );

        let third_hash = unique_token_hash("third", &suffix);
        let fourth_hash = unique_token_hash("fourth", &suffix);
        let (third, fourth) = tokio::join!(
            replace_active_token(
                &pool,
                user_id,
                &third_hash,
                now + chrono::Duration::seconds(2),
                now + chrono::Duration::hours(1),
            ),
            replace_active_token(
                &pool,
                user_id,
                &fourth_hash,
                now + chrono::Duration::seconds(2),
                now + chrono::Duration::hours(1),
            )
        );
        third.unwrap();
        fourth.unwrap();
        let active_tokens: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM password_reset_tokens \
             WHERE user_id = ? AND used_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(active_tokens.0, 1);

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn concurrent_consumption_updates_password_and_revokes_sessions_once() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = crate::auth::oauth::random_urlsafe_token();
        let user_id = create_user(&pool, &suffix).await;
        let now = chrono::Utc::now().naive_utc();
        let token_hash = unique_token_hash("consume", &suffix);
        replace_active_token(
            &pool,
            user_id,
            &token_hash,
            now,
            now + chrono::Duration::hours(1),
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO refresh_tokens (user_id, token_hash, expires_at, authenticated_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(format!("refresh-{suffix}"))
        .bind(now + chrono::Duration::hours(1))
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let (first, second) = tokio::join!(
            consume_and_update_password(&pool, &token_hash, "new-password-hash", now),
            consume_and_update_password(&pool, &token_hash, "new-password-hash", now)
        );
        let results = [first.unwrap(), second.unwrap()];
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == ConsumePasswordResetResult::Updated)
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == ConsumePasswordResetResult::Invalid)
                .count(),
            1
        );
        let row: (String, bool) = sqlx::query_as(
            "SELECT users.password_hash, refresh_tokens.revoked FROM users \
             INNER JOIN refresh_tokens ON refresh_tokens.user_id = users.id \
             WHERE users.id = ?",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row, ("new-password-hash".to_string(), true));
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn database_failure_rolls_back_password_token_and_session_changes() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = crate::auth::oauth::random_urlsafe_token();
        let user_id = create_user(&pool, &suffix).await;
        let now = chrono::Utc::now().naive_utc();
        let token_hash = unique_token_hash("rollback", &suffix);
        replace_active_token(
            &pool,
            user_id,
            &token_hash,
            now,
            now + chrono::Duration::hours(1),
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO refresh_tokens (user_id, token_hash, expires_at, authenticated_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(format!("rollback-refresh-{suffix}"))
        .bind(now + chrono::Duration::hours(1))
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let oversized_hash = "x".repeat(300);
        assert!(
            consume_and_update_password(&pool, &token_hash, &oversized_hash, now)
                .await
                .is_err()
        );
        let state: (String, bool, Option<NaiveDateTime>) = sqlx::query_as(
            "SELECT users.password_hash, refresh_tokens.revoked, password_reset_tokens.used_at \
             FROM users \
             INNER JOIN refresh_tokens ON refresh_tokens.user_id = users.id \
             INNER JOIN password_reset_tokens ON password_reset_tokens.user_id = users.id \
             WHERE users.id = ? AND password_reset_tokens.token_hash = ?",
        )
        .bind(user_id)
        .bind(&token_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, ("old-password-hash".to_string(), false, None));

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_and_oauth_only_tokens_are_invalid() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = crate::auth::oauth::random_urlsafe_token();
        let user_id = create_user(&pool, &suffix).await;
        let now = chrono::Utc::now().naive_utc();
        let token_hash = unique_token_hash("expired", &suffix);
        replace_active_token(
            &pool,
            user_id,
            &token_hash,
            now - chrono::Duration::hours(2),
            now - chrono::Duration::hours(1),
        )
        .await
        .unwrap();

        assert!(
            find_valid_target(&pool, &token_hash, now)
                .await
                .unwrap()
                .is_none()
        );
        sqlx::query("UPDATE users SET password_hash = NULL WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            consume_and_update_password(&pool, &token_hash, "new-hash", now)
                .await
                .unwrap(),
            ConsumePasswordResetResult::Invalid
        );
        assert!(delete_expired(&pool, now).await.unwrap() >= 1);
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
