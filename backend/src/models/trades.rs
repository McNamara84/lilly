use serde::{Deserialize, Serialize};

pub const MAX_BULK_WANTED_ISSUES: usize = 100;
pub const MAX_TRADE_LIST_SEARCH_LENGTH: usize = 200;

#[derive(Debug, Deserialize, Default)]
pub struct TradeListQueryParams {
    pub series_slug: Option<String>,
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

impl TradeListQueryParams {
    pub fn page(&self) -> u32 {
        self.page.max(1)
    }

    pub fn per_page(&self) -> u32 {
        self.per_page.clamp(1, 100)
    }

    pub fn offset(&self) -> u64 {
        u64::from(self.page().saturating_sub(1))
            .saturating_mul(u64::from(self.per_page()))
            .min(1_000_000)
    }

    pub fn series_slug(&self) -> Option<&str> {
        self.series_slug
            .as_deref()
            .map(str::trim)
            .filter(|slug| !slug.is_empty())
    }

    pub fn search(&self) -> Option<&str> {
        self.q
            .as_deref()
            .map(str::trim)
            .filter(|search| !search.is_empty())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self
            .series_slug()
            .is_some_and(|slug| slug.chars().count() > 100)
        {
            return Err("series_slug must not exceed 100 characters".to_string());
        }

        if self
            .search()
            .is_some_and(|search| search.chars().count() > MAX_TRADE_LIST_SEARCH_LENGTH)
        {
            return Err(format!(
                "q must not exceed {MAX_TRADE_LIST_SEARCH_LENGTH} characters"
            ));
        }

        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct TradeListEntryRow {
    pub entry_id: u32,
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: u8,
    pub condition_grade: Option<String>,
    pub owner_id: u32,
    pub owner_display_name: String,
}

#[derive(Debug, Serialize)]
pub struct TradeOfferResponse {
    pub entry_id: u32,
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: u8,
    pub condition_grade: String,
    pub offering_user_id: u32,
    pub offering_user_display_name: String,
}

impl TryFrom<&TradeListEntryRow> for TradeOfferResponse {
    type Error = String;

    fn try_from(row: &TradeListEntryRow) -> Result<Self, Self::Error> {
        let condition_grade = row
            .condition_grade
            .clone()
            .ok_or_else(|| "Tradeable entries must have a condition grade".to_string())?;

        Ok(Self {
            entry_id: row.entry_id,
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            title: row.title.clone(),
            series_id: row.series_id,
            series_name: row.series_name.clone(),
            series_slug: row.series_slug.clone(),
            cover_url: row.cover_url.clone(),
            cover_local_path: row.cover_local_path.clone(),
            copy_number: row.copy_number,
            condition_grade,
            offering_user_id: row.owner_id,
            offering_user_display_name: row.owner_display_name.clone(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct WantedEntryResponse {
    pub entry_id: u32,
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: u8,
    pub condition_grade: Option<String>,
}

impl From<&TradeListEntryRow> for WantedEntryResponse {
    fn from(row: &TradeListEntryRow) -> Self {
        Self {
            entry_id: row.entry_id,
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            title: row.title.clone(),
            series_id: row.series_id,
            series_name: row.series_name.clone(),
            series_slug: row.series_slug.clone(),
            cover_url: row.cover_url.clone(),
            cover_local_path: row.cover_local_path.clone(),
            copy_number: row.copy_number,
            condition_grade: row.condition_grade.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedTradeOffersResponse {
    pub data: Vec<TradeOfferResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct PaginatedWantedResponse {
    pub data: Vec<WantedEntryResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Debug, sqlx::FromRow)]
pub struct WantedCandidateRow {
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub wanted_entry_id: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct WantedCandidateResponse {
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub is_wanted: bool,
    pub wanted_entry_id: Option<u32>,
}

impl From<&WantedCandidateRow> for WantedCandidateResponse {
    fn from(row: &WantedCandidateRow) -> Self {
        Self {
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            title: row.title.clone(),
            series_id: row.series_id,
            series_name: row.series_name.clone(),
            series_slug: row.series_slug.clone(),
            cover_url: row.cover_url.clone(),
            cover_local_path: row.cover_local_path.clone(),
            is_wanted: row.wanted_entry_id.is_some(),
            wanted_entry_id: row.wanted_entry_id,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedWantedCandidatesResponse {
    pub data: Vec<WantedCandidateResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Debug, Deserialize)]
pub struct BulkWantedRequest {
    pub issue_ids: Vec<u32>,
}

pub fn normalize_bulk_issue_ids(issue_ids: &[u32]) -> Result<Vec<u32>, String> {
    if issue_ids.is_empty() {
        return Err("issue_ids must contain at least one issue".to_string());
    }
    if issue_ids.len() > MAX_BULK_WANTED_ISSUES {
        return Err(format!(
            "issue_ids must not contain more than {MAX_BULK_WANTED_ISSUES} items"
        ));
    }
    if issue_ids.contains(&0) {
        return Err("issue_ids must contain only positive IDs".to_string());
    }

    let mut normalized = issue_ids.to_vec();
    normalized.sort_unstable();
    normalized.dedup();
    Ok(normalized)
}

#[derive(Debug, Serialize)]
pub struct WantedMutationResult {
    pub issue_id: u32,
    pub entry_id: u32,
}

#[derive(Debug, Serialize)]
pub struct WantedRejection {
    pub issue_id: u32,
    pub reason: &'static str,
}

#[derive(Debug, Serialize, Default)]
pub struct BulkWantedResponse {
    pub created: Vec<WantedMutationResult>,
    pub unchanged: Vec<WantedMutationResult>,
    pub rejected: Vec<WantedRejection>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_row(condition_grade: Option<&str>) -> TradeListEntryRow {
        TradeListEntryRow {
            entry_id: 3,
            issue_id: 42,
            issue_number: 7,
            title: "Night of the Test".to_string(),
            series_id: 2,
            series_name: "Test Series".to_string(),
            series_slug: "test-series".to_string(),
            cover_url: Some("https://example.com/cover.jpg".to_string()),
            cover_local_path: None,
            copy_number: 1,
            condition_grade: condition_grade.map(str::to_string),
            owner_id: 9,
            owner_display_name: "Collector".to_string(),
        }
    }

    #[test]
    fn query_params_normalize_pagination_and_text() {
        let params = TradeListQueryParams {
            series_slug: Some("  maddrax  ".to_string()),
            q: Some("  ice  ".to_string()),
            page: 0,
            per_page: 500,
        };

        assert_eq!(params.page(), 1);
        assert_eq!(params.per_page(), 100);
        assert_eq!(params.offset(), 0);
        assert_eq!(params.series_slug(), Some("maddrax"));
        assert_eq!(params.search(), Some("ice"));
        assert!(params.validate().is_ok());
    }

    #[test]
    fn query_params_reject_overlong_values() {
        let params = TradeListQueryParams {
            series_slug: Some("s".repeat(101)),
            ..TradeListQueryParams::default()
        };
        assert!(params.validate().is_err());

        let params = TradeListQueryParams {
            q: Some("ä".repeat(MAX_TRADE_LIST_SEARCH_LENGTH + 1)),
            ..TradeListQueryParams::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn trade_offer_requires_and_exposes_condition_without_private_fields() {
        let response = TradeOfferResponse::try_from(&list_row(Some("Z2"))).unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["condition_grade"], "Z2");
        assert_eq!(json["offering_user_id"], 9);
        assert_eq!(json["offering_user_display_name"], "Collector");
        assert!(json.get("email").is_none());
        assert!(json.get("notes").is_none());
        assert!(json.get("photos").is_none());
        assert!(TradeOfferResponse::try_from(&list_row(None)).is_err());
    }

    #[test]
    fn wanted_entry_allows_missing_condition() {
        let response = WantedEntryResponse::from(&list_row(None));
        assert!(response.condition_grade.is_none());
        assert_eq!(response.issue_id, 42);
    }

    #[test]
    fn candidate_derives_wanted_state_from_entry_id() {
        let row = WantedCandidateRow {
            issue_id: 4,
            issue_number: 12,
            title: "Candidate".to_string(),
            series_id: 1,
            series_name: "Series".to_string(),
            series_slug: "series".to_string(),
            cover_url: None,
            cover_local_path: None,
            wanted_entry_id: Some(88),
        };
        let response = WantedCandidateResponse::from(&row);
        assert!(response.is_wanted);
        assert_eq!(response.wanted_entry_id, Some(88));
    }

    #[test]
    fn bulk_ids_are_validated_sorted_and_deduplicated() {
        assert_eq!(normalize_bulk_issue_ids(&[5, 2, 5, 3]).unwrap(), [2, 3, 5]);
        assert!(normalize_bulk_issue_ids(&[]).is_err());
        assert!(normalize_bulk_issue_ids(&[0]).is_err());
        assert!(normalize_bulk_issue_ids(&vec![1; MAX_BULK_WANTED_ISSUES + 1]).is_err());
    }
}
