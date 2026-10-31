use axum::Json;
#[cfg(test)]
use axum::Router;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::db::{issues, series};
use crate::error::AppError;
use crate::models::series::{IssueResponse, SeriesResponse};

#[cfg(test)]
#[allow(dead_code)]
pub fn router() -> Router<AppState> {
    openapi_router().into()
}

pub fn openapi_router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new()
        .routes(utoipa_axum::routes!(list_series))
        .routes(utoipa_axum::routes!(list_series_issues))
        .routes(utoipa_axum::routes!(get_issue))
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[schema(as = series::PaginationParams)]
#[into_params(parameter_in = Query)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = series::PaginatedResponse)]
struct PaginatedResponse<T: Serialize> {
    data: Vec<T>,
    page: u32,
    per_page: u32,
    total: u32,
}

#[utoipa::path(
    get,
    path = "/api/v1/series",
    operation_id = "series_list_series",
    tag = "series",
    summary = "List series",
    description = "List series.",
    security(),
    responses(
        (status = 200, description = "Successful operation", body = Vec<SeriesResponse>),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 409, description = "State conflict", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn list_series(State(state): State<AppState>) -> Result<Json<Vec<SeriesResponse>>, AppError> {
    let all_series = series::find_all_series(&state.inner.pool, true).await?;
    let response: Vec<SeriesResponse> = all_series.iter().map(SeriesResponse::from).collect();
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/series/{slug}/issues",
    operation_id = "series_list_series_issues",
    tag = "series",
    summary = "List series issues",
    description = "List series issues.",
    security(),
    params(("slug" = String, Path, description = "slug"), PaginationParams),
    responses(
        (status = 200, description = "Successful operation", body = PaginatedResponse<IssueResponse>),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 409, description = "State conflict", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
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

#[utoipa::path(
    get,
    path = "/api/v1/issues/{id}",
    operation_id = "series_get_issue",
    tag = "series",
    summary = "Get issue",
    description = "Get issue.",
    security(),
    params(("id" = u32, Path, description = "id", minimum = 1)),
    responses(
        (status = 200, description = "Successful operation", body = IssueResponse),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 409, description = "State conflict", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
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
