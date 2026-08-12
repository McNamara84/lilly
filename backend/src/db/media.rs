use std::collections::HashSet;

use sqlx::{MySqlConnection, MySqlPool};

use crate::models::media::{CollectionPhotoRow, MediaDeletionJob};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoUploadPreflight {
    Ready,
    NotFound,
    Full,
}

pub async fn photo_upload_preflight(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
    max_count: u8,
) -> Result<PhotoUploadPreflight, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT COUNT(cp.id) AS photo_count
           FROM collection_entries ce
           LEFT JOIN collection_photos cp ON cp.entry_id = ce.id
           WHERE ce.id = ? AND ce.user_id = ? AND ce.status <> 'wanted'
           GROUP BY ce.id"#,
        entry_id,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        None => PhotoUploadPreflight::NotFound,
        Some(row) if row.photo_count >= i64::from(max_count) => PhotoUploadPreflight::Full,
        Some(_) => PhotoUploadPreflight::Ready,
    })
}

pub async fn list_entry_photos_for_owner(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
) -> Result<Option<Vec<CollectionPhotoRow>>, sqlx::Error> {
    let owns_entry = sqlx::query_scalar::<_, bool>(
        "SELECT TRUE FROM collection_entries WHERE id = ? AND user_id = ?",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .unwrap_or(false);
    if !owns_entry {
        return Ok(None);
    }

    let photos = sqlx::query_as!(
        CollectionPhotoRow,
        r#"SELECT cp.id AS `id: u32`, ce.user_id AS `owner_user_id: u32`,
                  u.collection_public AS `collection_public: bool`,
                  cp.storage_key AS `storage_key: String`,
                  cp.media_type AS `media_type: String`, cp.byte_size AS `byte_size: u32`,
                  cp.width AS `width: u32`, cp.height AS `height: u32`,
                  cp.sort_order AS `sort_order: u8`,
                  cp.created_at AS `created_at: chrono::NaiveDateTime`
           FROM collection_photos cp
           JOIN collection_entries ce ON ce.id = cp.entry_id
           JOIN users u ON u.id = ce.user_id
           WHERE cp.entry_id = ? AND ce.user_id = ?
           ORDER BY cp.sort_order, cp.id"#,
        entry_id,
        user_id
    )
    .fetch_all(pool)
    .await?;
    Ok(Some(photos))
}

pub async fn find_photo(
    pool: &MySqlPool,
    photo_id: u32,
) -> Result<Option<CollectionPhotoRow>, sqlx::Error> {
    sqlx::query_as!(
        CollectionPhotoRow,
        r#"SELECT cp.id AS `id: u32`, ce.user_id AS `owner_user_id: u32`,
                  u.collection_public AS `collection_public: bool`,
                  cp.storage_key AS `storage_key: String`,
                  cp.media_type AS `media_type: String`, cp.byte_size AS `byte_size: u32`,
                  cp.width AS `width: u32`, cp.height AS `height: u32`,
                  cp.sort_order AS `sort_order: u8`,
                  cp.created_at AS `created_at: chrono::NaiveDateTime`
           FROM collection_photos cp
           JOIN collection_entries ce ON ce.id = cp.entry_id
           JOIN users u ON u.id = ce.user_id
           WHERE cp.id = ?"#,
        photo_id
    )
    .fetch_optional(pool)
    .await
}

pub async fn lock_uploadable_entry(
    connection: &mut MySqlConnection,
    entry_id: u32,
    user_id: u32,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, u32>(
        "SELECT id FROM collection_entries \
         WHERE id = ? AND user_id = ? AND status <> 'wanted' FOR UPDATE",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(connection)
    .await?
    .is_some())
}

pub async fn first_free_slot(
    connection: &mut MySqlConnection,
    entry_id: u32,
    max_count: u8,
) -> Result<Option<u8>, sqlx::Error> {
    let occupied = sqlx::query_scalar::<_, u8>(
        "SELECT sort_order FROM collection_photos WHERE entry_id = ? ORDER BY sort_order",
    )
    .bind(entry_id)
    .fetch_all(connection)
    .await?;
    Ok((0..max_count).find(|slot| !occupied.contains(slot)))
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_photo(
    connection: &mut MySqlConnection,
    entry_id: u32,
    storage_key: &str,
    media_type: &str,
    byte_size: u32,
    width: u32,
    height: u32,
    sort_order: u8,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO collection_photos \
         (entry_id, storage_key, media_type, byte_size, width, height, sort_order) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry_id)
    .bind(storage_key)
    .bind(media_type)
    .bind(byte_size)
    .bind(width)
    .bind(height)
    .bind(sort_order)
    .execute(&mut *connection)
    .await?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

pub async fn enqueue_and_delete_owned_photo(
    connection: &mut MySqlConnection,
    entry_id: u32,
    photo_id: u32,
    user_id: u32,
) -> Result<Option<String>, sqlx::Error> {
    let storage_key = sqlx::query_scalar::<_, String>(
        "SELECT cp.storage_key \
         FROM collection_photos cp \
         JOIN collection_entries ce ON ce.id = cp.entry_id \
         WHERE cp.id = ? AND cp.entry_id = ? AND ce.user_id = ? FOR UPDATE",
    )
    .bind(photo_id)
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(storage_key) = storage_key else {
        return Ok(None);
    };

    enqueue_storage_key(connection, &storage_key).await?;
    sqlx::query("DELETE FROM collection_photos WHERE id = ?")
        .bind(photo_id)
        .execute(connection)
        .await?;
    Ok(Some(storage_key))
}

pub async fn enqueue_entry_photo_deletions(
    connection: &mut MySqlConnection,
    entry_id: u32,
    user_id: u32,
) -> Result<Vec<String>, sqlx::Error> {
    let storage_keys = sqlx::query_scalar::<_, String>(
        "SELECT cp.storage_key FROM collection_photos cp \
         JOIN collection_entries ce ON ce.id = cp.entry_id \
         WHERE cp.entry_id = ? AND ce.user_id = ? FOR UPDATE",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_all(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO media_deletion_jobs (storage_key) \
         SELECT cp.storage_key FROM collection_photos cp \
         JOIN collection_entries ce ON ce.id = cp.entry_id \
         WHERE cp.entry_id = ? AND ce.user_id = ? \
         ON DUPLICATE KEY UPDATE processed_at = NULL, last_error = NULL",
    )
    .bind(entry_id)
    .bind(user_id)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "DELETE cp FROM collection_photos cp \
         JOIN collection_entries ce ON ce.id = cp.entry_id \
         WHERE cp.entry_id = ? AND ce.user_id = ?",
    )
    .bind(entry_id)
    .bind(user_id)
    .execute(&mut *connection)
    .await?;
    Ok(storage_keys)
}

async fn enqueue_storage_key(
    connection: &mut MySqlConnection,
    storage_key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO media_deletion_jobs (storage_key) VALUES (?) \
         ON DUPLICATE KEY UPDATE processed_at = NULL, last_error = NULL",
    )
    .bind(storage_key)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn pending_deletion_jobs(
    pool: &MySqlPool,
    limit: u32,
) -> Result<Vec<MediaDeletionJob>, sqlx::Error> {
    sqlx::query_as::<_, MediaDeletionJob>(
        "SELECT id, storage_key FROM media_deletion_jobs \
         WHERE processed_at IS NULL ORDER BY id LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn mark_deletion_processed(pool: &MySqlPool, job_id: u64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE media_deletion_jobs SET processed_at = CURRENT_TIMESTAMP, \
         attempts = attempts + 1, last_error = NULL WHERE id = ?",
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_deletion_processed_by_key(
    pool: &MySqlPool,
    storage_key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE media_deletion_jobs SET processed_at = CURRENT_TIMESTAMP, \
         attempts = attempts + 1, last_error = NULL \
         WHERE storage_key = ? AND processed_at IS NULL",
    )
    .bind(storage_key)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_deletion_failed_by_key(
    pool: &MySqlPool,
    storage_key: &str,
    message: &str,
) -> Result<(), sqlx::Error> {
    let message = message.chars().take(500).collect::<String>();
    sqlx::query(
        "UPDATE media_deletion_jobs SET attempts = attempts + 1, last_error = ? \
         WHERE storage_key = ? AND processed_at IS NULL",
    )
    .bind(message)
    .bind(storage_key)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_deletion_failed(
    pool: &MySqlPool,
    job_id: u64,
    message: &str,
) -> Result<(), sqlx::Error> {
    let message = message.chars().take(500).collect::<String>();
    sqlx::query(
        "UPDATE media_deletion_jobs SET attempts = attempts + 1, last_error = ? WHERE id = ?",
    )
    .bind(message)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn active_storage_keys(pool: &MySqlPool) -> Result<HashSet<String>, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT storage_key FROM collection_photos")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect(),
    )
}
