use serde_json::json;
use sqlx::{MySql, MySqlConnection, MySqlPool, QueryBuilder, Transaction};

use crate::db::notifications;
use crate::models::trade_matching::{TradeItemViewRow, TradeListRow, TradePageParams, TradeRecord};

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct MatchForProposalRow {
    pub id: u32,
    pub user_low_id: u32,
    pub user_high_id: u32,
    pub status: String,
    pub open_trade_id: Option<u32>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProposalItemRow {
    pub offer_entry_id: u32,
    pub wanted_entry_id: u32,
    pub issue_id: u32,
    pub offered_by_user_id: u32,
    pub wanted_by_user_id: u32,
    pub copy_number: u8,
    pub edition_label: Option<String>,
    pub wanted_edition_label: Option<String>,
    pub condition_grade: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProposalCancellationRow {
    pub id: u32,
    pub match_id: u32,
    pub initiator_id: u32,
    pub responder_id: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CompletionItemRow {
    pub offer_entry_id: Option<u32>,
    pub wanted_entry_id: Option<u32>,
    pub issue_id: u32,
    pub offered_by_user_id: u32,
    pub receiving_user_id: u32,
    pub condition_grade_snapshot: String,
    pub edition_label_snapshot: Option<String>,
    pub wanted_edition_label_snapshot: Option<String>,
    pub offer_user_id: Option<u32>,
    pub offer_issue_id: Option<u32>,
    pub offer_status: Option<String>,
    pub offer_condition_grade: Option<String>,
    pub offer_edition_label: Option<String>,
    pub wanted_user_id: Option<u32>,
    pub wanted_issue_id: Option<u32>,
    pub wanted_status: Option<String>,
    pub wanted_edition_label: Option<String>,
}

pub async fn lock_match_for_participant(
    transaction: &mut Transaction<'_, MySql>,
    match_id: u32,
    user_id: u32,
) -> Result<Option<MatchForProposalRow>, sqlx::Error> {
    sqlx::query_as::<_, MatchForProposalRow>(
        "SELECT m.id, m.user_low_id, m.user_high_id, m.status,
                ot.id AS open_trade_id
         FROM trade_matches m
         LEFT JOIN trades ot ON ot.open_match_id = m.id
         WHERE m.id = ? AND (m.user_low_id = ? OR m.user_high_id = ?)
         FOR UPDATE",
    )
    .bind(match_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn find_proposal_items_for_update(
    transaction: &mut Transaction<'_, MySql>,
    match_id: u32,
) -> Result<Vec<ProposalItemRow>, sqlx::Error> {
    sqlx::query_as::<_, ProposalItemRow>(
        "SELECT mi.offer_entry_id, mi.wanted_entry_id, mi.issue_id,
                mi.offered_by_user_id, mi.wanted_by_user_id,
                offer.copy_number, offer.edition_label,
                wanted.edition_label AS wanted_edition_label,
                offer.condition_grade
         FROM trade_match_items mi
         JOIN collection_entries offer ON offer.id = mi.offer_entry_id
         JOIN collection_entries wanted ON wanted.id = mi.wanted_entry_id
         WHERE mi.match_id = ? AND offer.status = 'duplicate'
           AND wanted.status = 'wanted'
         ORDER BY mi.offer_entry_id, mi.wanted_entry_id
         FOR UPDATE",
    )
    .bind(match_id)
    .fetch_all(&mut **transaction)
    .await
}

pub async fn insert_trade(
    connection: &mut MySqlConnection,
    match_id: u32,
    initiator_id: u32,
    responder_id: u32,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO trades
            (match_id, initiator_id, responder_id, status, open_match_id)
         VALUES (?, ?, ?, 'proposed', ?)",
    )
    .bind(match_id)
    .bind(initiator_id)
    .bind(responder_id)
    .bind(match_id)
    .execute(connection)
    .await?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

pub async fn insert_trade_item(
    connection: &mut MySqlConnection,
    trade_id: u32,
    item: &ProposalItemRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO trade_items
            (trade_id, offer_entry_id, wanted_entry_id, issue_id,
             offered_by_user_id, receiving_user_id, copy_number_snapshot,
             edition_label_snapshot, wanted_edition_label_snapshot,
             condition_grade_snapshot)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(trade_id)
    .bind(item.offer_entry_id)
    .bind(item.wanted_entry_id)
    .bind(item.issue_id)
    .bind(item.offered_by_user_id)
    .bind(item.wanted_by_user_id)
    .bind(item.copy_number)
    .bind(&item.edition_label)
    .bind(&item.wanted_edition_label)
    .bind(&item.condition_grade)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn insert_message_thread(
    connection: &mut MySqlConnection,
    trade_id: u32,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query("INSERT INTO message_threads (trade_id) VALUES (?)")
        .bind(trade_id)
        .execute(connection)
        .await?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

pub async fn lock_trade_for_participant(
    transaction: &mut Transaction<'_, MySql>,
    trade_id: u32,
    user_id: u32,
) -> Result<Option<TradeRecord>, sqlx::Error> {
    sqlx::query_as::<_, TradeRecord>(
        "SELECT id, match_id, initiator_id, responder_id, status,
                cancellation_reason, proposed_at, accepted_at, cancelled_at,
                completed_at, created_at, updated_at
         FROM trades
         WHERE id = ? AND (initiator_id = ? OR responder_id = ?)
         FOR UPDATE",
    )
    .bind(trade_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn lock_trade_entry_references(
    transaction: &mut Transaction<'_, MySql>,
    trade_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, u32>(
        "SELECT ce.id
         FROM collection_entries ce
         JOIN trade_items ti ON ti.offer_entry_id = ce.id
         WHERE ti.trade_id = ?
         ORDER BY ce.id FOR UPDATE",
    )
    .bind(trade_id)
    .fetch_all(&mut **transaction)
    .await?;
    sqlx::query_scalar::<_, u32>(
        "SELECT ce.id
         FROM collection_entries ce
         JOIN trade_items ti ON ti.wanted_entry_id = ce.id
         WHERE ti.trade_id = ?
         ORDER BY ce.id FOR UPDATE",
    )
    .bind(trade_id)
    .fetch_all(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn find_trade_issue_ids(
    connection: &mut MySqlConnection,
    trade_id: u32,
) -> Result<Vec<u32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT issue_id FROM trade_items
         WHERE trade_id = ? ORDER BY issue_id",
    )
    .bind(trade_id)
    .fetch_all(connection)
    .await
}

pub async fn find_completion_items(
    connection: &mut MySqlConnection,
    trade_id: u32,
) -> Result<Vec<CompletionItemRow>, sqlx::Error> {
    sqlx::query_as::<_, CompletionItemRow>(
        "SELECT ti.offer_entry_id, ti.wanted_entry_id, ti.issue_id,
                ti.offered_by_user_id, ti.receiving_user_id,
                ti.condition_grade_snapshot, ti.edition_label_snapshot,
                ti.wanted_edition_label_snapshot,
                offer.user_id AS offer_user_id, offer.issue_id AS offer_issue_id,
                offer.status AS offer_status,
                offer.condition_grade AS offer_condition_grade,
                offer.edition_label AS offer_edition_label,
                wanted.user_id AS wanted_user_id, wanted.issue_id AS wanted_issue_id,
                wanted.status AS wanted_status,
                wanted.edition_label AS wanted_edition_label
         FROM trade_items ti
         LEFT JOIN collection_entries offer ON offer.id = ti.offer_entry_id
         LEFT JOIN collection_entries wanted ON wanted.id = ti.wanted_entry_id
         WHERE ti.trade_id = ?
         ORDER BY ti.id",
    )
    .bind(trade_id)
    .fetch_all(connection)
    .await
}

pub async fn insert_completion_confirmation(
    connection: &mut MySqlConnection,
    trade_id: u32,
    user_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT IGNORE INTO trade_completion_confirmations (trade_id, user_id)
         VALUES (?, ?)",
    )
    .bind(trade_id)
    .bind(user_id)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn count_completion_confirmations(
    connection: &mut MySqlConnection,
    trade_id: u32,
) -> Result<u8, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trade_completion_confirmations WHERE trade_id = ?",
    )
    .bind(trade_id)
    .fetch_one(connection)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u8)
}

pub async fn update_wanted_entry_to_owned(
    connection: &mut MySqlConnection,
    entry_id: u32,
    receiving_user_id: u32,
    condition_grade: &str,
    edition_label: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE collection_entries
         SET status = 'owned', condition_grade = ?, edition_label = ?
         WHERE id = ? AND user_id = ? AND status = 'wanted'",
    )
    .bind(condition_grade)
    .bind(edition_label)
    .bind(entry_id)
    .bind(receiving_user_id)
    .execute(connection)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn mark_trade_completed(
    connection: &mut MySqlConnection,
    trade_id: u32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE trades
         SET status = 'completed', open_match_id = NULL,
             completed_at = COALESCE(completed_at, CURRENT_TIMESTAMP)
         WHERE id = ? AND status = 'accepted'",
    )
    .bind(trade_id)
    .execute(connection)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn trade_has_reservation_conflict(
    connection: &mut MySqlConnection,
    trade_id: u32,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM trade_items current_item
         JOIN trade_items reserved_item
           ON reserved_item.trade_id <> current_item.trade_id
          AND (
              reserved_item.offer_entry_id = current_item.offer_entry_id
              OR reserved_item.offer_entry_id = current_item.wanted_entry_id
              OR reserved_item.wanted_entry_id = current_item.offer_entry_id
              OR reserved_item.wanted_entry_id = current_item.wanted_entry_id
          )
         JOIN trades reserved_trade
           ON reserved_trade.id = reserved_item.trade_id
          AND reserved_trade.status = 'accepted'
         WHERE current_item.trade_id = ?",
    )
    .bind(trade_id)
    .fetch_one(connection)
    .await?
        > 0)
}

pub async fn proposal_item_is_reserved(
    connection: &mut MySqlConnection,
    item: &ProposalItemRow,
) -> Result<bool, sqlx::Error> {
    Ok(
        entry_is_reserved(&mut *connection, item.offer_entry_id).await?
            || entry_is_reserved(&mut *connection, item.wanted_entry_id).await?,
    )
}

pub async fn accept_trade(
    connection: &mut MySqlConnection,
    trade_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE trades SET status = 'accepted', accepted_at = CURRENT_TIMESTAMP
         WHERE id = ? AND status = 'proposed'",
    )
    .bind(trade_id)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn cancel_trade(
    connection: &mut MySqlConnection,
    trade_id: u32,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE trades
         SET status = 'cancelled', open_match_id = NULL,
             cancellation_reason = ?, cancelled_at = CURRENT_TIMESTAMP
         WHERE id = ? AND status IN ('proposed', 'accepted')",
    )
    .bind(reason)
    .bind(trade_id)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn count_trades(
    pool: &MySqlPool,
    user_id: u32,
    params: &TradePageParams,
) -> Result<u32, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trades
         WHERE ((? = 'open' AND status IN ('proposed', 'accepted'))
                OR (? = 'closed' AND status IN ('completed', 'cancelled')))
           AND (initiator_id = ? OR responder_id = ?)",
    )
    .bind(params.scope())
    .bind(params.scope())
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

pub async fn find_trades(
    pool: &MySqlPool,
    user_id: u32,
    params: &TradePageParams,
) -> Result<Vec<TradeListRow>, sqlx::Error> {
    sqlx::query_as::<_, TradeListRow>(
        "SELECT t.id, t.match_id, t.initiator_id, t.responder_id, t.status,
                t.cancellation_reason, t.proposed_at, t.accepted_at,
                t.cancelled_at, t.completed_at,
                (SELECT c.confirmed_at FROM trade_completion_confirmations c
                 WHERE c.trade_id = t.id AND c.user_id = ?) AS my_completion_confirmed_at,
                (SELECT c.confirmed_at FROM trade_completion_confirmations c
                 WHERE c.trade_id = t.id AND c.user_id <> ?
                 ORDER BY c.user_id LIMIT 1) AS partner_completion_confirmed_at,
                t.created_at, t.updated_at,
                partner.id AS partner_id, partner.display_name AS partner_display_name,
                partner.profile_public AS partner_profile_public,
                partner.avatar_path AS partner_avatar_path,
                partner.location AS partner_location, mt.id AS thread_id
         FROM trades t
         JOIN users partner ON partner.id = CASE
             WHEN t.initiator_id = ? THEN t.responder_id ELSE t.initiator_id END
         JOIN message_threads mt ON mt.trade_id = t.id
         WHERE ((? = 'open' AND t.status IN ('proposed', 'accepted'))
                OR (? = 'closed' AND t.status IN ('completed', 'cancelled')))
           AND (t.initiator_id = ? OR t.responder_id = ?)
         ORDER BY t.updated_at DESC, t.id DESC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(params.scope())
    .bind(params.scope())
    .bind(user_id)
    .bind(user_id)
    .bind(params.per_page())
    .bind(params.offset())
    .fetch_all(pool)
    .await
}

pub async fn find_trade_for_participant(
    pool: &MySqlPool,
    user_id: u32,
    trade_id: u32,
) -> Result<Option<TradeListRow>, sqlx::Error> {
    sqlx::query_as::<_, TradeListRow>(
        "SELECT t.id, t.match_id, t.initiator_id, t.responder_id, t.status,
                t.cancellation_reason, t.proposed_at, t.accepted_at,
                t.cancelled_at, t.completed_at,
                (SELECT c.confirmed_at FROM trade_completion_confirmations c
                 WHERE c.trade_id = t.id AND c.user_id = ?) AS my_completion_confirmed_at,
                (SELECT c.confirmed_at FROM trade_completion_confirmations c
                 WHERE c.trade_id = t.id AND c.user_id <> ?
                 ORDER BY c.user_id LIMIT 1) AS partner_completion_confirmed_at,
                t.created_at, t.updated_at,
                partner.id AS partner_id, partner.display_name AS partner_display_name,
                partner.profile_public AS partner_profile_public,
                partner.avatar_path AS partner_avatar_path,
                partner.location AS partner_location, mt.id AS thread_id
         FROM trades t
         JOIN users partner ON partner.id = CASE
             WHEN t.initiator_id = ? THEN t.responder_id ELSE t.initiator_id END
         JOIN message_threads mt ON mt.trade_id = t.id
         WHERE t.id = ? AND (t.initiator_id = ? OR t.responder_id = ?)",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(trade_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_trade_items(
    pool: &MySqlPool,
    trade_id: u32,
) -> Result<Vec<TradeItemViewRow>, sqlx::Error> {
    sqlx::query_as::<_, TradeItemViewRow>(
        "SELECT ti.trade_id, ti.offer_entry_id, ti.wanted_entry_id, ti.issue_id,
                ti.offered_by_user_id, ti.receiving_user_id,
                i.issue_number, i.title, s.id AS series_id,
                s.name AS series_name, s.slug AS series_slug,
                i.cover_url, i.cover_local_path, ti.copy_number_snapshot,
                ti.edition_label_snapshot, ti.wanted_edition_label_snapshot,
                ti.condition_grade_snapshot
         FROM trade_items ti
         JOIN issues i ON i.id = ti.issue_id
         JOIN series s ON s.id = i.series_id
         WHERE ti.trade_id = ?
         ORDER BY ti.offered_by_user_id, s.name, i.issue_number,
                  ti.copy_number_snapshot",
    )
    .bind(trade_id)
    .fetch_all(pool)
    .await
}

pub async fn find_trade_items_for_trades(
    pool: &MySqlPool,
    trade_ids: &[u32],
) -> Result<Vec<TradeItemViewRow>, sqlx::Error> {
    if trade_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT ti.trade_id, ti.offer_entry_id, ti.wanted_entry_id, ti.issue_id,
                ti.offered_by_user_id, ti.receiving_user_id,
                i.issue_number, i.title, s.id AS series_id,
                s.name AS series_name, s.slug AS series_slug,
                i.cover_url, i.cover_local_path, ti.copy_number_snapshot,
                ti.edition_label_snapshot, ti.wanted_edition_label_snapshot,
                ti.condition_grade_snapshot
         FROM trade_items ti
         JOIN issues i ON i.id = ti.issue_id
         JOIN series s ON s.id = i.series_id
         WHERE ti.trade_id IN (",
    );
    let mut separated = query.separated(", ");
    for trade_id in trade_ids {
        separated.push_bind(trade_id);
    }
    separated.push_unseparated(
        ") ORDER BY ti.trade_id, ti.offered_by_user_id, s.name,
                    i.issue_number, ti.copy_number_snapshot",
    );
    query.build_query_as().fetch_all(pool).await
}

pub async fn entry_is_reserved(
    connection: &mut MySqlConnection,
    entry_id: u32,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM trade_items ti
         JOIN trades t ON t.id = ti.trade_id AND t.status = 'accepted'
         WHERE ti.offer_entry_id = ? OR ti.wanted_entry_id = ?",
    )
    .bind(entry_id)
    .bind(entry_id)
    .fetch_one(connection)
    .await?
        > 0)
}

pub async fn lock_owned_entry(
    connection: &mut MySqlConnection,
    user_id: u32,
    entry_id: u32,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, u32>(
        "SELECT id FROM collection_entries
         WHERE id = ? AND user_id = ? FOR UPDATE",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(connection)
    .await?
    .is_some())
}

pub async fn find_owned_entry_issue(
    connection: &mut MySqlConnection,
    user_id: u32,
    entry_id: u32,
) -> Result<Option<u32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT issue_id FROM collection_entries
         WHERE id = ? AND user_id = ? FOR UPDATE",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(connection)
    .await
}

pub async fn cancel_proposals_for_entry(
    connection: &mut MySqlConnection,
    entry_id: u32,
    actor_user_id: Option<u32>,
) -> Result<Vec<u32>, sqlx::Error> {
    let proposals = sqlx::query_as::<_, ProposalCancellationRow>(
        "SELECT DISTINCT t.id, t.match_id, t.initiator_id, t.responder_id
         FROM trades t
         JOIN trade_items ti ON ti.trade_id = t.id
         WHERE t.status = 'proposed'
           AND (ti.offer_entry_id = ? OR ti.wanted_entry_id = ?)
         FOR UPDATE",
    )
    .bind(entry_id)
    .bind(entry_id)
    .fetch_all(&mut *connection)
    .await?;
    cancel_proposals_with_notifications(connection, &proposals, "items_changed", actor_user_id)
        .await?;
    Ok(proposals.iter().map(|proposal| proposal.id).collect())
}

pub async fn cancel_proposals_for_match(
    connection: &mut MySqlConnection,
    match_id: u32,
    actor_user_id: Option<u32>,
) -> Result<Vec<u32>, sqlx::Error> {
    let proposals = sqlx::query_as::<_, ProposalCancellationRow>(
        "SELECT id, match_id, initiator_id, responder_id
         FROM trades
         WHERE match_id = ? AND status = 'proposed'
         ORDER BY id FOR UPDATE",
    )
    .bind(match_id)
    .fetch_all(&mut *connection)
    .await?;
    cancel_proposals_with_notifications(connection, &proposals, "items_changed", actor_user_id)
        .await?;
    Ok(proposals.iter().map(|proposal| proposal.id).collect())
}

async fn cancel_proposals_with_notifications(
    connection: &mut MySqlConnection,
    proposals: &[ProposalCancellationRow],
    reason: &str,
    actor_user_id: Option<u32>,
) -> Result<(), sqlx::Error> {
    for proposal in proposals {
        cancel_trade(&mut *connection, proposal.id, reason).await?;
        let recipients = match actor_user_id {
            Some(actor) if actor == proposal.initiator_id => vec![proposal.responder_id],
            Some(actor) if actor == proposal.responder_id => vec![proposal.initiator_id],
            _ => vec![proposal.initiator_id, proposal.responder_id],
        };
        for recipient_id in recipients {
            notifications::insert_notification(
                &mut *connection,
                recipient_id,
                actor_user_id,
                "trade_cancelled",
                Some(proposal.match_id),
                Some(proposal.id),
                None,
                &format!("trade:{}:cancelled", proposal.id),
                &json!({ "trade_id": proposal.id, "reason": reason }),
            )
            .await?;
        }
    }
    Ok(())
}
