use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Database row model
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct CollectionEntry {
    pub id: u32,
    pub user_id: u32,
    pub issue_id: u32,
    pub copy_number: u8,
    pub condition_grade: String,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate)]
pub struct AddCollectionEntryRequest {
    pub issue_id: u32,
    pub condition_grade: String,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub copy_number: Option<u8>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCollectionEntryRequest {
    pub condition_grade: Option<String>,
    pub status: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CollectionQueryParams {
    pub series_slug: Option<String>,
    pub status: Option<String>,
    pub issue_number: Option<u32>,
    pub condition: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub condition_min: Option<String>,
    pub condition_max: Option<String>,
    pub sort: Option<String>,
    pub sort_dir: Option<String>,
    pub q: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

const fn default_page() -> u32 {
    1
}
const fn default_per_page() -> u32 {
    50
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CollectionEntryResponse {
    pub id: u32,
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: Option<u8>,
    pub condition_grade: Option<String>,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedCollectionResponse {
    pub data: Vec<CollectionEntryResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct CollectionStatsResponse {
    pub total_issues: Option<u32>,
    pub total_owned: u32,
    pub total_duplicate: u32,
    pub total_wanted: u32,
    pub overall_progress_percent: Option<f64>,
    pub series_stats: Vec<SeriesStatsEntry>,
}

#[derive(Debug, Serialize)]
pub struct SeriesStatsEntry {
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub total_in_series: Option<u32>,
    pub owned_count: u32,
    pub duplicate_count: u32,
    pub wanted_count: u32,
    pub progress_percent: Option<f64>,
}

// ---------------------------------------------------------------------------
// Joined DB row for list queries
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct CollectionEntryRow {
    pub id: u32,
    pub user_id: u32,
    pub issue_id: u32,
    pub copy_number: u8,
    pub condition_grade: String,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub issue_number: u32,
    pub title: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
}

impl From<&CollectionEntryRow> for CollectionEntryResponse {
    fn from(r: &CollectionEntryRow) -> Self {
        Self {
            id: r.id,
            issue_id: r.issue_id,
            issue_number: r.issue_number,
            title: r.title.clone(),
            series_id: r.series_id,
            series_name: r.series_name.clone(),
            series_slug: r.series_slug.clone(),
            cover_url: r.cover_url.clone(),
            cover_local_path: r.cover_local_path.clone(),
            copy_number: Some(r.copy_number),
            condition_grade: Some(r.condition_grade.clone()),
            status: r.status.clone(),
            notes: r.notes.clone(),
            created_at: Some(r.created_at),
            updated_at: Some(r.updated_at),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

const VALID_CONDITION_GRADES: &[&str] = &["Z0", "Z1", "Z2", "Z3", "Z4"];
const VALID_STATUSES: &[&str] = &["owned", "duplicate", "wanted"];
const VALID_COLLECTION_SORTS: &[&str] = &[
    "series",
    "issue_number",
    "condition",
    "title",
    "author",
    "added",
];
const VALID_SORT_DIRECTIONS: &[&str] = &["asc", "desc"];
pub const MAX_COLLECTION_NOTE_LENGTH: usize = 10_000;

pub fn validate_condition_grade(grade: &str) -> Result<(), String> {
    if VALID_CONDITION_GRADES.contains(&grade) {
        Ok(())
    } else {
        Err(format!(
            "Invalid condition grade '{grade}'. Must be one of: Z0, Z1, Z2, Z3, Z4"
        ))
    }
}

pub fn validate_status(status: &str) -> Result<(), String> {
    if VALID_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(format!(
            "Invalid status '{status}'. Must be one of: owned, duplicate, wanted"
        ))
    }
}

/// Validate and normalize a personal collection note without changing its
/// meaningful whitespace. Empty or whitespace-only notes are stored as NULL.
pub fn normalize_collection_note(note: Option<&str>) -> Result<Option<&str>, String> {
    let Some(note) = note else {
        return Ok(None);
    };

    if note.chars().count() > MAX_COLLECTION_NOTE_LENGTH {
        return Err(format!(
            "Collection notes must not exceed {MAX_COLLECTION_NOTE_LENGTH} characters"
        ));
    }

    if note.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(note))
    }
}

pub fn validate_collection_sort(sort: &str) -> Result<(), String> {
    if VALID_COLLECTION_SORTS.contains(&sort) {
        Ok(())
    } else {
        Err(format!(
            "Invalid sort field '{sort}'. Must be one of: series, issue_number, condition, title, author, added"
        ))
    }
}

pub fn validate_sort_direction(direction: &str) -> Result<(), String> {
    if VALID_SORT_DIRECTIONS.contains(&direction) {
        Ok(())
    } else {
        Err(format!(
            "Invalid sort direction '{direction}'. Must be one of: asc, desc"
        ))
    }
}

pub fn validate_missing_collection_sort(sort: &str) -> Result<(), String> {
    if matches!(sort, "condition" | "added") {
        Err(format!(
            "Sort field '{sort}' cannot be used with status=missing"
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_condition_grade_valid() {
        for grade in VALID_CONDITION_GRADES {
            assert!(validate_condition_grade(grade).is_ok());
        }
    }

    #[test]
    fn test_validate_condition_grade_invalid() {
        assert!(validate_condition_grade("Z6").is_err());
        assert!(validate_condition_grade("Z5").is_err());
        assert!(validate_condition_grade("").is_err());
        assert!(validate_condition_grade("z0").is_err());
    }

    #[test]
    fn test_validate_status_valid() {
        for status in VALID_STATUSES {
            assert!(validate_status(status).is_ok());
        }
    }

    #[test]
    fn test_validate_status_invalid() {
        assert!(validate_status("sold").is_err());
        assert!(validate_status("").is_err());
        assert!(validate_status("missing").is_err());
    }

    #[test]
    fn test_normalize_collection_note_preserves_unicode_and_line_breaks() {
        let note = "  Erste Zeile\r\nZweite Zeile 🟡  ";
        assert_eq!(normalize_collection_note(Some(note)), Ok(Some(note)));
    }

    #[test]
    fn test_normalize_collection_note_turns_blank_text_into_none() {
        assert_eq!(normalize_collection_note(Some(" \n\t ")), Ok(None));
        assert_eq!(normalize_collection_note(None), Ok(None));
    }

    #[test]
    fn test_normalize_collection_note_counts_unicode_characters() {
        let valid = "ä".repeat(MAX_COLLECTION_NOTE_LENGTH);
        assert!(normalize_collection_note(Some(&valid)).is_ok());

        let invalid = "🟡".repeat(MAX_COLLECTION_NOTE_LENGTH + 1);
        assert!(normalize_collection_note(Some(&invalid)).is_err());
    }

    #[test]
    fn test_validate_collection_sort() {
        for sort in VALID_COLLECTION_SORTS {
            assert!(validate_collection_sort(sort).is_ok());
        }
        assert!(validate_collection_sort("publisher").is_err());
        assert!(validate_collection_sort("").is_err());
    }

    #[test]
    fn test_validate_sort_direction() {
        for direction in VALID_SORT_DIRECTIONS {
            assert!(validate_sort_direction(direction).is_ok());
        }
        assert!(validate_sort_direction("sideways").is_err());
        assert!(validate_sort_direction("").is_err());
    }

    #[test]
    fn test_validate_missing_collection_sort() {
        for sort in ["series", "issue_number", "title", "author"] {
            assert!(validate_missing_collection_sort(sort).is_ok());
        }
        assert!(validate_missing_collection_sort("condition").is_err());
        assert!(validate_missing_collection_sort("added").is_err());
    }

    #[test]
    fn test_collection_entry_response_from_row() {
        let row = CollectionEntryRow {
            id: 1,
            user_id: 10,
            issue_id: 100,
            copy_number: 1,
            condition_grade: "Z2".to_string(),
            status: "owned".to_string(),
            notes: Some("Nice copy".to_string()),
            created_at: chrono::NaiveDateTime::default(),
            updated_at: chrono::NaiveDateTime::default(),
            issue_number: 42,
            title: "Test Issue".to_string(),
            cover_url: Some("http://example.com/cover.jpg".to_string()),
            cover_local_path: None,
            series_id: 5,
            series_name: "Test Series".to_string(),
            series_slug: "test-series".to_string(),
        };

        let response = CollectionEntryResponse::from(&row);
        assert_eq!(response.id, 1);
        assert_eq!(response.issue_number, 42);
        assert_eq!(response.condition_grade, Some("Z2".to_string()));
        assert_eq!(response.status, "owned");
        assert_eq!(response.series_slug, "test-series");
        assert_eq!(response.notes, Some("Nice copy".to_string()));
        assert_eq!(response.copy_number, Some(1));
        assert!(response.created_at.is_some());
        assert!(response.updated_at.is_some());
    }

    #[test]
    fn test_default_query_params() {
        let params: CollectionQueryParams = serde_json::from_str(r"{}").unwrap();
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 50);
        assert!(params.series_slug.is_none());
        assert!(params.status.is_none());
    }

    #[test]
    fn test_add_request_deserialization() {
        let json = r#"{"issue_id": 42, "condition_grade": "Z1"}"#;
        let req: AddCollectionEntryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.issue_id, 42);
        assert_eq!(req.condition_grade, "Z1");
        assert!(req.status.is_none());
        assert!(req.copy_number.is_none());
    }

    #[test]
    fn test_update_request_all_none() {
        let json = r"{}";
        let req: UpdateCollectionEntryRequest = serde_json::from_str(json).unwrap();
        assert!(req.condition_grade.is_none());
        assert!(req.status.is_none());
        assert!(req.notes.is_none());
    }
}
