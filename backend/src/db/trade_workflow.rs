use serde_json::json;
use sqlx::{MySql, MySqlConnection, MySqlPool, QueryBuilder, Transaction};

use crate::db::notifications;
use crate::models::trade_matching::{PageParams, TradeItemViewRow, TradeListRow, TradeRecord};

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
    pub condition_grade: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProposalCancellationRow {
    pub id: u32,
    pub match_id: u32,
    pub initiator_id: u32,
    pub responder_id: u32,
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
                offer.copy_number, offer.condition_grade
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
             condition_grade_snapshot)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(trade_id)
    .bind(item.offer_entry_id)
    .bind(item.wanted_entry_id)
    .bind(item.issue_id)
    .bind(item.offered_by_user_id)
    .bind(item.wanted_by_user_id)
    .bind(item.copy_number)
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
                created_at, updated_at
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

pub async fn count_open_trades(pool: &MySqlPool, user_id: u32) -> Result<u32, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trades
         WHERE status IN ('proposed', 'accepted')
           AND (initiator_id = ? OR responder_id = ?)",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

pub async fn find_open_trades(
    pool: &MySqlPool,
    user_id: u32,
    params: &PageParams,
) -> Result<Vec<TradeListRow>, sqlx::Error> {
    sqlx::query_as::<_, TradeListRow>(
        "SELECT t.id, t.match_id, t.initiator_id, t.responder_id, t.status,
                t.cancellation_reason, t.proposed_at, t.accepted_at,
                t.cancelled_at, t.created_at, t.updated_at,
                partner.id AS partner_id, partner.display_name AS partner_display_name,
                partner.profile_public AS partner_profile_public,
                partner.avatar_path AS partner_avatar_path,
                partner.location AS partner_location, mt.id AS thread_id
         FROM trades t
         JOIN users partner ON partner.id = CASE
             WHEN t.initiator_id = ? THEN t.responder_id ELSE t.initiator_id END
         JOIN message_threads mt ON mt.trade_id = t.id
         WHERE t.status IN ('proposed', 'accepted')
           AND (t.initiator_id = ? OR t.responder_id = ?)
         ORDER BY t.updated_at DESC, t.id DESC
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

pub async fn find_trade_for_participant(
    pool: &MySqlPool,
    user_id: u32,
    trade_id: u32,
) -> Result<Option<TradeListRow>, sqlx::Error> {
    sqlx::query_as::<_, TradeListRow>(
        "SELECT t.id, t.match_id, t.initiator_id, t.responder_id, t.status,
                t.cancellation_reason, t.proposed_at, t.accepted_at,
                t.cancelled_at, t.created_at, t.updated_at,
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
