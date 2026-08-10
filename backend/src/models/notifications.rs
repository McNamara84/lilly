use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::trade_matching::PageParams;

#[derive(Debug, Deserialize, Default)]
pub struct NotificationQueryParams {
    #[serde(flatten)]
    pub pagination: PageParams,
    #[serde(default)]
    pub unread_only: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationRow {
    pub id: u32,
    pub kind: String,
    pub actor_user_id: Option<u32>,
    pub match_id: Option<u32>,
    pub trade_id: Option<u32>,
    pub message_id: Option<u32>,
    pub payload: Value,
    pub read_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationResponse {
    pub id: u32,
    pub kind: String,
    pub actor_user_id: Option<u32>,
    pub match_id: Option<u32>,
    pub trade_id: Option<u32>,
    pub message_id: Option<u32>,
    pub payload: Value,
    pub read_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

impl From<NotificationRow> for NotificationResponse {
    fn from(row: NotificationRow) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            actor_user_id: row.actor_user_id,
            match_id: row.match_id,
            trade_id: row.trade_id,
            message_id: row.message_id,
            payload: row.payload,
            read_at: row.read_at,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedNotificationsResponse {
    pub data: Vec<NotificationResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct UnreadCountResponse {
    pub unread_count: u32,
}
