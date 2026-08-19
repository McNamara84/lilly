use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::models::profile::avatar_content_url;

pub const MAX_PROPOSAL_ENTRIES_PER_DIRECTION: usize = 100;

#[derive(Debug, Deserialize, Default)]
pub struct PageParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

const fn default_page() -> u32 {
    1
}

const fn default_per_page() -> u32 {
    24
}

impl PageParams {
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
}

#[derive(Debug, Deserialize, Default)]
pub struct TradePageParams {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

impl TradePageParams {
    pub fn scope(&self) -> &str {
        self.scope.as_deref().unwrap_or("open")
    }

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

    pub fn validate(&self) -> Result<(), String> {
        if matches!(self.scope(), "open" | "closed") {
            Ok(())
        } else {
            Err("scope must be one of: open, closed".to_string())
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct MatchRecord {
    pub id: u32,
    pub user_low_id: u32,
    pub user_high_id: u32,
    pub status: String,
    pub fingerprint: String,
    pub revision: u32,
    pub detected_at: NaiveDateTime,
    pub changed_at: NaiveDateTime,
    pub stale_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct MatchItemRecord {
    pub id: u32,
    pub match_id: u32,
    pub offer_entry_id: u32,
    pub wanted_entry_id: u32,
    pub issue_id: u32,
    pub offered_by_user_id: u32,
    pub wanted_by_user_id: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MatchListRow {
    pub id: u32,
    pub status: String,
    pub revision: u32,
    pub changed_at: NaiveDateTime,
    pub partner_id: u32,
    pub partner_display_name: String,
    pub partner_profile_public: bool,
    pub partner_avatar_path: Option<String>,
    pub partner_location: Option<String>,
    pub open_trade_id: Option<u32>,
    pub open_trade_status: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct MatchItemViewRow {
    pub match_id: u32,
    pub offer_entry_id: u32,
    pub wanted_entry_id: u32,
    pub issue_id: u32,
    pub offered_by_user_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: u8,
    pub edition_label: Option<String>,
    pub wanted_edition_label: Option<String>,
    pub condition_grade: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradePartnerResponse {
    pub id: Option<u32>,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub location: Option<String>,
}

impl From<&MatchListRow> for TradePartnerResponse {
    fn from(row: &MatchListRow) -> Self {
        Self {
            id: Some(row.partner_id),
            display_name: row.partner_display_name.clone(),
            avatar_path: row
                .partner_profile_public
                .then(|| avatar_content_url(row.partner_id, row.partner_avatar_path.is_some()))
                .flatten(),
            location: row
                .partner_profile_public
                .then(|| row.partner_location.clone())
                .flatten(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchIssueResponse {
    pub entry_id: u32,
    pub wanted_entry_id: u32,
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: u8,
    pub edition_label: Option<String>,
    pub wanted_edition_label: Option<String>,
    pub condition_grade: String,
}

impl From<&MatchItemViewRow> for MatchIssueResponse {
    fn from(row: &MatchItemViewRow) -> Self {
        Self {
            entry_id: row.offer_entry_id,
            wanted_entry_id: row.wanted_entry_id,
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            title: row.title.clone(),
            series_id: row.series_id,
            series_name: row.series_name.clone(),
            series_slug: row.series_slug.clone(),
            cover_url: row.cover_url.clone(),
            cover_local_path: row.cover_local_path.clone(),
            copy_number: row.copy_number,
            edition_label: row.edition_label.clone(),
            wanted_edition_label: row.wanted_edition_label.clone(),
            condition_grade: row.condition_grade.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeMatchResponse {
    pub id: u32,
    pub status: String,
    pub revision: u32,
    pub changed_at: NaiveDateTime,
    pub partner: TradePartnerResponse,
    pub my_offers: Vec<MatchIssueResponse>,
    pub partner_offers: Vec<MatchIssueResponse>,
    pub match_score: u8,
    pub open_trade_id: Option<u32>,
    pub open_trade_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedMatchesResponse {
    pub data: Vec<TradeMatchResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Debug, Deserialize)]
pub struct CreateTradeProposalRequest {
    pub offered_entry_ids: Vec<u32>,
    pub requested_entry_ids: Vec<u32>,
}

impl CreateTradeProposalRequest {
    pub fn normalize(&self) -> Result<(Vec<u32>, Vec<u32>), String> {
        Ok((
            normalize_entry_ids("offered_entry_ids", &self.offered_entry_ids)?,
            normalize_entry_ids("requested_entry_ids", &self.requested_entry_ids)?,
        ))
    }
}

fn normalize_entry_ids(field: &str, values: &[u32]) -> Result<Vec<u32>, String> {
    if values.is_empty() {
        return Err(format!("{field} must contain at least one entry"));
    }
    if values.len() > MAX_PROPOSAL_ENTRIES_PER_DIRECTION {
        return Err(format!(
            "{field} must not contain more than {MAX_PROPOSAL_ENTRIES_PER_DIRECTION} entries"
        ));
    }
    if values.contains(&0) {
        return Err(format!("{field} must contain only positive IDs"));
    }
    let mut normalized = values.to_vec();
    normalized.sort_unstable();
    normalized.dedup();
    Ok(normalized)
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct TradeRecord {
    pub id: u32,
    pub match_id: Option<u32>,
    pub initiator_id: Option<u32>,
    pub responder_id: Option<u32>,
    pub status: String,
    pub cancellation_reason: Option<String>,
    pub proposed_at: NaiveDateTime,
    pub accepted_at: Option<NaiveDateTime>,
    pub cancelled_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct TradeListRow {
    pub id: u32,
    pub match_id: Option<u32>,
    pub initiator_id: Option<u32>,
    pub responder_id: Option<u32>,
    pub status: String,
    pub cancellation_reason: Option<String>,
    pub proposed_at: NaiveDateTime,
    pub accepted_at: Option<NaiveDateTime>,
    pub cancelled_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub my_completion_confirmed_at: Option<NaiveDateTime>,
    pub partner_completion_confirmed_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub partner_id: Option<u32>,
    pub partner_display_name: String,
    pub partner_profile_public: bool,
    pub partner_avatar_path: Option<String>,
    pub partner_location: Option<String>,
    pub thread_id: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct TradeItemViewRow {
    pub trade_id: u32,
    pub offer_entry_id: Option<u32>,
    pub wanted_entry_id: Option<u32>,
    pub issue_id: u32,
    pub offered_by_user_id: Option<u32>,
    pub receiving_user_id: Option<u32>,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number_snapshot: u8,
    pub edition_label_snapshot: Option<String>,
    pub wanted_edition_label_snapshot: Option<String>,
    pub condition_grade_snapshot: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeItemResponse {
    pub entry_id: Option<u32>,
    pub wanted_entry_id: Option<u32>,
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: u8,
    pub edition_label: Option<String>,
    pub wanted_edition_label: Option<String>,
    pub condition_grade: String,
}

impl From<&TradeItemViewRow> for TradeItemResponse {
    fn from(row: &TradeItemViewRow) -> Self {
        Self {
            entry_id: row.offer_entry_id,
            wanted_entry_id: row.wanted_entry_id,
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            title: row.title.clone(),
            series_id: row.series_id,
            series_name: row.series_name.clone(),
            series_slug: row.series_slug.clone(),
            cover_url: row.cover_url.clone(),
            cover_local_path: row.cover_local_path.clone(),
            copy_number: row.copy_number_snapshot,
            edition_label: row.edition_label_snapshot.clone(),
            wanted_edition_label: row.wanted_edition_label_snapshot.clone(),
            condition_grade: row.condition_grade_snapshot.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeResponse {
    pub id: u32,
    pub match_id: Option<u32>,
    pub status: String,
    pub role: String,
    pub partner: TradePartnerResponse,
    pub my_offers: Vec<TradeItemResponse>,
    pub partner_offers: Vec<TradeItemResponse>,
    pub thread_id: u32,
    pub cancellation_reason: Option<String>,
    pub proposed_at: NaiveDateTime,
    pub accepted_at: Option<NaiveDateTime>,
    pub cancelled_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub my_completion_confirmed_at: Option<NaiveDateTime>,
    pub partner_completion_confirmed_at: Option<NaiveDateTime>,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct PaginatedTradesResponse {
    pub data: Vec<TradeResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

pub fn calculate_match_score(my_offers: usize, partner_offers: usize) -> u8 {
    if my_offers == 0 || partner_offers == 0 {
        return 0;
    }
    let smaller = my_offers.min(partner_offers);
    let total = my_offers.saturating_add(partner_offers);
    let rounded_score = 200_usize.saturating_mul(smaller).saturating_add(total / 2) / total;
    u8::try_from(rounded_score).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_is_bounded() {
        let params = PageParams {
            page: 0,
            per_page: 500,
        };
        assert_eq!(params.page(), 1);
        assert_eq!(params.per_page(), 100);
        assert_eq!(params.offset(), 0);
    }

    #[test]
    fn trade_scope_defaults_and_validates() {
        let default = TradePageParams::default();
        assert_eq!(default.scope(), "open");
        assert!(default.validate().is_ok());

        let closed = TradePageParams {
            scope: Some("closed".to_string()),
            page: 0,
            per_page: 500,
        };
        assert!(closed.validate().is_ok());
        assert_eq!(closed.page(), 1);
        assert_eq!(closed.per_page(), 100);
        assert_eq!(closed.offset(), 0);

        let invalid = TradePageParams {
            scope: Some("all".to_string()),
            ..TradePageParams::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn proposal_ids_are_required_positive_bounded_and_deduplicated() {
        let request = CreateTradeProposalRequest {
            offered_entry_ids: vec![4, 2, 4],
            requested_entry_ids: vec![8],
        };
        assert_eq!(request.normalize().unwrap(), (vec![2, 4], vec![8]));
        assert!(normalize_entry_ids("items", &[]).is_err());
        assert!(normalize_entry_ids("items", &[0]).is_err());
        assert!(
            normalize_entry_ids("items", &vec![1; MAX_PROPOSAL_ENTRIES_PER_DIRECTION + 1]).is_err()
        );
    }

    #[test]
    fn match_score_rewards_balance() {
        assert_eq!(calculate_match_score(0, 2), 0);
        assert_eq!(calculate_match_score(1, 1), 100);
        assert_eq!(calculate_match_score(1, 3), 50);
        assert_eq!(calculate_match_score(2, 3), 80);
    }

    #[test]
    fn private_partner_fields_are_suppressed() {
        let row = MatchListRow {
            id: 1,
            status: "active".to_string(),
            revision: 1,
            changed_at: NaiveDateTime::default(),
            partner_id: 2,
            partner_display_name: "Private".to_string(),
            partner_profile_public: false,
            partner_avatar_path: Some("/secret.webp".to_string()),
            partner_location: Some("Secret".to_string()),
            open_trade_id: None,
            open_trade_status: None,
        };
        let partner = TradePartnerResponse::from(&row);
        assert!(partner.avatar_path.is_none());
        assert!(partner.location.is_none());
    }

    #[test]
    fn public_partner_avatar_uses_a_controlled_content_url() {
        let row = MatchListRow {
            id: 1,
            status: "active".to_string(),
            revision: 1,
            changed_at: NaiveDateTime::default(),
            partner_id: 2,
            partner_display_name: "Public".to_string(),
            partner_profile_public: true,
            partner_avatar_path: Some("internal-storage-key.jpg".to_string()),
            partner_location: None,
            open_trade_id: None,
            open_trade_status: None,
        };

        assert_eq!(
            TradePartnerResponse::from(&row).avatar_path.as_deref(),
            Some("/api/v1/users/2/avatar")
        );
    }
}
