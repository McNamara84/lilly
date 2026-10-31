use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::trade_matching::PageParams;

#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
#[schema(as = notifications::NotificationQueryParams)]
pub struct NotificationQueryParams {
    #[serde(flatten)]
    pub pagination: PageParams,
    #[serde(default)]
    pub unread_only: bool,
}

// IntoParams does not expand serde-flattened query structs. Describe the actual
// flat query string instead of inventing a nested `pagination` parameter.
impl utoipa::IntoParams for NotificationQueryParams {
    fn into_params(
        parameter_in_provider: impl Fn() -> Option<utoipa::openapi::path::ParameterIn>,
    ) -> Vec<utoipa::openapi::path::Parameter> {
        use utoipa::PartialSchema;
        use utoipa::openapi::path::{ParameterBuilder, ParameterIn};
        let mut parameters = PageParams::into_params(parameter_in_provider);
        parameters.push(
            ParameterBuilder::new()
                .name("unread_only")
                .parameter_in(ParameterIn::Query)
                .required(utoipa::openapi::Required::False)
                .schema(Some(bool::schema()))
                .description(Some("Only unread notifications; defaults to false"))
                .build(),
        );
        parameters
    }
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

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[schema(as = notifications::NotificationResponse)]
pub struct NotificationResponse {
    pub id: u32,
    pub kind: String,
    pub actor_user_id: Option<u32>,
    pub match_id: Option<u32>,
    pub trade_id: Option<u32>,
    pub message_id: Option<u32>,
    pub payload: Value,
    #[schema(format = "local-date-time")]
    pub read_at: Option<NaiveDateTime>,
    #[schema(format = "local-date-time")]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = notifications::PaginatedNotificationsResponse)]
pub struct PaginatedNotificationsResponse {
    pub data: Vec<NotificationResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = notifications::UnreadCountResponse)]
pub struct UnreadCountResponse {
    pub unread_count: u32,
}
