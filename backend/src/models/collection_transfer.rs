use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u8 = 1;
pub const FORMAT_NAME: &str = "lilly.collection";
pub const DEFAULT_MAX_BYTES: usize = 10 * 1024 * 1024;
pub const DEFAULT_MAX_ROWS: usize = 10_000;
pub const PREVIEW_SECONDS: i64 = 30 * 60;
pub const EXPORT_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = collection_transfer::TransferEntry)]
pub struct TransferEntry {
    pub copy_key: String,
    pub series_slug: String,
    pub issue_number: u32,
    pub copy_number: u8,
    pub status: String,
    #[serde(default)]
    pub source_key: Option<String>,
    #[serde(default)]
    pub source_record_id: Option<String>,
    #[serde(default)]
    pub condition_grade: Option<String>,
    #[serde(default)]
    pub edition_label: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub series_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = collection_transfer::CollectionDocument)]
pub struct CollectionDocument {
    pub format: String,
    pub format_version: u8,
    pub exported_at: Option<String>,
    #[schema(value_type = Vec<TransferEntry>)]
    pub entries: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ParsedRow {
    pub row_number: u32,
    pub entry: Option<TransferEntry>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[schema(as = collection_transfer::RowResult)]
pub struct RowResult {
    #[sqlx(rename = "record_number")]
    pub row_number: u32,
    pub copy_key: Option<String>,
    pub status: String,
    pub code: Option<String>,
    pub entry_id: Option<u32>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransferJob {
    pub id: String,
    pub user_id: Option<u32>,
    pub kind: String,
    pub format: String,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
    pub snapshot_at: Option<NaiveDateTime>,
    pub expires_at: NaiveDateTime,
    pub invalidated_at: Option<NaiveDateTime>,
    pub files_deleted: bool,
    pub error_code: Option<String>,
    pub total_rows: u32,
    pub byte_size: u64,
    pub source_sha256: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = collection_transfer::ResultCounts)]
pub struct ResultCounts {
    pub new: u32,
    pub created: u32,
    pub unchanged: u32,
    pub conflict: u32,
    pub invalid: u32,
    pub failed: u32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = collection_transfer::JobResponse)]
pub struct JobResponse {
    pub id: String,
    pub kind: String,
    pub format: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub snapshot_at: Option<String>,
    pub expires_at: String,
    pub error_code: Option<String>,
    pub total_rows: u32,
    pub counts: ResultCounts,
    pub results: Vec<RowResult>,
    pub page: u32,
    pub per_page: u32,
    pub byte_size: u64,
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TransferQuery {
    pub format: Option<String>,
    pub page: Option<u32>,
}
