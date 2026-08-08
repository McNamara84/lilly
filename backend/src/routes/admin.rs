use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::middleware::AdminUser;
use crate::db::{import_jobs, issues, series};
use crate::error::AppError;
use crate::models::series::{ImportJobResponse, IssueResponse, SeriesResponse};
use crate::services::import::{ImportTrigger, start_import as start_import_service};
use crate::services::import_scheduler::ImportScheduleStatus;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/series", get(list_all_series))
        .route(
            "/api/v1/admin/series/{slug}/activate",
            post(activate_series),
        )
        .route(
            "/api/v1/admin/series/{slug}/deactivate",
            post(deactivate_series),
        )
        .route("/api/v1/admin/adapters", get(list_adapters))
        .route("/api/v1/admin/import", post(start_import))
        .route("/api/v1/admin/import/schedule", get(import_schedule))
        .route("/api/v1/admin/import/history", get(import_history))
        .route("/api/v1/admin/import/{id}", get(get_import_job))
        .route(
            "/api/v1/admin/import/{id}/series-issues",
            get(get_import_series_issues),
        )
}

async fn import_schedule(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<ImportScheduleStatus>, AppError> {
    let status = state
        .inner
        .import_scheduler_config
        .status(chrono::Utc::now())
        .map_err(|error| AppError::InternalError(anyhow::anyhow!(error)))?;
    Ok(Json(status))
}

async fn list_all_series(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<SeriesResponse>>, AppError> {
    let all_series = series::find_all_series(&state.inner.pool, false).await?;
    let response: Vec<SeriesResponse> = all_series.iter().map(SeriesResponse::from).collect();
    Ok(Json(response))
}

async fn activate_series(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = series::find_series_by_slug(&state.inner.pool, &slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series '{slug}' not found")))?;

    series::set_series_active(&state.inner.pool, s.id, true).await?;
    tracing::info!(slug = %slug, "Series activated");

    Ok(Json(serde_json::json!({ "message": "Series activated" })))
}

async fn deactivate_series(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = series::find_series_by_slug(&state.inner.pool, &slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series '{slug}' not found")))?;

    series::set_series_active(&state.inner.pool, s.id, false).await?;
    tracing::info!(slug = %slug, "Series deactivated");

    Ok(Json(serde_json::json!({ "message": "Series deactivated" })))
}

#[derive(Debug, Serialize)]
struct AdapterInfo {
    name: String,
    display_name: String,
    version: String,
}

async fn list_adapters(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<AdapterInfo>>, AppError> {
    let adapters = state
        .inner
        .adapter_registry
        .list()
        .into_iter()
        .map(|(name, display_name, version)| AdapterInfo {
            name: name.to_string(),
            display_name: display_name.to_string(),
            version: version.to_string(),
        })
        .collect();

    Ok(Json(adapters))
}

#[derive(Debug, Deserialize)]
struct StartImportRequest {
    adapter: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ImportJobWithSlug {
    id: u32,
    series_id: u32,
    series_slug: String,
    adapter_name: String,
    trigger_type: String,
    scheduled_for: Option<chrono::NaiveDateTime>,
    status: String,
    total_issues: u32,
    imported_issues: u32,
    failed_issues: u32,
    error_message: Option<String>,
    started_by: Option<u32>,
    started_at: Option<chrono::NaiveDateTime>,
    completed_at: Option<chrono::NaiveDateTime>,
    #[allow(dead_code)]
    created_at: chrono::NaiveDateTime,
}

async fn start_import(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(request): Json<StartImportRequest>,
) -> Result<Json<ImportJobResponse>, AppError> {
    let job = start_import_service(
        state.inner.clone(),
        &request.adapter,
        ImportTrigger::Manual {
            user_id: admin.0.user_id,
        },
    )
    .await?;
    Ok(Json(job))
}

async fn get_import_job(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<ImportJobResponse>, AppError> {
    let job = import_jobs::find_import_job_by_id(&state.inner.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Import job {id} not found")))?;

    let slug = resolve_series_slug(&state.inner.pool, job.series_id).await?;
    Ok(Json(ImportJobResponse::from_job_with_slug(&job, slug)))
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

/// Returns all issues for the series associated with this import job (not just issues from this specific import run).
async fn get_import_series_issues(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedIssueResponse>, AppError> {
    let per_page = params.per_page.clamp(1, 100);
    let page = params.page.max(1);

    let job = import_jobs::find_import_job_by_id(&state.inner.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Import job {id} not found")))?;

    let total = issues::count_issues_by_series(&state.inner.pool, job.series_id).await?;
    let issue_list =
        issues::find_issues_by_series(&state.inner.pool, job.series_id, page, per_page).await?;
    let data = issues::build_issue_responses(&state.inner.pool, &issue_list).await?;

    Ok(Json(PaginatedIssueResponse {
        data,
        page,
        per_page,
        total,
    }))
}

#[derive(Debug, Serialize)]
struct PaginatedIssueResponse {
    data: Vec<IssueResponse>,
    page: u32,
    per_page: u32,
    total: u32,
}

async fn import_history(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ImportJobResponse>>, AppError> {
    let rows: Vec<ImportJobWithSlug> = sqlx::query_as(
        "SELECT j.id, j.series_id, s.slug AS series_slug, j.adapter_name, j.trigger_type, \
         j.scheduled_for, j.status, j.total_issues, j.imported_issues, j.failed_issues, \
         j.error_message, j.started_by, \
         j.started_at, j.completed_at, j.created_at \
         FROM import_jobs j \
         JOIN series s ON s.id = j.series_id \
         ORDER BY j.created_at DESC",
    )
    .fetch_all(&state.inner.pool)
    .await?;

    let response = rows
        .iter()
        .map(|r| ImportJobResponse {
            id: r.id,
            series_id: r.series_id,
            series_slug: r.series_slug.clone(),
            adapter_name: r.adapter_name.clone(),
            trigger_type: r.trigger_type.clone(),
            scheduled_for: r.scheduled_for,
            status: r.status.clone(),
            total_issues: r.total_issues,
            imported_issues: r.imported_issues,
            failed_issues: r.failed_issues,
            error_message: r.error_message.clone(),
            started_by: r.started_by,
            started_at: r.started_at,
            completed_at: r.completed_at,
        })
        .collect();
    Ok(Json(response))
}

async fn resolve_series_slug(pool: &sqlx::MySqlPool, series_id: u32) -> Result<String, AppError> {
    let s = series::find_series_by_id(pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series {series_id} not found")))?;
    Ok(s.slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_info_serialization() {
        let info = AdapterInfo {
            name: "maddrax".to_string(),
            display_name: "Maddrax".to_string(),
            version: "0.9".to_string(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "maddrax");
        assert_eq!(json["display_name"], "Maddrax");
        assert_eq!(json["version"], "0.9");
    }

    #[test]
    fn test_start_import_request_deserialization() {
        let req: StartImportRequest = serde_json::from_str(r#"{"adapter": "maddrax"}"#).unwrap();
        assert_eq!(req.adapter, "maddrax");
    }

    #[test]
    fn test_paginated_issue_response_serialization() {
        let resp = PaginatedIssueResponse {
            data: vec![],
            page: 1,
            per_page: 50,
            total: 0,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["page"], 1);
        assert_eq!(json["total"], 0);
        assert!(json["data"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_default_pagination() {
        assert_eq!(default_page(), 1);
        assert_eq!(default_per_page(), 50);
    }
}
