use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use super::trade_matching::{PageParams, TradePartnerResponse};
use crate::models::profile::avatar_content_url;

pub const MAX_MESSAGE_LENGTH: usize = 4_000;

#[derive(Debug, Deserialize)]
pub struct MessagePageParams {
    pub before_id: Option<u32>,
    #[serde(default = "default_message_limit")]
    pub limit: u32,
}

const fn default_message_limit() -> u32 {
    50
}

impl MessagePageParams {
    pub fn limit(&self) -> u32 {
        self.limit.clamp(1, 100)
    }
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub client_message_id: String,
    pub content: String,
}

impl SendMessageRequest {
    pub fn validate(&self) -> Result<&str, String> {
        if !is_uuid_shape(&self.client_message_id) {
            return Err("client_message_id must be a UUID".to_string());
        }
        let content = self.content.trim();
        if content.is_empty() {
            return Err("content must not be empty".to_string());
        }
        if content.chars().count() > MAX_MESSAGE_LENGTH {
            return Err(format!(
                "content must not exceed {MAX_MESSAGE_LENGTH} characters"
            ));
        }
        Ok(content)
    }
}

fn is_uuid_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

#[derive(Debug, Deserialize)]
pub struct MarkThreadReadRequest {
    pub through_message_id: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct MessageRecord {
    pub id: u32,
    pub thread_id: u32,
    pub sender_id: u32,
    pub client_message_id: String,
    pub content: String,
    pub created_at: NaiveDateTime,
    pub read_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    pub id: u32,
    pub thread_id: u32,
    pub sender_id: u32,
    pub content: String,
    pub created_at: NaiveDateTime,
    pub read_at: Option<NaiveDateTime>,
    pub is_mine: bool,
}

impl MessageResponse {
    pub fn from_record(record: MessageRecord, user_id: u32) -> Self {
        Self {
            id: record.id,
            thread_id: record.thread_id,
            sender_id: record.sender_id,
            content: record.content,
            created_at: record.created_at,
            read_at: record.read_at,
            is_mine: record.sender_id == user_id,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MessageListResponse {
    pub data: Vec<MessageResponse>,
    pub next_before_id: Option<u32>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThreadListRow {
    pub thread_id: u32,
    pub trade_id: u32,
    pub trade_status: String,
    pub partner_id: u32,
    pub partner_display_name: String,
    pub partner_profile_public: bool,
    pub partner_avatar_path: Option<String>,
    pub partner_location: Option<String>,
    pub last_message: Option<String>,
    pub last_message_at: Option<NaiveDateTime>,
    pub unread_count: i64,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadSummaryResponse {
    pub id: u32,
    pub trade_id: u32,
    pub trade_status: String,
    pub partner: TradePartnerResponse,
    pub last_message: Option<String>,
    pub last_message_at: Option<NaiveDateTime>,
    pub unread_count: u32,
    pub updated_at: NaiveDateTime,
}

impl From<&ThreadListRow> for ThreadSummaryResponse {
    fn from(row: &ThreadListRow) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Self {
            id: row.thread_id,
            trade_id: row.trade_id,
            trade_status: row.trade_status.clone(),
            partner: TradePartnerResponse {
                id: row.partner_id,
                display_name: row.partner_display_name.clone(),
                avatar_path: row
                    .partner_profile_public
                    .then(|| avatar_content_url(row.partner_id, row.partner_avatar_path.is_some()))
                    .flatten(),
                location: row
                    .partner_profile_public
                    .then(|| row.partner_location.clone())
                    .flatten(),
            },
            last_message: row.last_message.clone(),
            last_message_at: row.last_message_at,
            unread_count: row.unread_count.max(0) as u32,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedThreadsResponse {
    pub data: Vec<ThreadSummaryResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

pub type ThreadPageParams = PageParams;

#[cfg(test)]
mod tests {
    use super::*;

    fn request(content: &str) -> SendMessageRequest {
        SendMessageRequest {
            client_message_id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn message_validation_accepts_unicode_and_trims_edges() {
        assert_eq!(
            request("  Hallo\nWelt 👋  ").validate().unwrap(),
            "Hallo\nWelt 👋"
        );
    }

    #[test]
    fn message_validation_rejects_empty_overlong_and_bad_uuid() {
        assert!(request("  \n ").validate().is_err());
        assert!(
            request(&"ä".repeat(MAX_MESSAGE_LENGTH + 1))
                .validate()
                .is_err()
        );
        let mut invalid = request("ok");
        invalid.client_message_id = "not-a-uuid".to_string();
        assert!(invalid.validate().is_err());
    }
}
