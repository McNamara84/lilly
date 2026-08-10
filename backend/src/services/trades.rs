use std::collections::BTreeSet;

use serde_json::json;
use sqlx::MySqlPool;

use crate::db::{notifications, trade_workflow};
use crate::error::AppError;
use crate::models::trade_matching::{
    CreateTradeProposalRequest, PageParams, PaginatedTradesResponse, TradeItemResponse,
    TradeListRow, TradePartnerResponse, TradeResponse,
};

pub async fn create_proposal(
    pool: &MySqlPool,
    user_id: u32,
    match_id: u32,
    request: &CreateTradeProposalRequest,
) -> Result<TradeResponse, AppError> {
    let (offered_entry_ids, requested_entry_ids) =
        request.normalize().map_err(AppError::BadRequest)?;
    let offered_ids = offered_entry_ids.into_iter().collect::<BTreeSet<_>>();
    let requested_ids = requested_entry_ids.into_iter().collect::<BTreeSet<_>>();

    let mut transaction = pool.begin().await?;
    let matched = trade_workflow::lock_match_for_participant(&mut transaction, match_id, user_id)
        .await?
        .ok_or_else(resource_not_found)?;
    if matched.status != "active" {
        return Err(conflict("This match is no longer active", "match_stale"));
    }
    if matched.open_trade_id.is_some() {
        return Err(conflict(
            "This match already has an open trade",
            "open_trade_exists",
        ));
    }
    let responder_id = if matched.user_low_id == user_id {
        matched.user_high_id
    } else {
        matched.user_low_id
    };
    let current_items =
        trade_workflow::find_proposal_items_for_update(&mut transaction, match_id).await?;
    let mut selected = Vec::new();
    let mut found_offered = BTreeSet::new();
    let mut found_requested = BTreeSet::new();
    for item in current_items {
        if item.offered_by_user_id == user_id && offered_ids.contains(&item.offer_entry_id) {
            if found_offered.insert(item.offer_entry_id) {
                selected.push(item);
            }
        } else if item.offered_by_user_id == responder_id
            && requested_ids.contains(&item.offer_entry_id)
            && found_requested.insert(item.offer_entry_id)
        {
            selected.push(item);
        }
    }
    if found_offered != offered_ids || found_requested != requested_ids {
        return Err(conflict(
            "One or more selected entries are no longer part of the match",
            "match_items_changed",
        ));
    }
    for item in &selected {
        if trade_workflow::proposal_item_is_reserved(&mut transaction, item).await? {
            return Err(conflict(
                "One or more selected entries are reserved by an accepted trade",
                "trade_item_reserved",
            ));
        }
    }

    let trade_id =
        match trade_workflow::insert_trade(&mut transaction, match_id, user_id, responder_id).await
        {
            Ok(trade_id) => trade_id,
            Err(error)
                if matches!(
                    &error,
                    sqlx::Error::Database(database_error)
                        if database_error.kind() == sqlx::error::ErrorKind::UniqueViolation
                ) =>
            {
                return Err(conflict(
                    "This match already has an open trade",
                    "open_trade_exists",
                ));
            }
            Err(error) => return Err(error.into()),
        };
    for item in &selected {
        trade_workflow::insert_trade_item(&mut transaction, trade_id, item).await?;
    }
    let thread_id = trade_workflow::insert_message_thread(&mut transaction, trade_id).await?;
    notifications::insert_notification(
        &mut transaction,
        responder_id,
        Some(user_id),
        "trade_proposed",
        Some(match_id),
        Some(trade_id),
        None,
        &format!("trade:{trade_id}:proposed"),
        &json!({ "trade_id": trade_id, "thread_id": thread_id }),
    )
    .await?;
    transaction.commit().await?;
    get_trade(pool, user_id, trade_id).await
}

