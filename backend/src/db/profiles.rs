use sqlx::MySqlPool;

use crate::models::profile::{OwnProfileRow, PublicProfileRow};

pub async fn find_own_profile(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Option<OwnProfileRow>, sqlx::Error> {
    sqlx::query_as::<_, OwnProfileRow>(
        "SELECT id, email, display_name, avatar_path, location, profile_public, \
         collection_public, created_at FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_visibility(
    pool: &MySqlPool,
    user_id: u32,
    profile_public: bool,
    collection_public: bool,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE users SET profile_public = ?, collection_public = ? WHERE id = ?")
            .bind(profile_public)
            .bind(collection_public)
            .bind(user_id)
            .execute(pool)
            .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn find_public_profile(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Option<PublicProfileRow>, sqlx::Error> {
    sqlx::query_as::<_, PublicProfileRow>(
        "SELECT id, display_name, avatar_path, location, created_at \
         FROM users WHERE id = ? AND profile_public = TRUE",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn is_collection_public(pool: &MySqlPool, user_id: u32) -> Result<bool, sqlx::Error> {
    let visible = sqlx::query_scalar::<_, bool>(
        "SELECT collection_public FROM users WHERE id = ? AND collection_public = TRUE",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(visible.unwrap_or(false))
}
