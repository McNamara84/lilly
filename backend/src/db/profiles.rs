use sqlx::{MySqlConnection, MySqlPool};

use crate::db::media;
use crate::models::profile::{AvatarRow, OwnProfileRow, PublicProfileRow};

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

pub async fn update_profile(
    pool: &MySqlPool,
    user_id: u32,
    display_name: &str,
    location: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET display_name = ?, location = ? WHERE id = ?")
        .bind(display_name)
        .bind(location)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
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

pub async fn is_profile_and_collection_public(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<bool, sqlx::Error> {
    let visible = sqlx::query_scalar::<_, bool>(
        "SELECT TRUE FROM users \
         WHERE id = ? AND profile_public = TRUE AND collection_public = TRUE",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(visible.unwrap_or(false))
}

pub async fn replace_avatar(
    connection: &mut MySqlConnection,
    user_id: u32,
    new_storage_key: Option<&str>,
) -> Result<Option<Option<String>>, sqlx::Error> {
    let old_storage_key = sqlx::query_scalar::<_, Option<String>>(
        "SELECT avatar_path FROM users WHERE id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(old_storage_key) = old_storage_key else {
        return Ok(None);
    };

    if let Some(storage_key) = old_storage_key.as_deref()
        && Some(storage_key) != new_storage_key
    {
        media::enqueue_storage_key(connection, storage_key).await?;
    }
    sqlx::query("UPDATE users SET avatar_path = ? WHERE id = ?")
        .bind(new_storage_key)
        .bind(user_id)
        .execute(connection)
        .await?;

    Ok(Some(old_storage_key))
}

pub async fn find_avatar(pool: &MySqlPool, user_id: u32) -> Result<Option<AvatarRow>, sqlx::Error> {
    sqlx::query_as::<_, AvatarRow>(
        "SELECT id AS user_id, avatar_path AS storage_key, profile_public \
         FROM users WHERE id = ? AND avatar_path IS NOT NULL",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
