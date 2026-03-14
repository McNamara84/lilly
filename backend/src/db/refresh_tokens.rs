use sqlx::MySqlPool;

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct RefreshTokenRow {
    pub id: u32,
    pub user_id: u32,
    pub token_hash: String,
    pub expires_at: chrono::NaiveDateTime,
    pub revoked: bool,
}

pub async fn store_refresh_token(
    pool: &MySqlPool,
    user_id: u32,
    token_hash: &str,
    expires_at: chrono::NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn find_valid_refresh_token(
    pool: &MySqlPool,
    token_hash: &str,
) -> Result<Option<RefreshTokenRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, RefreshTokenRow>(
        "SELECT id, user_id, token_hash, expires_at, revoked \
         FROM refresh_tokens \
         WHERE token_hash = ? AND revoked = FALSE AND expires_at > NOW()",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn revoke_refresh_token(pool: &MySqlPool, token_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE token_hash = ?")
        .bind(token_hash)
        .execute(pool)
        .await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn revoke_all_user_refresh_tokens(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = ? AND revoked = FALSE")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Atomically revoke an old refresh token and store a new one.
/// Returns an error if the old token was already revoked (race condition guard).
pub async fn rotate_refresh_token(
    pool: &MySqlPool,
    old_token_hash: &str,
    new_user_id: u32,
    new_token_hash: &str,
    new_expires_at: chrono::NaiveDateTime,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    let result = sqlx::query(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE token_hash = ? AND revoked = FALSE",
    )
    .bind(old_token_hash)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() != 1 {
        tx.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }

    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES (?, ?, ?)")
        .bind(new_user_id)
        .bind(new_token_hash)
        .bind(new_expires_at)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
