use std::collections::BTreeMap;

use sqlx::MySqlPool;

use crate::db::{trade_matching, trade_workflow};
use crate::error::AppError;
use crate::models::trade_matching::{
    MatchIssueResponse, MatchListRow, PageParams, PaginatedMatchesResponse, TradeMatchResponse,
    TradePartnerResponse, calculate_match_score,
};

pub async fn list_matches(
    pool: &MySqlPool,
    user_id: u32,
    params: &PageParams,
) -> Result<PaginatedMatchesResponse, AppError> {
    let total = trade_matching::count_matches(pool, user_id).await?;
    let rows = trade_matching::find_matches(pool, user_id, params).await?;
    let match_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    let mut items_by_match = BTreeMap::new();
    for item in trade_matching::find_match_item_views_for_matches(pool, &match_ids).await? {
        items_by_match
            .entry(item.match_id)
            .or_insert_with(Vec::new)
            .push(item);
    }
    let data = rows
        .into_iter()
        .map(|row| {
            let items = items_by_match.remove(&row.id).unwrap_or_default();
            build_match(user_id, row, &items)
        })
        .collect();
    Ok(PaginatedMatchesResponse {
        data,
        page: params.page(),
        per_page: params.per_page(),
        total,
    })
}

pub async fn get_match(
    pool: &MySqlPool,
    user_id: u32,
    match_id: u32,
) -> Result<TradeMatchResponse, AppError> {
    let row = trade_matching::find_match_for_participant(pool, user_id, match_id)
        .await?
        .ok_or_else(resource_not_found)?;
    let items = trade_matching::find_match_item_views(pool, row.id).await?;
    Ok(build_match(user_id, row, &items))
}

#[allow(dead_code)]
pub async fn reconcile_user(pool: &MySqlPool, user_id: u32) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    trade_matching::reconcile_user_matches(&mut transaction, user_id).await?;
    transaction.commit().await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn prepare_entry_mutation(
    pool: &MySqlPool,
    user_id: u32,
    entry_id: u32,
) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    prepare_entry_mutation_in_transaction(&mut transaction, user_id, entry_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn prepare_entry_mutation_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: u32,
    entry_id: u32,
) -> Result<(), AppError> {
    let issue_id = trade_workflow::find_owned_entry_issue(transaction, user_id, entry_id)
        .await?
        .ok_or_else(resource_not_found)?;
    trade_matching::lock_reconciliation_users_for_issues(transaction, user_id, &[issue_id]).await?;
    if !trade_workflow::lock_owned_entry(transaction, user_id, entry_id).await? {
        return Err(resource_not_found());
    }
    if trade_workflow::entry_is_reserved(transaction, entry_id).await? {
        return Err(AppError::ConflictWithCode {
            message: "This collection entry is reserved by an accepted trade".to_string(),
            code: "entry_reserved_by_trade".to_string(),
        });
    }
    trade_workflow::cancel_proposals_for_entry(transaction, entry_id, Some(user_id)).await?;
    Ok(())
}

fn build_match(
    user_id: u32,
    row: MatchListRow,
    items: &[crate::models::trade_matching::MatchItemViewRow],
) -> TradeMatchResponse {
    let my_offers = items
        .iter()
        .filter(|item| item.offered_by_user_id == user_id)
        .map(MatchIssueResponse::from)
        .collect::<Vec<_>>();
    let partner_offers = items
        .iter()
        .filter(|item| item.offered_by_user_id != user_id)
        .map(MatchIssueResponse::from)
        .collect::<Vec<_>>();
    TradeMatchResponse {
        id: row.id,
        status: row.status.clone(),
        revision: row.revision,
        changed_at: row.changed_at,
        partner: TradePartnerResponse::from(&row),
        match_score: calculate_match_score(my_offers.len(), partner_offers.len()),
        my_offers,
        partner_offers,
        open_trade_id: row.open_trade_id,
        open_trade_status: row.open_trade_status,
    }
}

fn resource_not_found() -> AppError {
    AppError::NotFound("Resource not found".to_string())
}
