use serde_json::Value;
use sqlx::{MySqlConnection, MySqlPool};

use crate::models::notifications::{NotificationQueryParams, NotificationRow};

#[allow(clippy::too_many_arguments)]
pub async fn insert_notification(
    connection: &mut MySqlConnection,
    user_id: u32,
    actor_user_id: Option<u32>,
    kind: &str,
    match_id: Option<u32>,
    trade_id: Option<u32>,
    message_id: Option<u32>,
    dedupe_key: &str,
    payload: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT IGNORE INTO notifications
            (user_id, actor_user_id, kind, match_id, trade_id, message_id,
             dedupe_key, payload)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(actor_user_id)
    .bind(kind)
    .bind(match_id)
    .bind(trade_id)
    .bind(message_id)
    .bind(dedupe_key)
    .bind(payload)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn find_notifications(
    pool: &MySqlPool,
    user_id: u32,
    params: &NotificationQueryParams,
) -> Result<Vec<NotificationRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationRow>(
        "SELECT id, kind, actor_user_id, match_id, trade_id, message_id,
                payload, read_at, created_at
         FROM notifications
         WHERE user_id = ? AND (? = FALSE OR read_at IS NULL)
         ORDER BY created_at DESC, id DESC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(params.unread_only)
    .bind(params.pagination.per_page())
    .bind(params.pagination.offset())
    .fetch_all(pool)
    .await
}

pub async fn count_notifications(
    pool: &MySqlPool,
    user_id: u32,
    unread_only: bool,
) -> Result<u32, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM notifications
         WHERE user_id = ? AND (? = FALSE OR read_at IS NULL)",
    )
    .bind(user_id)
    .bind(unread_only)
    .fetch_one(pool)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

pub async fn mark_notification_read(
    pool: &MySqlPool,
    user_id: u32,
    notification_id: u32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE notifications SET read_at = CURRENT_TIMESTAMP
         WHERE id = ? AND user_id = ? AND read_at IS NULL",
    )
    .bind(notification_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    if result.rows_affected() > 0 {
        return Ok(true);
    }
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM notifications WHERE id = ? AND user_id = ?)",
    )
    .bind(notification_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn mark_all_notifications_read(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE notifications SET read_at = CURRENT_TIMESTAMP
         WHERE user_id = ? AND read_at IS NULL",
    )
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected())
}
