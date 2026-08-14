use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;
use sqlx::MySqlPool;

use crate::db::{collection, media, notifications, trade_matching, trade_workflow};
use crate::error::AppError;
use crate::models::trade_matching::{
    CreateTradeProposalRequest, PaginatedTradesResponse, TradeItemResponse, TradeListRow,
    TradePageParams, TradePartnerResponse, TradeResponse,
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

pub struct CompletionResult {
    pub trade: TradeResponse,
    pub photo_storage_keys: Vec<String>,
}

#[allow(clippy::too_many_lines)]
pub async fn complete_trade(
    pool: &MySqlPool,
    user_id: u32,
    trade_id: u32,
) -> Result<CompletionResult, AppError> {
    let mut transaction = pool.begin().await?;
    let trade = trade_workflow::lock_trade_for_participant(&mut transaction, trade_id, user_id)
        .await?
        .ok_or_else(resource_not_found)?;

    if trade.status == "completed" {
        transaction.commit().await?;
        return Ok(CompletionResult {
            trade: get_trade(pool, user_id, trade_id).await?,
            photo_storage_keys: Vec::new(),
        });
    }
    if trade.status != "accepted" {
        return Err(conflict(
            "The trade cannot be completed in its current state",
            "invalid_trade_transition",
        ));
    }

    trade_workflow::insert_completion_confirmation(&mut transaction, trade_id, user_id).await?;
    let confirmation_count =
        trade_workflow::count_completion_confirmations(&mut transaction, trade_id).await?;
    let partner_id = if trade.initiator_id == user_id {
        trade.responder_id
    } else {
        trade.initiator_id
    };

    if confirmation_count < 2 {
        notifications::insert_notification(
            &mut transaction,
            partner_id,
            Some(user_id),
            "trade_completion_confirmed",
            Some(trade.match_id),
            Some(trade_id),
            None,
            &format!("trade:{trade_id}:completion-confirmed:{user_id}"),
            &json!({ "trade_id": trade_id }),
        )
        .await?;
        transaction.commit().await?;
        return Ok(CompletionResult {
            trade: get_trade(pool, user_id, trade_id).await?,
            photo_storage_keys: Vec::new(),
        });
    }

    let issue_ids = trade_workflow::find_trade_issue_ids(&mut transaction, trade_id).await?;
    if issue_ids.is_empty() {
        return Err(conflict(
            "The trade contains no transferable items",
            "trade_items_changed",
        ));
    }
    let first_user_id = trade.initiator_id.min(trade.responder_id);
    trade_matching::lock_reconciliation_users_for_issues(
        &mut transaction,
        first_user_id,
        &issue_ids,
    )
    .await?;
    trade_workflow::lock_trade_entry_references(&mut transaction, trade_id).await?;
    let items = trade_workflow::find_completion_items(&mut transaction, trade_id).await?;
    if items.is_empty() {
        return Err(conflict(
            "The trade contains no transferable items",
            "trade_items_changed",
        ));
    }
    for item in &items {
        validate_completion_item(item)?;
    }

    let mut photo_storage_keys = Vec::new();
    let mut deleted_offer_ids = BTreeSet::new();
    for item in &items {
        let offer_entry_id = item.offer_entry_id.ok_or_else(trade_items_changed)?;
        if deleted_offer_ids.insert(offer_entry_id) {
            photo_storage_keys.extend(
                media::enqueue_entry_photo_deletions(
                    &mut transaction,
                    offer_entry_id,
                    item.offered_by_user_id,
                )
                .await?,
            );
            if !collection::delete_entry_on_connection(
                &mut transaction,
                offer_entry_id,
                item.offered_by_user_id,
            )
            .await?
            {
                return Err(trade_items_changed());
            }
        }
    }

    let mut consumed_wanted_ids = BTreeSet::new();
    for item in &items {
        let wanted_entry_id = item.wanted_entry_id.ok_or_else(trade_items_changed)?;
        if consumed_wanted_ids.insert(wanted_entry_id) {
            if !trade_workflow::update_wanted_entry_to_owned(
                &mut transaction,
                wanted_entry_id,
                item.receiving_user_id,
                &item.condition_grade_snapshot,
                item.edition_label_snapshot.as_deref(),
            )
            .await?
            {
                return Err(trade_items_changed());
            }
        } else {
            let copy_number = collection::next_copy_number_on_connection(
                &mut transaction,
                item.receiving_user_id,
                item.issue_id,
            )
            .await?
            .ok_or_else(|| {
                conflict(
                    "No free copy number is available for the received issue",
                    "collection_capacity_exceeded",
                )
            })?;
            collection::add_entry_on_connection(
                &mut transaction,
                collection::NewCollectionEntry {
                    user_id: item.receiving_user_id,
                    issue_id: item.issue_id,
                    copy_number,
                    condition_grade: Some(&item.condition_grade_snapshot),
                    status: "owned",
                    notes: None,
                    edition_label: item.edition_label_snapshot.as_deref(),
                },
            )
            .await?;
        }
    }

    if !trade_workflow::mark_trade_completed(&mut transaction, trade_id).await? {
        return Err(conflict(
            "The trade completion was already applied",
            "invalid_trade_transition",
        ));
    }

    for participant_id in [trade.initiator_id, trade.responder_id]
        .into_iter()
        .collect::<BTreeSet<_>>()
    {
        trade_matching::reconcile_user_matches(&mut transaction, participant_id).await?;
    }

    notifications::insert_notification(
        &mut transaction,
        partner_id,
        Some(user_id),
        "trade_completed",
        Some(trade.match_id),
        Some(trade_id),
        None,
        &format!("trade:{trade_id}:completed"),
        &json!({ "trade_id": trade_id }),
    )
    .await?;
    transaction.commit().await?;

    Ok(CompletionResult {
        trade: get_trade(pool, user_id, trade_id).await?,
        photo_storage_keys,
    })
}

fn validate_completion_item(item: &trade_workflow::CompletionItemRow) -> Result<(), AppError> {
    let offer_valid = item.offer_entry_id.is_some()
        && item.offer_user_id == Some(item.offered_by_user_id)
        && item.offer_issue_id == Some(item.issue_id)
        && item.offer_status.as_deref() == Some("duplicate")
        && item.offer_condition_grade.as_deref() == Some(item.condition_grade_snapshot.as_str())
        && item.offer_edition_label == item.edition_label_snapshot;
    let wanted_valid = item.wanted_entry_id.is_some()
        && item.wanted_user_id == Some(item.receiving_user_id)
        && item.wanted_issue_id == Some(item.issue_id)
        && item.wanted_status.as_deref() == Some("wanted")
        && item.wanted_edition_label == item.wanted_edition_label_snapshot;
    if offer_valid && wanted_valid {
        Ok(())
    } else {
        Err(trade_items_changed())
    }
}

fn trade_items_changed() -> AppError {
    conflict(
        "One or more agreed collection entries changed before completion",
        "trade_items_changed",
    )
}

pub async fn list_trades(
    pool: &MySqlPool,
    user_id: u32,
    params: &TradePageParams,
) -> Result<PaginatedTradesResponse, AppError> {
    params.validate().map_err(AppError::BadRequest)?;
    let total = trade_workflow::count_trades(pool, user_id, params).await?;
    let rows = trade_workflow::find_trades(pool, user_id, params).await?;
    let trade_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    let mut items_by_trade = BTreeMap::new();
    for item in trade_workflow::find_trade_items_for_trades(pool, &trade_ids).await? {
        items_by_trade
            .entry(item.trade_id)
            .or_insert_with(Vec::new)
            .push(item);
    }
    let data = rows
        .into_iter()
        .map(|row| {
            let items = items_by_trade.remove(&row.id).unwrap_or_default();
            build_trade(user_id, row, &items)
        })
        .collect();
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
    let items = trade_workflow::find_trade_items(pool, row.id).await?;
    Ok(build_trade(user_id, row, &items))
}

fn build_trade(
    user_id: u32,
    row: TradeListRow,
    items: &[crate::models::trade_matching::TradeItemViewRow],
) -> TradeResponse {
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
    TradeResponse {
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
                .then(|| {
                    crate::models::profile::avatar_content_url(
                        row.partner_id,
                        row.partner_avatar_path.is_some(),
                    )
                })
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
        completed_at: row.completed_at,
        my_completion_confirmed_at: row.my_completion_confirmed_at,
        partner_completion_confirmed_at: row.partner_completion_confirmed_at,
        updated_at: row.updated_at,
    }
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
