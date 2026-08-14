use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlConnection, MySqlPool, QueryBuilder, Transaction};

use crate::db::trade_workflow;
use crate::models::trade_matching::{
    MatchItemRecord, MatchItemViewRow, MatchListRow, MatchRecord, PageParams,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, sqlx::FromRow)]
struct CandidateItem {
    offer_entry_id: u32,
    wanted_entry_id: u32,
    issue_id: u32,
    offered_by_user_id: u32,
    wanted_by_user_id: u32,
    issue_number: u32,
    title: String,
    series_name: String,
    edition_label: Option<String>,
    wanted_edition_label: Option<String>,
}

impl CandidateItem {
    fn pair(&self) -> (u32, u32) {
        normalize_user_pair(self.offered_by_user_id, self.wanted_by_user_id)
            .expect("candidate query excludes self matches")
    }

    fn identity(&self) -> (u32, u32, u32, u32, u32) {
        (
            self.offer_entry_id,
            self.wanted_entry_id,
            self.issue_id,
            self.offered_by_user_id,
            self.wanted_by_user_id,
        )
    }
}

/// Reduces the compatibility join to a deterministic one-to-one assignment.
/// Edition-specific wishes are assigned before generic wishes so a generic
/// wish cannot consume the only offer that satisfies a specific edition.
fn select_one_to_one_candidates(candidates: Vec<CandidateItem>) -> Vec<CandidateItem> {
    let mut by_direction_and_issue = BTreeMap::<(u32, u32, u32), Vec<CandidateItem>>::new();
    for candidate in candidates {
        by_direction_and_issue
            .entry((
                candidate.offered_by_user_id,
                candidate.wanted_by_user_id,
                candidate.issue_id,
            ))
            .or_default()
            .push(candidate);
    }

    let mut selected = Vec::new();
    for mut items in by_direction_and_issue.into_values() {
        items.sort_by_key(|item| {
            (
                item.wanted_edition_label.is_none(),
                item.wanted_entry_id,
                item.offer_entry_id,
            )
        });
        let mut used_offers = BTreeSet::new();
        let mut used_wishes = BTreeSet::new();
        for item in items {
            if used_offers.contains(&item.offer_entry_id)
                || used_wishes.contains(&item.wanted_entry_id)
            {
                continue;
            }
            used_offers.insert(item.offer_entry_id);
            used_wishes.insert(item.wanted_entry_id);
            selected.push(item);
        }
    }
    selected.sort();
    selected
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReconciliationStats {
    pub created: u32,
    pub updated: u32,
    pub reactivated: u32,
    pub staled: u32,
}

impl ReconciliationStats {
    fn merge(&mut self, other: Self) {
        self.created = self.created.saturating_add(other.created);
        self.updated = self.updated.saturating_add(other.updated);
        self.reactivated = self.reactivated.saturating_add(other.reactivated);
        self.staled = self.staled.saturating_add(other.staled);
    }
}

pub fn normalize_user_pair(first: u32, second: u32) -> Option<(u32, u32)> {
    match first.cmp(&second) {
        std::cmp::Ordering::Less => Some((first, second)),
        std::cmp::Ordering::Greater => Some((second, first)),
        std::cmp::Ordering::Equal => None,
    }
}

pub async fn reconcile_all_matches(pool: &MySqlPool) -> Result<ReconciliationStats, sqlx::Error> {
    let user_ids = sqlx::query_scalar::<_, u32>("SELECT id FROM users ORDER BY id")
        .fetch_all(pool)
        .await?;
    let mut total = ReconciliationStats::default();
    for user_id in user_ids {
        let mut transaction = pool.begin().await?;
        let stats = reconcile_user_matches(&mut transaction, user_id).await?;
        transaction.commit().await?;
        total.merge(stats);
    }
    Ok(total)
}

#[allow(clippy::too_many_lines)]
pub async fn reconcile_user_matches(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
) -> Result<ReconciliationStats, sqlx::Error> {
    let candidates =
        select_one_to_one_candidates(find_candidate_items(transaction, user_id).await?);
    let mut by_pair = BTreeMap::<(u32, u32), Vec<CandidateItem>>::new();
    for candidate in candidates {
        by_pair.entry(candidate.pair()).or_default().push(candidate);
    }

    for pair in find_existing_pairs(transaction, user_id).await? {
        by_pair.entry(pair).or_default();
    }

    let mut stats = ReconciliationStats::default();
    for ((user_low_id, user_high_id), mut items) in by_pair {
        lock_user_pair(transaction, user_low_id, user_high_id).await?;
        items.sort();
        let has_low_to_high = items
            .iter()
            .any(|item| item.offered_by_user_id == user_low_id);
        let has_high_to_low = items
            .iter()
            .any(|item| item.offered_by_user_id == user_high_id);
        let eligible = has_low_to_high && has_high_to_low;

        let existing = find_match_for_update(transaction, user_low_id, user_high_id).await?;
        if eligible {
            let fingerprint = fingerprint_items(&items);
            match existing {
                None => {
                    let match_id =
                        insert_match(transaction, user_low_id, user_high_id, &fingerprint).await?;
                    replace_match_items(transaction, match_id, &items).await?;
                    create_match_notifications(
                        transaction,
                        match_id,
                        1,
                        "trade_match",
                        user_low_id,
                        user_high_id,
                        &items,
                    )
                    .await?;
                    stats.created = stats.created.saturating_add(1);
                }
                Some(record) => {
                    let old_items = find_match_items(transaction, record.id).await?;
                    let old_identities = old_items
                        .iter()
                        .map(|item| {
                            (
                                item.offer_entry_id,
                                item.wanted_entry_id,
                                item.issue_id,
                                item.offered_by_user_id,
                                item.wanted_by_user_id,
                            )
                        })
                        .collect::<BTreeSet<_>>();
                    let new_identities = items
                        .iter()
                        .map(CandidateItem::identity)
                        .collect::<BTreeSet<_>>();
                    let has_additions = new_identities.difference(&old_identities).next().is_some();
                    let reactivated = record.status == "stale";
                    if record.fingerprint != fingerprint || reactivated {
                        let revision = record.revision.saturating_add(1);
                        update_match_active(transaction, record.id, &fingerprint, revision).await?;
                        replace_match_items(transaction, record.id, &items).await?;
                        if reactivated || has_additions {
                            create_match_notifications(
                                transaction,
                                record.id,
                                revision,
                                "trade_match_updated",
                                user_low_id,
                                user_high_id,
                                &items,
                            )
                            .await?;
                        }
                        if reactivated {
                            stats.reactivated = stats.reactivated.saturating_add(1);
                        } else {
                            stats.updated = stats.updated.saturating_add(1);
                        }
                    }
                }
            }
        } else if let Some(record) = existing
            && record.status == "active"
        {
            mark_match_stale(transaction, record.id, record.revision.saturating_add(1)).await?;
            sqlx::query("DELETE FROM trade_match_items WHERE match_id = ?")
                .bind(record.id)
                .execute(&mut **transaction)
                .await?;
            trade_workflow::cancel_proposals_for_match(transaction, record.id, Some(user_id))
                .await?;
            stats.staled = stats.staled.saturating_add(1);
        }
    }

    Ok(stats)
}

async fn find_candidate_items(
    connection: &mut MySqlConnection,
    user_id: u32,
) -> Result<Vec<CandidateItem>, sqlx::Error> {
    sqlx::query_as::<_, CandidateItem>(
        "SELECT offer.id AS offer_entry_id, wanted.id AS wanted_entry_id,
                offer.issue_id, offer.user_id AS offered_by_user_id,
                wanted.user_id AS wanted_by_user_id, i.issue_number, i.title,
                s.name AS series_name, offer.edition_label,
                wanted.edition_label AS wanted_edition_label
         FROM collection_entries offer
         JOIN collection_entries wanted
           ON wanted.issue_id = offer.issue_id
          AND wanted.status = 'wanted'
          AND wanted.user_id <> offer.user_id
          AND (wanted.edition_label IS NULL
               OR wanted.edition_label = offer.edition_label)
         JOIN issues i ON i.id = offer.issue_id
         JOIN series s ON s.id = i.series_id AND s.active = TRUE
         WHERE offer.status = 'duplicate'
           AND (offer.user_id = ? OR wanted.user_id = ?)
         ORDER BY LEAST(offer.user_id, wanted.user_id),
                  GREATEST(offer.user_id, wanted.user_id),
                  offer.id, wanted.id",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(connection)
    .await
}

async fn find_existing_pairs(
    connection: &mut MySqlConnection,
    user_id: u32,
) -> Result<Vec<(u32, u32)>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct PairRow {
        user_low_id: u32,
        user_high_id: u32,
    }
    Ok(sqlx::query_as::<_, PairRow>(
        "SELECT user_low_id, user_high_id FROM trade_matches
         WHERE user_low_id = ? OR user_high_id = ?",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(connection)
    .await?
    .into_iter()
    .map(|row| (row.user_low_id, row.user_high_id))
    .collect())
}

/// Serializes collection mutations that can affect the same reciprocal match.
/// Call this before changing any collection row for the supplied issues so the
/// first candidate snapshot taken by reconciliation observes prior commits.
pub async fn lock_reconciliation_users_for_issues(
    connection: &mut MySqlConnection,
    user_id: u32,
    issue_ids: &[u32],
) -> Result<(), sqlx::Error> {
    if !issue_ids.is_empty() {
        let mut issues = QueryBuilder::<MySql>::new("SELECT id FROM issues WHERE id IN (");
        let mut separated = issues.separated(", ");
        for issue_id in issue_ids.iter().copied().collect::<BTreeSet<_>>() {
            separated.push_bind(issue_id);
        }
        separated.push_unseparated(") ORDER BY id FOR UPDATE");
        issues
            .build_query_scalar::<u32>()
            .fetch_all(&mut *connection)
            .await?;
    }

    let mut user_ids = BTreeSet::from([user_id]);
    if !issue_ids.is_empty() {
        let mut partners = QueryBuilder::<MySql>::new(
            "SELECT DISTINCT user_id FROM collection_entries
             WHERE user_id <> ",
        );
        partners.push_bind(user_id).push(" AND issue_id IN (");
        let mut separated = partners.separated(", ");
        for issue_id in issue_ids {
            separated.push_bind(issue_id);
        }
        separated.push_unseparated(")");
        user_ids.extend(
            partners
                .build_query_scalar::<u32>()
                .fetch_all(&mut *connection)
                .await?,
        );
    }

    let mut lock = QueryBuilder::<MySql>::new("SELECT id FROM users WHERE id IN (");
    let mut separated = lock.separated(", ");
    for candidate_user_id in user_ids {
        separated.push_bind(candidate_user_id);
    }
    separated.push_unseparated(") ORDER BY id FOR UPDATE");
    lock.build_query_scalar::<u32>()
        .fetch_all(connection)
        .await?;
    Ok(())
}

async fn lock_user_pair(
    connection: &mut MySqlConnection,
    user_low_id: u32,
    user_high_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, u32>("SELECT id FROM users WHERE id IN (?, ?) ORDER BY id FOR UPDATE")
        .bind(user_low_id)
        .bind(user_high_id)
        .fetch_all(connection)
        .await?;
    Ok(())
}

async fn find_match_for_update(
    connection: &mut MySqlConnection,
    user_low_id: u32,
    user_high_id: u32,
) -> Result<Option<MatchRecord>, sqlx::Error> {
    sqlx::query_as::<_, MatchRecord>(
        "SELECT id, user_low_id, user_high_id, status, fingerprint, revision,
                detected_at, changed_at, stale_at
         FROM trade_matches
         WHERE user_low_id = ? AND user_high_id = ?
         FOR UPDATE",
    )
    .bind(user_low_id)
    .bind(user_high_id)
    .fetch_optional(connection)
    .await
}

async fn insert_match(
    connection: &mut MySqlConnection,
    user_low_id: u32,
    user_high_id: u32,
    fingerprint: &str,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO trade_matches
            (user_low_id, user_high_id, status, fingerprint, revision)
         VALUES (?, ?, 'active', ?, 1)",
    )
    .bind(user_low_id)
    .bind(user_high_id)
    .bind(fingerprint)
    .execute(connection)
    .await?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

async fn update_match_active(
    connection: &mut MySqlConnection,
    match_id: u32,
    fingerprint: &str,
    revision: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE trade_matches
         SET status = 'active', fingerprint = ?, revision = ?,
             changed_at = CURRENT_TIMESTAMP, stale_at = NULL
         WHERE id = ?",
    )
    .bind(fingerprint)
    .bind(revision)
    .bind(match_id)
    .execute(connection)
    .await?;
    Ok(())
}

async fn mark_match_stale(
    connection: &mut MySqlConnection,
    match_id: u32,
    revision: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE trade_matches
         SET status = 'stale', revision = ?, changed_at = CURRENT_TIMESTAMP,
             stale_at = CURRENT_TIMESTAMP
         WHERE id = ?",
    )
    .bind(revision)
    .bind(match_id)
    .execute(connection)
    .await?;
    Ok(())
}

