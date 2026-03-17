use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::db::{issues, series};
use crate::error::AppError;
use crate::models::series::{IssueResponse, SeriesResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/series", get(list_series))
        .route("/api/v1/series/{slug}/issues", get(list_series_issues))
        .route("/api/v1/issues/{id}", get(get_issue))
}

#[derive(Debug, Deserialize)]
struct PaginationParams {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
}

const fn default_page() -> u32 {
    1
}

const fn default_per_page() -> u32 {
    50
}

#[derive(Debug, Serialize)]
struct PaginatedResponse<T: Serialize> {
    data: Vec<T>,
    page: u32,
    per_page: u32,
    total: u32,
}

async fn list_series(State(state): State<AppState>) -> Result<Json<Vec<SeriesResponse>>, AppError> {
    let all_series = series::find_all_series(&state.inner.pool, true).await?;
    let response: Vec<SeriesResponse> = all_series.iter().map(SeriesResponse::from).collect();
    Ok(Json(response))
}

async fn list_series_issues(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<IssueResponse>>, AppError> {
    let per_page = params.per_page.clamp(1, 100);
    let page = params.page.max(1);

    let s = series::find_series_by_slug(&state.inner.pool, &slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series '{slug}' not found")))?;

    if !s.active {
        return Err(AppError::NotFound(format!("Series '{slug}' not found")));
    }

    let total = issues::count_issues_by_series(&state.inner.pool, s.id).await?;
    let issue_list = issues::find_issues_by_series(&state.inner.pool, s.id, page, per_page).await?;
    let data = issues::build_issue_responses(&state.inner.pool, &issue_list).await?;

    Ok(Json(PaginatedResponse {
        data,
        page,
        per_page,
        total,
    }))
}

async fn get_issue(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<IssueResponse>, AppError> {
    let issue = issues::find_issue_by_id(&state.inner.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Issue {id} not found")))?;

    // Only return issues from active series
    let s = series::find_series_by_id(&state.inner.pool, issue.series_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Issue {id} not found")))?;
    if !s.active {
        return Err(AppError::NotFound(format!("Issue {id} not found")));
    }

    let response = issues::build_issue_response(&state.inner.pool, &issue).await?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pagination() {
        assert_eq!(default_page(), 1);
        assert_eq!(default_per_page(), 50);
    }

    #[test]
    fn test_pagination_params_deserialization() {
        let params: PaginationParams =
            serde_json::from_str(r#"{"page": 2, "per_page": 25}"#).unwrap();
        assert_eq!(params.page, 2);
        assert_eq!(params.per_page, 25);
    }

    #[test]
    fn test_pagination_params_defaults() {
        let params: PaginationParams = serde_json::from_str(r"{}").unwrap();
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 50);
    }

    #[test]
    fn test_paginated_response_serialization() {
        let resp = PaginatedResponse {
            data: vec![1, 2, 3],
            page: 1,
            per_page: 50,
            total: 3,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["page"], 1);
        assert_eq!(json["per_page"], 50);
        assert_eq!(json["total"], 3);
        assert_eq!(json["data"].as_array().unwrap().len(), 3);
    }
}