pub async fn accept_trade(
    pool: &MySqlPool,
    user_id: u32,
    trade_id: u32,
) -> Result<TradeResponse, AppError> {
    let mut transaction = pool.begin().await?;
    let trade = trade_workflow::lock_trade_for_participant(&mut transaction, trade_id, user_id)
        .await?
        .ok_or_else(resource_not_found)?;
    if trade.responder_id != user_id {
        return Err(AppError::Forbidden {
            message: "Only the recipient can accept this trade".to_string(),
            code: Some("trade_accept_forbidden".to_string()),
        });
    }
    match trade.status.as_str() {
        "accepted" => {
            transaction.commit().await?;
            return get_trade(pool, user_id, trade_id).await;
        }
        "proposed" => {}
        _ => {
            return Err(conflict(
                "The trade cannot be accepted in its current state",
                "invalid_trade_transition",
            ));
        }
    }
    trade_workflow::lock_trade_entry_references(&mut transaction, trade_id).await?;
    if trade_workflow::trade_has_reservation_conflict(&mut transaction, trade_id).await? {
        return Err(conflict(
            "One or more selected entries are reserved by another accepted trade",
            "trade_item_reserved",
        ));
    }
    trade_workflow::accept_trade(&mut transaction, trade_id).await?;
    notifications::insert_notification(
        &mut transaction,
        trade.initiator_id,
        Some(user_id),
        "trade_accepted",
        Some(trade.match_id),
        Some(trade_id),
        None,
        &format!("trade:{trade_id}:accepted"),
        &json!({ "trade_id": trade_id }),
    )
    .await?;
    transaction.commit().await?;
    get_trade(pool, user_id, trade_id).await
}

pub async fn cancel_trade(pool: &MySqlPool, user_id: u32, trade_id: u32) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    let trade = trade_workflow::lock_trade_for_participant(&mut transaction, trade_id, user_id)
        .await?
        .ok_or_else(resource_not_found)?;
    if trade.status == "cancelled" {
        transaction.commit().await?;
        return Ok(());
    }
    if !matches!(trade.status.as_str(), "proposed" | "accepted") {
        return Err(conflict(
            "The trade cannot be cancelled in its current state",
            "invalid_trade_transition",
        ));
    }
    trade_workflow::cancel_trade(&mut transaction, trade_id, "cancelled_by_participant").await?;
    let recipient_id = if trade.initiator_id == user_id {
        trade.responder_id
    } else {
        trade.initiator_id
    };
    notifications::insert_notification(
        &mut transaction,
        recipient_id,
        Some(user_id),
        "trade_cancelled",
        Some(trade.match_id),
        Some(trade_id),
        None,
        &format!("trade:{trade_id}:cancelled"),
        &json!({ "trade_id": trade_id }),
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn list_open_trades(
    pool: &MySqlPool,
    user_id: u32,
    params: &PageParams,
) -> Result<PaginatedTradesResponse, AppError> {
    let total = trade_workflow::count_open_trades(pool, user_id).await?;
    let rows = trade_workflow::find_open_trades(pool, user_id, params).await?;
    let mut data = Vec::with_capacity(rows.len());
    for row in rows {
        data.push(build_trade(pool, user_id, row).await?);
    }
    Ok(PaginatedTradesResponse {
        data,
        page: params.page(),
        per_page: params.per_page(),
        total,
    })
}

pub async fn get_trade(
    pool: &MySqlPool,
    user_id: u32,
    trade_id: u32,
) -> Result<TradeResponse, AppError> {
    let row = trade_workflow::find_trade_for_participant(pool, user_id, trade_id)
        .await?
        .ok_or_else(resource_not_found)?;
    build_trade(pool, user_id, row).await
}

async fn build_trade(
    pool: &MySqlPool,
    user_id: u32,
    row: TradeListRow,
) -> Result<TradeResponse, AppError> {
    let items = trade_workflow::find_trade_items(pool, row.id).await?;
    let my_offers = items
        .iter()
        .filter(|item| item.offered_by_user_id == user_id)
        .map(TradeItemResponse::from)
        .collect();
    let partner_offers = items
        .iter()
        .filter(|item| item.offered_by_user_id != user_id)
        .map(TradeItemResponse::from)
        .collect();
    Ok(TradeResponse {
        id: row.id,
        match_id: row.match_id,
        status: row.status,
        role: if row.initiator_id == user_id {
            "initiator".to_string()
        } else {
            "responder".to_string()
        },
        partner: TradePartnerResponse {
            id: row.partner_id,
            display_name: row.partner_display_name,
            avatar_path: row
                .partner_profile_public
                .then_some(row.partner_avatar_path)
                .flatten(),
            location: row
                .partner_profile_public
                .then_some(row.partner_location)
                .flatten(),
        },
        my_offers,
        partner_offers,
        thread_id: row.thread_id,
        cancellation_reason: row.cancellation_reason,
        proposed_at: row.proposed_at,
        accepted_at: row.accepted_at,
        cancelled_at: row.cancelled_at,
        updated_at: row.updated_at,
    })
}

fn resource_not_found() -> AppError {
    AppError::NotFound("Resource not found".to_string())
}

fn conflict(message: &str, code: &str) -> AppError {
    AppError::ConflictWithCode {
        message: message.to_string(),
        code: code.to_string(),
    }
}
