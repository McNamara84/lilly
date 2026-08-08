use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::trades;
use crate::error::AppError;
use crate::models::trades::{
    BulkWantedRequest, BulkWantedResponse, PaginatedTradeOffersResponse,
    PaginatedWantedCandidatesResponse, PaginatedWantedResponse, TradeListQueryParams,
    TradeOfferResponse, WantedCandidateResponse, WantedEntryResponse, normalize_bulk_issue_ids,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/trade-offers", get(list_trade_offers))
        .route("/api/v1/me/wanted", get(list_wanted_entries))
        .route("/api/v1/me/wanted/candidates", get(list_wanted_candidates))
        .route("/api/v1/me/wanted/bulk", post(add_wanted_entries))
        .route("/api/v1/me/wanted/{entry_id}", delete(delete_wanted_entry))
}

async fn list_trade_offers(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<TradeListQueryParams>,
) -> Result<Json<PaginatedTradeOffersResponse>, AppError> {
    params.validate().map_err(AppError::BadRequest)?;

    let total =
        trades::count_trade_list_entries(&state.inner.pool, auth.user_id, "duplicate", &params)
            .await?;
    let rows =
        trades::find_trade_list_entries(&state.inner.pool, auth.user_id, "duplicate", &params)
            .await?;
    let data = rows
        .iter()
        .map(TradeOfferResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|message| AppError::InternalError(anyhow::anyhow!(message)))?;

    Ok(Json(PaginatedTradeOffersResponse {
        data,
        page: params.page(),
        per_page: params.per_page(),
        total,
    }))
}

async fn list_wanted_entries(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<TradeListQueryParams>,
) -> Result<Json<PaginatedWantedResponse>, AppError> {
    params.validate().map_err(AppError::BadRequest)?;

    let total =
        trades::count_trade_list_entries(&state.inner.pool, auth.user_id, "wanted", &params)
            .await?;
    let rows =
        trades::find_trade_list_entries(&state.inner.pool, auth.user_id, "wanted", &params).await?;

    Ok(Json(PaginatedWantedResponse {
        data: rows.iter().map(WantedEntryResponse::from).collect(),
        page: params.page(),
        per_page: params.per_page(),
        total,
    }))
}

async fn list_wanted_candidates(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<TradeListQueryParams>,
) -> Result<Json<PaginatedWantedCandidatesResponse>, AppError> {
    params.validate().map_err(AppError::BadRequest)?;
    if params.series_slug().is_none() {
        return Err(AppError::BadRequest(
            "series_slug is required for wanted candidates".to_string(),
        ));
    }

    let total = trades::count_wanted_candidates(&state.inner.pool, auth.user_id, &params).await?;
    let rows = trades::find_wanted_candidates(&state.inner.pool, auth.user_id, &params).await?;

    Ok(Json(PaginatedWantedCandidatesResponse {
        data: rows.iter().map(WantedCandidateResponse::from).collect(),
        page: params.page(),
        per_page: params.per_page(),
        total,
    }))
}

async fn add_wanted_entries(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<BulkWantedRequest>,
) -> Result<Json<BulkWantedResponse>, AppError> {
    let issue_ids = normalize_bulk_issue_ids(&body.issue_ids).map_err(AppError::BadRequest)?;
    let result = trades::add_wanted_entries(&state.inner.pool, auth.user_id, &issue_ids).await?;
    Ok(Json(result))
}

async fn delete_wanted_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(entry_id): Path<u32>,
) -> Result<StatusCode, AppError> {
    if entry_id == 0 {
        return Err(wanted_entry_not_found());
    }

    let deleted = trades::delete_wanted_entry(&state.inner.pool, auth.user_id, entry_id).await?;
    if !deleted {
        return Err(wanted_entry_not_found());
    }

    Ok(StatusCode::NO_CONTENT)
}

fn wanted_entry_not_found() -> AppError {
    AppError::NotFound("Wanted entry not found".to_string())
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::*;

    #[test]
    fn wanted_not_found_response_is_generic() {
        let response = wanted_entry_not_found().into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
