use sqlx::{MySql, MySqlConnection, MySqlPool, Transaction};

use crate::models::messaging::{MessagePageParams, MessageRecord, ThreadListRow, ThreadPageParams};

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct ThreadAccessRow {
    pub thread_id: u32,
    pub trade_id: u32,
    pub trade_status: String,
    pub initiator_id: u32,
    pub responder_id: u32,
    pub partner_display_name: String,
}

impl ThreadAccessRow {
    pub fn recipient_id(&self, sender_id: u32) -> u32 {
        if self.initiator_id == sender_id {
            self.responder_id
        } else {
            self.initiator_id
        }
    }
}

pub async fn count_threads(pool: &MySqlPool, user_id: u32) -> Result<u32, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM message_threads mt
         JOIN trades t ON t.id = mt.trade_id
         WHERE t.initiator_id = ? OR t.responder_id = ?",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

pub async fn find_threads(
    pool: &MySqlPool,
    user_id: u32,
    params: &ThreadPageParams,
) -> Result<Vec<ThreadListRow>, sqlx::Error> {
    sqlx::query_as::<_, ThreadListRow>(
        "SELECT mt.id AS thread_id, t.id AS trade_id, t.status AS trade_status,
                partner.id AS partner_id, partner.display_name AS partner_display_name,
                partner.profile_public AS partner_profile_public,
                partner.avatar_path AS partner_avatar_path,
                partner.location AS partner_location,
                (SELECT m.content FROM messages m WHERE m.thread_id = mt.id
                 ORDER BY m.id DESC LIMIT 1) AS last_message,
                (SELECT m.created_at FROM messages m WHERE m.thread_id = mt.id
                 ORDER BY m.id DESC LIMIT 1) AS last_message_at,
                (SELECT COUNT(*) FROM messages unread
                 WHERE unread.thread_id = mt.id AND unread.sender_id <> ?
                   AND unread.read_at IS NULL) AS unread_count,
                mt.updated_at
         FROM message_threads mt
         JOIN trades t ON t.id = mt.trade_id
         JOIN users partner ON partner.id = CASE
             WHEN t.initiator_id = ? THEN t.responder_id ELSE t.initiator_id END
         WHERE t.initiator_id = ? OR t.responder_id = ?
         ORDER BY COALESCE(last_message_at, mt.created_at) DESC, mt.id DESC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(params.per_page())
    .bind(params.offset())
    .fetch_all(pool)
    .await
}

pub async fn find_thread_access(
    pool: &MySqlPool,
    thread_id: u32,
    user_id: u32,
) -> Result<Option<ThreadAccessRow>, sqlx::Error> {
    sqlx::query_as::<_, ThreadAccessRow>(
        "SELECT mt.id AS thread_id, t.id AS trade_id, t.status AS trade_status,
                t.initiator_id, t.responder_id,
                partner.display_name AS partner_display_name
         FROM message_threads mt
         JOIN trades t ON t.id = mt.trade_id
         JOIN users partner ON partner.id = CASE
             WHEN t.initiator_id = ? THEN t.responder_id ELSE t.initiator_id END
         WHERE mt.id = ? AND (t.initiator_id = ? OR t.responder_id = ?)",
    )
    .bind(user_id)
    .bind(thread_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn lock_thread_access(
    transaction: &mut Transaction<'_, MySql>,
    thread_id: u32,
    user_id: u32,
) -> Result<Option<ThreadAccessRow>, sqlx::Error> {
    sqlx::query_as::<_, ThreadAccessRow>(
        "SELECT mt.id AS thread_id, t.id AS trade_id, t.status AS trade_status,
                t.initiator_id, t.responder_id,
                partner.display_name AS partner_display_name
         FROM message_threads mt
         JOIN trades t ON t.id = mt.trade_id
         JOIN users partner ON partner.id = CASE
             WHEN t.initiator_id = ? THEN t.responder_id ELSE t.initiator_id END
         WHERE mt.id = ? AND (t.initiator_id = ? OR t.responder_id = ?)
         FOR UPDATE",
    )
    .bind(user_id)
    .bind(thread_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn find_messages(
    pool: &MySqlPool,
    thread_id: u32,
    params: &MessagePageParams,
) -> Result<Vec<MessageRecord>, sqlx::Error> {
    sqlx::query_as::<_, MessageRecord>(
        "SELECT id, thread_id, sender_id, client_message_id, content,
                created_at, read_at
         FROM messages
         WHERE thread_id = ? AND (? IS NULL OR id < ?)
         ORDER BY id DESC LIMIT ?",
    )
    .bind(thread_id)
    .bind(params.before_id)
    .bind(params.before_id)
    .bind(params.limit())
    .fetch_all(pool)
    .await
}

pub async fn insert_message(
    connection: &mut MySqlConnection,
    thread_id: u32,
    sender_id: u32,
    client_message_id: &str,
    content: &str,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO messages (thread_id, sender_id, client_message_id, content)
         VALUES (?, ?, ?, ?)",
    )
    .bind(thread_id)
    .bind(sender_id)
    .bind(client_message_id)
    .bind(content)
    .execute(connection)
    .await?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

pub async fn find_message_by_client_id(
    connection: &mut MySqlConnection,
    thread_id: u32,
    sender_id: u32,
    client_message_id: &str,
) -> Result<Option<MessageRecord>, sqlx::Error> {
    sqlx::query_as::<_, MessageRecord>(
        "SELECT id, thread_id, sender_id, client_message_id, content,
                created_at, read_at
         FROM messages
         WHERE thread_id = ? AND sender_id = ? AND client_message_id = ?",
    )
    .bind(thread_id)
    .bind(sender_id)
    .bind(client_message_id)
    .fetch_optional(connection)
    .await
}

pub async fn find_message_by_id(
    connection: &mut MySqlConnection,
    message_id: u32,
) -> Result<MessageRecord, sqlx::Error> {
    sqlx::query_as::<_, MessageRecord>(
        "SELECT id, thread_id, sender_id, client_message_id, content,
                created_at, read_at FROM messages WHERE id = ?",
    )
    .bind(message_id)
    .fetch_one(connection)
    .await
}

pub async fn touch_thread(
    connection: &mut MySqlConnection,
    thread_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE message_threads SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(thread_id)
        .execute(connection)
        .await?;
    Ok(())
}

pub async fn mark_thread_read(
    transaction: &mut Transaction<'_, MySql>,
    thread_id: u32,
    user_id: u32,
    through_message_id: u32,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE messages
         SET read_at = CURRENT_TIMESTAMP
         WHERE thread_id = ? AND id <= ? AND sender_id <> ? AND read_at IS NULL",
    )
    .bind(thread_id)
    .bind(through_message_id)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "UPDATE notifications n
         JOIN messages m ON m.id = n.message_id
         SET n.read_at = COALESCE(n.read_at, CURRENT_TIMESTAMP)
         WHERE n.user_id = ? AND n.kind = 'trade_message'
           AND m.thread_id = ? AND m.id <= ?",
    )
    .bind(user_id)
    .bind(thread_id)
    .bind(through_message_id)
    .execute(&mut **transaction)
    .await?;
    Ok(result.rows_affected())
}
