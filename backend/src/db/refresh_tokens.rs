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
