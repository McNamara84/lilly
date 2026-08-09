use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewItem {
    pub id: u32,
    pub job_id: u32,
    pub issue_id: Option<u32>,
    pub issue_number: u32,
    pub outcome: String,
    pub severity: String,
    pub stage: Option<String>,
    pub message: Option<String>,
    pub source_key: String,
    pub source_record_id: Option<String>,
    pub source_url: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub cover_artists: Vec<String>,
    pub published_at: Option<NaiveDate>,
    pub part_number: Option<u32>,
    pub part_total: Option<u32>,
    pub cycle: Option<String>,
    pub cover_status: String,
    pub cover_reason: Option<String>,
    pub cover_local_path: Option<String>,
    pub processed_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaginatedReviewItems {
    pub items: Vec<ReviewItem>,
    pub total: u32,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ReviewOutcomeCounts {
    pub total: u32,
    pub not_processed: u32,
    pub created: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub skipped: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EligibilityReason {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActivationEligibility {
    pub eligible: bool,
    pub requires_acknowledgement: bool,
    pub reasons: Vec<EligibilityReason>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReferenceCheck {
    pub issue_number: u32,
    pub expected_title: String,
    pub expected_authors: Vec<String>,
    pub expected_published_at: NaiveDate,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct PublicationEvent {
    pub id: u32,
    pub series_id: u32,
    pub import_job_id: Option<u32>,
    pub actor_user_id: Option<u32>,
    pub action: String,
    pub decision: Option<String>,
    pub warning_count: u32,
    pub blocking_count: u32,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewSummary {
    pub job_id: u32,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub series_active: bool,
    pub job_status: String,
    pub outcomes: ReviewOutcomeCounts,
    pub warning_count: u32,
    pub blocking_count: u32,
    pub eligibility: ActivationEligibility,
    pub reference_checks: Vec<ReferenceCheck>,
    pub sample_issue_numbers: Vec<u32>,
    pub last_publication_event: Option<PublicationEvent>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ActivateImportRequest {
    #[serde(default)]
    pub acknowledge_warnings: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActivationResponse {
    pub series_id: u32,
    pub active: bool,
    pub event: Option<PublicationEvent>,
}