async fn replace_match_items(
    connection: &mut MySqlConnection,
    match_id: u32,
    items: &[CandidateItem],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM trade_match_items WHERE match_id = ?")
        .bind(match_id)
        .execute(&mut *connection)
        .await?;
    for item in items {
        sqlx::query(
            "INSERT INTO trade_match_items
                (match_id, offer_entry_id, wanted_entry_id, issue_id,
                 offered_by_user_id, wanted_by_user_id)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(match_id)
        .bind(item.offer_entry_id)
        .bind(item.wanted_entry_id)
        .bind(item.issue_id)
        .bind(item.offered_by_user_id)
        .bind(item.wanted_by_user_id)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

async fn create_match_notifications(
    connection: &mut MySqlConnection,
    match_id: u32,
    revision: u32,
    kind: &str,
    user_low_id: u32,
    user_high_id: u32,
    items: &[CandidateItem],
) -> Result<(), sqlx::Error> {
    let display_names = sqlx::query_as::<_, UserDisplayRow>(
        "SELECT id, display_name FROM users WHERE id IN (?, ?)",
    )
    .bind(user_low_id)
    .bind(user_high_id)
    .fetch_all(&mut *connection)
    .await?;
    for recipient in [user_low_id, user_high_id] {
        let partner_id = if recipient == user_low_id {
            user_high_id
        } else {
            user_low_id
        };
        let partner_name = display_names
            .iter()
            .find(|user| user.id == partner_id)
            .map_or("Sammler", |user| user.display_name.as_str());
        let my_offers = item_summaries(items, recipient);
        let partner_offers = item_summaries(items, partner_id);
        let payload = json!({
            "partner": { "id": partner_id, "display_name": partner_name },
            "my_offers": my_offers,
            "partner_offers": partner_offers,
            "revision": revision
        });
        sqlx::query(
            "INSERT IGNORE INTO notifications
                (user_id, kind, match_id, dedupe_key, payload)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(recipient)
        .bind(kind)
        .bind(match_id)
        .bind(format!("match:{match_id}:revision:{revision}"))
        .bind(payload)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct UserDisplayRow {
    id: u32,
    display_name: String,
}

fn item_summaries(items: &[CandidateItem], offered_by: u32) -> Vec<serde_json::Value> {
    items
        .iter()
        .filter(|item| item.offered_by_user_id == offered_by)
        .map(|item| {
            json!({
                "issue_id": item.issue_id,
                "issue_number": item.issue_number,
                "title": item.title,
                "series_name": item.series_name,
                "edition_label": item.edition_label
            })
        })
        .collect()
}

fn fingerprint_items(items: &[CandidateItem]) -> String {
    let mut hasher = Sha256::new();
    for item in items {
        let identity = item.identity();
        hasher.update(identity.0.to_be_bytes());
        hasher.update(identity.1.to_be_bytes());
        hasher.update(identity.2.to_be_bytes());
        hasher.update(identity.3.to_be_bytes());
        hasher.update(identity.4.to_be_bytes());
        if let Some(edition_label) = &item.edition_label {
            hasher.update(edition_label.as_bytes());
        }
        hasher.update([0]);
        if let Some(wanted_edition_label) = &item.wanted_edition_label {
            hasher.update(wanted_edition_label.as_bytes());
        }
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

async fn find_match_items(
    connection: &mut MySqlConnection,
    match_id: u32,
) -> Result<Vec<MatchItemRecord>, sqlx::Error> {
    sqlx::query_as::<_, MatchItemRecord>(
        "SELECT id, match_id, offer_entry_id, wanted_entry_id, issue_id,
                offered_by_user_id, wanted_by_user_id
         FROM trade_match_items WHERE match_id = ? ORDER BY id",
    )
    .bind(match_id)
    .fetch_all(connection)
    .await
}

pub async fn count_matches(pool: &MySqlPool, user_id: u32) -> Result<u32, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trade_matches
         WHERE status = 'active' AND (user_low_id = ? OR user_high_id = ?)",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

pub async fn find_matches(
    pool: &MySqlPool,
    user_id: u32,
    params: &PageParams,
) -> Result<Vec<MatchListRow>, sqlx::Error> {
    sqlx::query_as::<_, MatchListRow>(
        "SELECT m.id, m.status, m.revision, m.changed_at,
                partner.id AS partner_id, partner.display_name AS partner_display_name,
                partner.profile_public AS partner_profile_public,
                partner.avatar_path AS partner_avatar_path,
                partner.location AS partner_location,
                ot.id AS open_trade_id, ot.status AS open_trade_status
         FROM trade_matches m
         JOIN users partner ON partner.id = CASE
             WHEN m.user_low_id = ? THEN m.user_high_id ELSE m.user_low_id END
         LEFT JOIN trades ot ON ot.open_match_id = m.id
         WHERE m.status = 'active' AND (m.user_low_id = ? OR m.user_high_id = ?)
         ORDER BY m.changed_at DESC, m.id ASC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(params.per_page())
    .bind(params.offset())
    .fetch_all(pool)
    .await
}

pub async fn find_match_for_participant(
    pool: &MySqlPool,
    user_id: u32,
    match_id: u32,
) -> Result<Option<MatchListRow>, sqlx::Error> {
    sqlx::query_as::<_, MatchListRow>(
        "SELECT m.id, m.status, m.revision, m.changed_at,
                partner.id AS partner_id, partner.display_name AS partner_display_name,
                partner.profile_public AS partner_profile_public,
                partner.avatar_path AS partner_avatar_path,
                partner.location AS partner_location,
                ot.id AS open_trade_id, ot.status AS open_trade_status
         FROM trade_matches m
         JOIN users partner ON partner.id = CASE
             WHEN m.user_low_id = ? THEN m.user_high_id ELSE m.user_low_id END
         LEFT JOIN trades ot ON ot.open_match_id = m.id
         WHERE m.id = ? AND (m.user_low_id = ? OR m.user_high_id = ?)",
    )
    .bind(user_id)
    .bind(match_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_match_item_views(
    pool: &MySqlPool,
    match_id: u32,
) -> Result<Vec<MatchItemViewRow>, sqlx::Error> {
    sqlx::query_as::<_, MatchItemViewRow>(
        "SELECT mi.match_id, mi.offer_entry_id, mi.wanted_entry_id, mi.issue_id,
                mi.offered_by_user_id, i.issue_number, i.title,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug,
                i.cover_url, i.cover_local_path, offer.copy_number, offer.edition_label,
                wanted.edition_label AS wanted_edition_label,
                offer.condition_grade
         FROM trade_match_items mi
         JOIN collection_entries offer ON offer.id = mi.offer_entry_id
         JOIN collection_entries wanted ON wanted.id = mi.wanted_entry_id
         JOIN issues i ON i.id = mi.issue_id
         JOIN series s ON s.id = i.series_id
         WHERE mi.match_id = ?
         ORDER BY mi.offered_by_user_id, s.name, i.issue_number, offer.copy_number",
    )
    .bind(match_id)
    .fetch_all(pool)
    .await
}

pub async fn find_match_item_views_for_matches(
    pool: &MySqlPool,
    match_ids: &[u32],
) -> Result<Vec<MatchItemViewRow>, sqlx::Error> {
    if match_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT mi.match_id, mi.offer_entry_id, mi.wanted_entry_id, mi.issue_id,
                mi.offered_by_user_id, i.issue_number, i.title,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug,
                i.cover_url, i.cover_local_path, offer.copy_number, offer.edition_label,
                wanted.edition_label AS wanted_edition_label,
                offer.condition_grade
         FROM trade_match_items mi
         JOIN collection_entries offer ON offer.id = mi.offer_entry_id
         JOIN collection_entries wanted ON wanted.id = mi.wanted_entry_id
         JOIN issues i ON i.id = mi.issue_id
         JOIN series s ON s.id = i.series_id
         WHERE mi.match_id IN (",
    );
    let mut separated = query.separated(", ");
    for match_id in match_ids {
        separated.push_bind(match_id);
    }
    separated.push_unseparated(
        ") ORDER BY mi.match_id, mi.offered_by_user_id, s.name,
                    i.issue_number, offer.copy_number",
    );
    query.build_query_as().fetch_all(pool).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sqlx::mysql::MySqlPoolOptions;
    use tokio::sync::Barrier;

    use super::*;
    use crate::error::AppError;
    use crate::models::messaging::{MessagePageParams, SendMessageRequest};
    use crate::models::trade_matching::{CreateTradeProposalRequest, PageParams, TradePageParams};
    use crate::services::{messaging, trade_matching as matching_service, trades};

    fn candidate(
        offer: u32,
        wanted: u32,
        issue: u32,
        offerer: u32,
        wanted_by_user: u32,
    ) -> CandidateItem {
        CandidateItem {
            offer_entry_id: offer,
            wanted_entry_id: wanted,
            issue_id: issue,
            offered_by_user_id: offerer,
            wanted_by_user_id: wanted_by_user,
            issue_number: issue,
            title: format!("Issue {issue}"),
            series_name: "Series".to_string(),
            edition_label: None,
            wanted_edition_label: None,
        }
    }

    fn candidate_with_editions(
        offer: u32,
        wanted: u32,
        offer_edition: Option<&str>,
        wanted_edition: Option<&str>,
    ) -> CandidateItem {
        let mut item = candidate(offer, wanted, 3, 4, 5);
        item.edition_label = offer_edition.map(str::to_string);
        item.wanted_edition_label = wanted_edition.map(str::to_string);
        item
    }

    #[test]
    fn user_pairs_are_normalized_and_self_pairs_rejected() {
        assert_eq!(normalize_user_pair(2, 7), Some((2, 7)));
        assert_eq!(normalize_user_pair(7, 2), Some((2, 7)));
        assert_eq!(normalize_user_pair(2, 2), None);
    }

    #[test]
    fn fingerprints_are_stable_for_sorted_candidates_and_change_with_items() {
        let first = candidate(1, 2, 3, 4, 5);
        let second = candidate(6, 7, 8, 5, 4);
        assert_eq!(
            fingerprint_items(&[first.clone(), second.clone()]),
            fingerprint_items(&[first.clone(), second.clone()])
        );
        assert_ne!(
            fingerprint_items(std::slice::from_ref(&first)),
            fingerprint_items(&[first, second])
        );

        let mut edition_changed = candidate(1, 2, 3, 4, 5);
        edition_changed.edition_label = Some("Variantcover".to_string());
        assert_ne!(
            fingerprint_items(&[candidate(1, 2, 3, 4, 5)]),
            fingerprint_items(&[edition_changed])
        );
    }

    #[test]
    fn candidate_selection_is_deterministic_and_one_to_one() {
        let candidates = vec![
            candidate(11, 21, 3, 4, 5),
            candidate(10, 20, 3, 4, 5),
            candidate(11, 20, 3, 4, 5),
            candidate(10, 21, 3, 4, 5),
        ];
        let mut reversed = candidates.clone();
        reversed.reverse();

        let selected = select_one_to_one_candidates(candidates);
        assert_eq!(selected, select_one_to_one_candidates(reversed));
        assert_eq!(selected.len(), 2);
        assert_eq!(
            selected
                .iter()
                .map(|item| (item.offer_entry_id, item.wanted_entry_id))
                .collect::<Vec<_>>(),
            vec![(10, 20), (11, 21)]
        );
    }

    #[test]
    fn candidate_selection_preserves_specific_and_generic_wishes() {
        let candidates = vec![
            candidate_with_editions(10, 20, Some("A"), None),
            candidate_with_editions(10, 21, Some("A"), Some("A")),
            candidate_with_editions(11, 20, Some("B"), None),
        ];

        let selected = select_one_to_one_candidates(candidates);
        assert_eq!(
            selected
                .iter()
                .map(|item| (item.offer_entry_id, item.wanted_entry_id))
                .collect::<Vec<_>>(),
            vec![(10, 21), (11, 20)]
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn matching_trade_and_messaging_lifecycle_works_against_mariadb() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        crate::db::migrate_test_database(&pool)
            .await
            .expect("test migrations must succeed");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let first_user_id = insert_user(
            &pool,
            &format!("matching-first-{suffix}@example.test"),
            "First Collector",
        )
        .await;
        let second_user_id = insert_user(
            &pool,
            &format!("matching-second-{suffix}@example.test"),
            "Second Collector",
        )
        .await;
        let series_id = insert_series(&pool, suffix).await;
        let first_issue_id = insert_issue(&pool, series_id, 1, "First Direction").await;
        let second_issue_id = insert_issue(&pool, series_id, 2, "Second Direction").await;
        let first_offer_id = insert_entry(
            &pool,
            first_user_id,
            first_issue_id,
            "duplicate",
            Some("Z1"),
        )
        .await;
        let second_wanted_id =
            insert_entry(&pool, second_user_id, first_issue_id, "wanted", None).await;
        let second_offer_id = insert_entry(
            &pool,
            second_user_id,
            second_issue_id,
            "duplicate",
            Some("Z2"),
        )
        .await;
        let first_wanted_id =
            insert_entry(&pool, first_user_id, second_issue_id, "wanted", None).await;

        sqlx::query("UPDATE collection_entries SET edition_label = '1. Auflage' WHERE id = ?")
            .bind(first_offer_id)
            .execute(&pool)
            .await
            .expect("offered edition must be assigned");
        sqlx::query("UPDATE collection_entries SET edition_label = '2. Auflage' WHERE id = ?")
            .bind(second_wanted_id)
            .execute(&pool)
            .await
            .expect("wanted edition mismatch must be assigned");
        sqlx::query("UPDATE collection_entries SET edition_label = 'Variantcover' WHERE id = ?")
            .bind(second_offer_id)
            .execute(&pool)
            .await
            .expect("second offered edition must be assigned");
        let edition_mismatch = reconcile(&pool, first_user_id).await;
        assert_eq!(edition_mismatch.created, 0);

        sqlx::query("UPDATE collection_entries SET edition_label = '1. Auflage' WHERE id = ?")
            .bind(second_wanted_id)
            .execute(&pool)
            .await
            .expect("wanted edition must be made compatible");
        let created = reconcile(&pool, first_user_id).await;
        assert_eq!(created.created, 1);
        let page = PageParams {
            page: 1,
            per_page: 20,
        };
        let matches = matching_service::list_matches(&pool, first_user_id, &page)
            .await
            .expect("matches must be listed");
        assert_eq!(matches.total, 1);
        assert_eq!(matches.data[0].match_score, 100);
        assert_eq!(matches.data[0].my_offers[0].entry_id, first_offer_id);
        assert_eq!(matches.data[0].partner_offers[0].entry_id, second_offer_id);
        assert_eq!(
            matches.data[0].my_offers[0].edition_label.as_deref(),
            Some("1. Auflage")
        );
        assert_eq!(
            matches.data[0].my_offers[0].wanted_edition_label.as_deref(),
            Some("1. Auflage")
        );
        let match_id = matches.data[0].id;

        let unchanged = reconcile(&pool, first_user_id).await;
        assert_eq!(unchanged.created, 0);
        assert_eq!(unchanged.updated, 0);
        assert_eq!(notification_count(&pool, first_user_id).await, 1);
        assert_eq!(notification_count(&pool, second_user_id).await, 1);

        sqlx::query("UPDATE collection_entries SET status = 'owned' WHERE id = ?")
            .bind(first_offer_id)
            .execute(&pool)
            .await
            .expect("offer must become owned");
        let stale = reconcile(&pool, first_user_id).await;
        assert_eq!(stale.staled, 1);
        assert_eq!(
            matching_service::list_matches(&pool, first_user_id, &page)
                .await
                .expect("stale matches must be hidden")
                .total,
            0
        );

        sqlx::query("UPDATE collection_entries SET status = 'duplicate' WHERE id = ?")
            .bind(first_offer_id)
            .execute(&pool)
            .await
            .expect("offer must become duplicate again");
        let reactivated = reconcile(&pool, first_user_id).await;
        assert_eq!(reactivated.reactivated, 1);
        assert_eq!(notification_count(&pool, first_user_id).await, 2);
        assert_eq!(notification_count(&pool, second_user_id).await, 2);
        let reactivated_match = matching_service::get_match(&pool, first_user_id, match_id)
            .await
            .expect("reactivated match must be readable");
        assert_eq!(reactivated_match.revision, 3);

        let mut proposal = trades::create_proposal(
            &pool,
            first_user_id,
            match_id,
            &CreateTradeProposalRequest {
                offered_entry_ids: vec![first_offer_id],
                requested_entry_ids: vec![second_offer_id],
            },
        )
        .await
        .expect("proposal must be created");
        assert_eq!(proposal.status, "proposed");
        assert_eq!(
            proposal.my_offers[0].wanted_entry_id,
            Some(second_wanted_id)
        );
        assert_eq!(
            proposal.partner_offers[0].wanted_entry_id,
            Some(first_wanted_id)
        );

        let duplicate_proposal = trades::create_proposal(
            &pool,
            first_user_id,
            match_id,
            &CreateTradeProposalRequest {
                offered_entry_ids: vec![first_offer_id],
                requested_entry_ids: vec![second_offer_id],
            },
        )
        .await;
        assert!(matches!(
            duplicate_proposal,
            Err(AppError::ConflictWithCode { ref code, .. }) if code == "open_trade_exists"
        ));

        matching_service::prepare_entry_mutation(&pool, first_user_id, first_offer_id)
            .await
            .expect("a relevant mutation must cancel a proposal");
        assert_eq!(trade_status(&pool, proposal.id).await, "cancelled");
        assert_eq!(
            trade_notification_count(&pool, second_user_id, proposal.id, "trade_cancelled").await,
            1
        );

        let stale_proposal = trades::create_proposal(
            &pool,
            first_user_id,
            match_id,
            &CreateTradeProposalRequest {
                offered_entry_ids: vec![first_offer_id],
                requested_entry_ids: vec![second_offer_id],
            },
        )
        .await
        .expect("a replacement proposal must be created");
        sqlx::query("UPDATE collection_entries SET status = 'owned' WHERE id = ?")
            .bind(first_offer_id)
            .execute(&pool)
            .await
            .expect("offer must become owned");
        assert_eq!(reconcile(&pool, first_user_id).await.staled, 1);
        assert_eq!(trade_status(&pool, stale_proposal.id).await, "cancelled");
        assert_eq!(
            trade_notification_count(&pool, second_user_id, stale_proposal.id, "trade_cancelled")
                .await,
            1
        );
        sqlx::query("UPDATE collection_entries SET status = 'duplicate' WHERE id = ?")
            .bind(first_offer_id)
            .execute(&pool)
            .await
            .expect("offer must become duplicate again");
        assert_eq!(reconcile(&pool, first_user_id).await.reactivated, 1);
        proposal = trades::create_proposal(
            &pool,
            first_user_id,
            match_id,
            &CreateTradeProposalRequest {
                offered_entry_ids: vec![first_offer_id],
                requested_entry_ids: vec![second_offer_id],
            },
        )
        .await
        .expect("the lifecycle proposal must be recreated");

        let proposal_notification_id = sqlx::query_scalar::<_, u32>(
            "SELECT id FROM notifications
             WHERE user_id = ? AND trade_id = ? AND kind = 'trade_proposed'",
        )
        .bind(second_user_id)
        .bind(proposal.id)
        .fetch_one(&pool)
        .await
        .expect("proposal notification must exist");
        assert!(
            crate::db::notifications::mark_notification_read(
                &pool,
                second_user_id,
                proposal_notification_id
            )
            .await
            .expect("first read update must succeed")
        );
        assert!(
            crate::db::notifications::mark_notification_read(
                &pool,
                second_user_id,
                proposal_notification_id
            )
            .await
            .expect("idempotent read update must succeed")
        );
        assert!(
            !crate::db::notifications::mark_notification_read(
                &pool,
                first_user_id,
                proposal_notification_id
            )
            .await
            .expect("foreign notification lookup must succeed")
        );

        let request = SendMessageRequest {
            client_message_id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            content: "  Versand als BüWa?  ".to_string(),
        };
        let (sent, created) =
            messaging::send_message(&pool, first_user_id, proposal.thread_id, &request)
                .await
                .expect("message must be sent");
        assert!(created);
        assert_eq!(sent.content, "Versand als BüWa?");
        let (same_message, created_again) =
            messaging::send_message(&pool, first_user_id, proposal.thread_id, &request)
                .await
                .expect("idempotent retry must succeed");
        assert!(!created_again);
        assert_eq!(same_message.id, sent.id);

        let inbox = messaging::list_messages(
            &pool,
            second_user_id,
            proposal.thread_id,
            &MessagePageParams {
                before_id: None,
                limit: 50,
            },
        )
        .await
        .expect("recipient must read the message");
        assert_eq!(inbox.data.len(), 1);
        assert!(!inbox.data[0].is_mine);
        messaging::mark_thread_read(&pool, second_user_id, proposal.thread_id, sent.id)
            .await
            .expect("message must be marked read");

        let accepted = trades::accept_trade(&pool, second_user_id, proposal.id)
            .await
            .expect("recipient must accept the trade");
        assert_eq!(accepted.status, "accepted");
        let mutation =
            matching_service::prepare_entry_mutation(&pool, first_user_id, first_offer_id).await;
        assert!(matches!(
            mutation,
            Err(AppError::ConflictWithCode { ref code, .. }) if code == "entry_reserved_by_trade"
        ));

        trades::cancel_trade(&pool, first_user_id, proposal.id)
            .await
            .expect("either participant must be able to cancel");
        matching_service::prepare_entry_mutation(&pool, first_user_id, first_offer_id)
            .await
            .expect("cancelled trades must release entries");
        assert_eq!(
            messaging::list_messages(
                &pool,
                second_user_id,
                proposal.thread_id,
                &MessagePageParams {
                    before_id: None,
                    limit: 50,
                },
            )
            .await
            .expect("cancelled trade messages must be retained")
            .data
            .len(),
            1
        );

        let completion_proposal = trades::create_proposal(
            &pool,
            first_user_id,
            match_id,
            &CreateTradeProposalRequest {
                offered_entry_ids: vec![first_offer_id],
                requested_entry_ids: vec![second_offer_id],
            },
        )
        .await
        .expect("completion proposal must be created");
        assert_eq!(
            completion_proposal.my_offers[0].edition_label.as_deref(),
            Some("1. Auflage")
        );
        assert_eq!(
            completion_proposal.partner_offers[0]
                .edition_label
                .as_deref(),
            Some("Variantcover")
        );
        trades::accept_trade(&pool, second_user_id, completion_proposal.id)
            .await
            .expect("completion proposal must be accepted");

        let photo_storage_key = format!("matching-completion-{suffix}.jpg");
        sqlx::query(
            "INSERT INTO collection_photos
			    (entry_id, storage_key, media_type, byte_size, width, height, sort_order)
			 VALUES (?, ?, 'image/jpeg', 10, 1, 1, 0)",
        )
        .bind(first_offer_id)
        .bind(&photo_storage_key)
        .execute(&pool)
        .await
        .expect("offer photo fixture must be inserted");

        let first_confirmation =
            trades::complete_trade(&pool, first_user_id, completion_proposal.id)
                .await
                .expect("first completion confirmation must succeed");
        assert_eq!(first_confirmation.trade.status, "accepted");
        assert!(
            first_confirmation
                .trade
                .my_completion_confirmed_at
                .is_some()
        );
        assert!(
            first_confirmation
                .trade
                .partner_completion_confirmed_at
                .is_none()
        );
        assert!(first_confirmation.photo_storage_keys.is_empty());
        assert_eq!(collection_entry_count(&pool, first_offer_id).await, 1);
        assert_eq!(collection_entry_count(&pool, second_offer_id).await, 1);

        trades::complete_trade(&pool, first_user_id, completion_proposal.id)
            .await
            .expect("repeated first confirmation must be idempotent");
        assert_eq!(
            completion_confirmation_count(&pool, completion_proposal.id).await,
            1
        );
        assert_eq!(
            trade_notification_count(
                &pool,
                second_user_id,
                completion_proposal.id,
                "trade_completion_confirmed",
            )
            .await,
            1
        );

        sqlx::query("UPDATE collection_entries SET edition_label = 'Verändert' WHERE id = ?")
            .bind(first_offer_id)
            .execute(&pool)
            .await
            .expect("accepted offer must be changed for rollback test");
        let changed_completion =
            trades::complete_trade(&pool, second_user_id, completion_proposal.id).await;
        assert!(matches!(
            changed_completion,
            Err(AppError::ConflictWithCode { ref code, .. }) if code == "trade_items_changed"
        ));
        assert_eq!(
            trade_status(&pool, completion_proposal.id).await,
            "accepted"
        );
        assert_eq!(
            completion_confirmation_count(&pool, completion_proposal.id).await,
            1
        );
        assert_eq!(collection_entry_count(&pool, first_offer_id).await, 1);

        sqlx::query("UPDATE collection_entries SET edition_label = '1. Auflage' WHERE id = ?")
            .bind(first_offer_id)
            .execute(&pool)
            .await
            .expect("accepted offer edition must be restored");
        let completed = trades::complete_trade(&pool, second_user_id, completion_proposal.id)
            .await
            .expect("second confirmation must complete the trade");
        assert_eq!(completed.trade.status, "completed");
        assert!(completed.trade.completed_at.is_some());
        assert!(completed.trade.my_completion_confirmed_at.is_some());
        assert!(completed.trade.partner_completion_confirmed_at.is_some());
        assert_eq!(
            completed.photo_storage_keys,
            vec![photo_storage_key.clone()]
        );
        assert_eq!(collection_entry_count(&pool, first_offer_id).await, 0);
        assert_eq!(collection_entry_count(&pool, second_offer_id).await, 0);
        assert_eq!(
            collection_entry_state(&pool, second_wanted_id).await,
            (
                "owned".to_string(),
                Some("Z1".to_string()),
                Some("1. Auflage".to_string())
            )
        );
        assert_eq!(
            collection_entry_state(&pool, first_wanted_id).await,
            (
                "owned".to_string(),
                Some("Z2".to_string()),
                Some("Variantcover".to_string())
            )
        );
        assert_eq!(media_deletion_job_count(&pool, &photo_storage_key).await, 1);
        assert_eq!(
            trade_notification_count(
                &pool,
                first_user_id,
                completion_proposal.id,
                "trade_completed",
            )
            .await,
            1
        );

        let open = trades::list_trades(
            &pool,
            first_user_id,
            &TradePageParams {
                scope: None,
                page: 1,
                per_page: 20,
            },
        )
        .await
        .expect("open trades must be listed");
        assert!(
            !open
                .data
                .iter()
                .any(|trade| trade.id == completion_proposal.id)
        );
        let history = trades::list_trades(
            &pool,
            first_user_id,
            &TradePageParams {
                scope: Some("closed".to_string()),
                page: 1,
                per_page: 20,
            },
        )
        .await
        .expect("trade history must be listed");
        assert!(
            history
                .data
                .iter()
                .any(|trade| { trade.id == completion_proposal.id && trade.status == "completed" })
        );
        assert!(history.data.iter().any(|trade| trade.id == proposal.id));

        let repeated_completion =
            trades::complete_trade(&pool, first_user_id, completion_proposal.id)
                .await
                .expect("completed trade retry must be idempotent");
        assert_eq!(repeated_completion.trade.status, "completed");
        assert!(repeated_completion.photo_storage_keys.is_empty());
        let completed_cancellation =
            trades::cancel_trade(&pool, first_user_id, completion_proposal.id).await;
        assert!(matches!(
            completed_cancellation,
            Err(AppError::ConflictWithCode { ref code, .. }) if code == "invalid_trade_transition"
        ));
        assert_eq!(
            messaging::list_messages(
                &pool,
                second_user_id,
                completion_proposal.thread_id,
                &MessagePageParams {
                    before_id: None,
                    limit: 50,
                },
            )
            .await
            .expect("completed trade messages must be retained")
            .data
            .len(),
            0
        );

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(first_user_id)
            .execute(&pool)
            .await
            .expect("account deletion must cascade");
        assert_eq!(row_count(&pool, "trade_matches", match_id).await, 0);
        assert_eq!(row_count(&pool, "trades", proposal.id).await, 0);
        assert_eq!(row_count(&pool, "trades", completion_proposal.id).await, 0);
        assert_eq!(
            row_count(&pool, "message_threads", proposal.thread_id).await,
            0
        );
        assert_eq!(row_count(&pool, "messages", sent.id).await, 0);

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(second_user_id)
            .execute(&pool)
            .await
            .expect("second user fixture must be deleted");
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series fixture must be deleted");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn complementary_collection_mutations_are_serialized_before_reconciliation() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        crate::db::migrate_test_database(&pool)
            .await
            .expect("test migrations must succeed");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let first_user_id = insert_user(
            &pool,
            &format!("matching-concurrent-first-{suffix}@example.test"),
            "Concurrent First",
        )
        .await;
        let second_user_id = insert_user(
            &pool,
            &format!("matching-concurrent-second-{suffix}@example.test"),
            "Concurrent Second",
        )
        .await;
        let series_id = insert_series(&pool, suffix.saturating_add(1)).await;
        let first_issue_id = insert_issue(&pool, series_id, 1, "Concurrent First").await;
        let second_issue_id = insert_issue(&pool, series_id, 2, "Concurrent Second").await;
        insert_entry(
            &pool,
            second_user_id,
            second_issue_id,
            "duplicate",
            Some("Z2"),
        )
        .await;
        insert_entry(&pool, first_user_id, second_issue_id, "wanted", None).await;

        let barrier = Arc::new(Barrier::new(2));
        let first_pool = pool.clone();
        let first_barrier = barrier.clone();
        let first = async move {
            let mut transaction = first_pool.begin().await?;
            first_barrier.wait().await;
            lock_reconciliation_users_for_issues(
                &mut transaction,
                first_user_id,
                &[first_issue_id],
            )
            .await?;
            crate::db::collection::add_entry_on_connection(
                &mut transaction,
                crate::db::collection::NewCollectionEntry {
                    user_id: first_user_id,
                    issue_id: first_issue_id,
                    copy_number: 1,
                    condition_grade: Some("Z1"),
                    status: "duplicate",
                    notes: None,
                    edition_label: None,
                },
            )
            .await?;
            reconcile_user_matches(&mut transaction, first_user_id).await?;
            transaction.commit().await
        };
        let second_pool = pool.clone();
        let second_barrier = barrier.clone();
        let second = async move {
            let mut transaction = second_pool.begin().await?;
            second_barrier.wait().await;
            lock_reconciliation_users_for_issues(
                &mut transaction,
                second_user_id,
                &[first_issue_id],
            )
            .await?;
            crate::db::collection::add_entry_on_connection(
                &mut transaction,
                crate::db::collection::NewCollectionEntry {
                    user_id: second_user_id,
                    issue_id: first_issue_id,
                    copy_number: 1,
                    condition_grade: None,
                    status: "wanted",
                    notes: None,
                    edition_label: None,
                },
            )
            .await?;
            reconcile_user_matches(&mut transaction, second_user_id).await?;
            transaction.commit().await
        };

        let (first_result, second_result): (Result<(), sqlx::Error>, Result<(), sqlx::Error>) =
            tokio::join!(first, second);
        first_result.expect("first concurrent mutation must commit");
        second_result.expect("second concurrent mutation must commit");
        let match_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM trade_matches
             WHERE user_low_id = LEAST(?, ?) AND user_high_id = GREATEST(?, ?)
               AND status = 'active'",
        )
        .bind(first_user_id)
        .bind(second_user_id)
        .bind(first_user_id)
        .bind(second_user_id)
        .fetch_one(&pool)
        .await
        .expect("match must be countable");
        assert_eq!(match_count, 1);

        sqlx::query("DELETE FROM users WHERE id IN (?, ?)")
            .bind(first_user_id)
            .bind(second_user_id)
            .execute(&pool)
            .await
            .expect("user fixtures must be deleted");
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series fixture must be deleted");
    }

    async fn reconcile(pool: &MySqlPool, user_id: u32) -> ReconciliationStats {
        let mut transaction = pool.begin().await.expect("transaction must start");
        let stats = reconcile_user_matches(&mut transaction, user_id)
            .await
            .expect("matching reconciliation must succeed");
        transaction.commit().await.expect("transaction must commit");
        stats
    }

    async fn insert_user(pool: &MySqlPool, email: &str, name: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO users (email, display_name) VALUES (?, ?)")
                .bind(email)
                .bind(name)
                .execute(pool)
                .await
                .expect("user fixture must be inserted"),
        )
    }

    async fn insert_series(pool: &MySqlPool, suffix: u128) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO series (name, slug, active) VALUES (?, ?, TRUE)")
                .bind(format!("Matching Test {suffix}"))
                .bind(format!("matching-test-{suffix}"))
                .execute(pool)
                .await
                .expect("series fixture must be inserted"),
        )
    }

    async fn insert_issue(pool: &MySqlPool, series_id: u32, issue_number: u32, title: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)")
                .bind(series_id)
                .bind(issue_number)
                .bind(title)
                .execute(pool)
                .await
                .expect("issue fixture must be inserted"),
        )
    }

    async fn insert_entry(
        pool: &MySqlPool,
        user_id: u32,
        issue_id: u32,
        status: &str,
        condition: Option<&str>,
    ) -> u32 {
        inserted_id(
            &sqlx::query(
                "INSERT INTO collection_entries
                    (user_id, issue_id, copy_number, condition_grade, status)
                 VALUES (?, ?, 1, ?, ?)",
            )
            .bind(user_id)
            .bind(issue_id)
            .bind(condition)
            .bind(status)
            .execute(pool)
            .await
            .expect("collection entry fixture must be inserted"),
        )
    }

    async fn notification_count(pool: &MySqlPool, user_id: u32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .expect("notifications must be countable")
    }

    async fn trade_notification_count(
        pool: &MySqlPool,
        user_id: u32,
        trade_id: u32,
        kind: &str,
    ) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications
             WHERE user_id = ? AND trade_id = ? AND kind = ?",
        )
        .bind(user_id)
        .bind(trade_id)
        .bind(kind)
        .fetch_one(pool)
        .await
        .expect("trade notifications must be countable")
    }

    async fn trade_status(pool: &MySqlPool, trade_id: u32) -> String {
        sqlx::query_scalar("SELECT status FROM trades WHERE id = ?")
            .bind(trade_id)
            .fetch_one(pool)
            .await
            .expect("trade status must be readable")
    }

    async fn completion_confirmation_count(pool: &MySqlPool, trade_id: u32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM trade_completion_confirmations WHERE trade_id = ?")
            .bind(trade_id)
            .fetch_one(pool)
            .await
            .expect("completion confirmations must be countable")
    }

    async fn collection_entry_count(pool: &MySqlPool, entry_id: u32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM collection_entries WHERE id = ?")
            .bind(entry_id)
            .fetch_one(pool)
            .await
            .expect("collection entry must be countable")
    }

    async fn collection_entry_state(
        pool: &MySqlPool,
        entry_id: u32,
    ) -> (String, Option<String>, Option<String>) {
        sqlx::query_as(
            "SELECT status, condition_grade, edition_label
			 FROM collection_entries WHERE id = ?",
        )
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .expect("collection entry state must be readable")
    }

    async fn media_deletion_job_count(pool: &MySqlPool, storage_key: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM media_deletion_jobs WHERE storage_key = ?")
            .bind(storage_key)
            .fetch_one(pool)
            .await
            .expect("media deletion job must be countable")
    }

    async fn row_count(pool: &MySqlPool, table: &str, id: u32) -> i64 {
        match table {
            "trade_matches" => {
                sqlx::query_scalar("SELECT COUNT(*) FROM trade_matches WHERE id = ?")
                    .bind(id)
                    .fetch_one(pool)
                    .await
            }
            "trades" => {
                sqlx::query_scalar("SELECT COUNT(*) FROM trades WHERE id = ?")
                    .bind(id)
                    .fetch_one(pool)
                    .await
            }
            "message_threads" => {
                sqlx::query_scalar("SELECT COUNT(*) FROM message_threads WHERE id = ?")
                    .bind(id)
                    .fetch_one(pool)
                    .await
            }
            "messages" => {
                sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = ?")
                    .bind(id)
                    .fetch_one(pool)
                    .await
            }
            _ => panic!("unsupported fixture table: {table}"),
        }
        .expect("fixture rows must be countable")
    }

    fn inserted_id(result: &sqlx::mysql::MySqlQueryResult) -> u32 {
        result
            .last_insert_id()
            .try_into()
            .expect("fixture ID must fit into u32")
    }
}
