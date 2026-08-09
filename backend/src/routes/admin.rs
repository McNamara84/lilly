use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::middleware::AdminUser;
use crate::db::{import_jobs, import_review, issues, series};
use crate::error::AppError;
use crate::models::import_review::{
    ActivateImportRequest, ActivationResponse, PaginatedReviewItems, ReviewSummary,
};
use crate::models::series::{ImportJobError, ImportJobResponse, IssueResponse, SeriesResponse};
use crate::services::import::{
    ImportTrigger, retry_import as retry_import_service, start_import as start_import_service,
};
use crate::services::import_scheduler::ImportScheduleStatus;
use crate::services::publication;

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
        .route("/api/v1/admin/import/{id}/cancel", post(cancel_import))
        .route("/api/v1/admin/import/{id}/retry", post(retry_import))
        .route("/api/v1/admin/import/{id}/errors", get(get_import_errors))
        .route(
            "/api/v1/admin/import/{id}/review/summary",
            get(get_import_review_summary),
        )
        .route(
            "/api/v1/admin/import/{id}/review/items",
            get(get_import_review_items),
        )
        .route("/api/v1/admin/import/{id}/activate", post(activate_import))
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
) -> Result<Json<Vec<SeriesAdminResponse>>, AppError> {
    let all_series = series::find_all_series(&state.inner.pool, false).await?;
    let mut response = Vec::with_capacity(all_series.len());
    for target_series in &all_series {
        response.push(SeriesAdminResponse {
            series: SeriesResponse::from(target_series),
            latest_import_job_id: import_review::latest_import_job_id_for_series(
                &state.inner.pool,
                target_series.id,
            )
            .await?,
        });
    }
    Ok(Json(response))
}

#[derive(Debug, Serialize)]
struct SeriesAdminResponse {
    #[serde(flatten)]
    series: SeriesResponse,
    latest_import_job_id: Option<u32>,
}

async fn activate_series(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = series::find_series_by_slug(&state.inner.pool, &slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series '{slug}' not found")))?;

    if !s.active {
        return Err(AppError::ConflictWithCode {
            message: "Review and activate the latest import job instead".to_string(),
            code: "review_required".to_string(),
        });
    }

    Ok(Json(
        serde_json::json!({ "message": "Series is already active" }),
    ))
}

async fn deactivate_series(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = series::find_series_by_slug(&state.inner.pool, &slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series '{slug}' not found")))?;

    publication::deactivate_series(&state.inner, s.id, admin.0.user_id).await?;
    tracing::info!(slug = %slug, "Series deactivated");

    Ok(Json(serde_json::json!({ "message": "Series deactivated" })))
}

#[derive(Debug, Serialize)]
struct AdapterInfo {
    name: String,
    display_name: String,
    version: String,
    source_key: String,
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
        .map(|(name, display_name, version, source)| AdapterInfo {
            name: name.to_string(),
            display_name: display_name.to_string(),
            version: version.to_string(),
            source_key: source.source_key.to_string(),
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
    source_key: Option<String>,
    trigger_type: String,
    scheduled_for: Option<chrono::NaiveDateTime>,
    status: String,
    total_issues: u32,
    imported_issues: u32,
    created_issues: u32,
    updated_issues: u32,
    unchanged_issues: u32,
    skipped_issues: u32,
    failed_issues: u32,
    error_message: Option<String>,
    started_by: Option<u32>,
    started_at: Option<chrono::NaiveDateTime>,
    completed_at: Option<chrono::NaiveDateTime>,
    #[allow(dead_code)]
    created_at: chrono::NaiveDateTime,
    updated_at: chrono::NaiveDateTime,
    cancel_requested_at: Option<chrono::NaiveDateTime>,
    retry_of_job_id: Option<u32>,
}

async fn start_import(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(request): Json<StartImportRequest>,
) -> Result<(StatusCode, Json<ImportJobResponse>), AppError> {
    let job = start_import_service(
        state.inner.clone(),
        &request.adapter,
        ImportTrigger::Manual {
            user_id: admin.0.user_id,
        },
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
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

async fn cancel_import(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<(StatusCode, Json<ImportJobResponse>), AppError> {
    let existing = import_jobs::find_import_job_by_id(&state.inner.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Import job {id} not found")))?;
    if existing.status == "cancelled" {
        let slug = resolve_series_slug(&state.inner.pool, existing.series_id).await?;
        return Ok((
            StatusCode::OK,
            Json(ImportJobResponse::from_job_with_slug(&existing, slug)),
        ));
    }
    if !matches!(existing.status.as_str(), "pending" | "running") {
        return Err(AppError::Conflict(format!(
            "Import job {id} is already finished"
        )));
    }
    let cancellation_persisted =
        import_jobs::request_import_cancellation(&state.inner.pool, id).await?;
    let job = import_jobs::find_import_job_by_id(&state.inner.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Import job {id} not found")))?;
    let response_status = if cancellation_persisted {
        StatusCode::ACCEPTED
    } else {
        cancellation_status_after_noop(id, &job.status, job.cancel_requested_at.is_some())?
    };
    let slug = resolve_series_slug(&state.inner.pool, job.series_id).await?;
    Ok((
        response_status,
        Json(ImportJobResponse::from_job_with_slug(&job, slug)),
    ))
}

fn cancellation_status_after_noop(
    id: u32,
    status: &str,
    cancellation_already_requested: bool,
) -> Result<StatusCode, AppError> {
    if status == "cancelled" {
        return Ok(StatusCode::OK);
    }
    if matches!(status, "pending" | "running") && cancellation_already_requested {
        return Ok(StatusCode::ACCEPTED);
    }
    Err(AppError::Conflict(format!(
        "Import job {id} reached status '{status}' before cancellation was accepted"
    )))
}

async fn retry_import(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<(StatusCode, Json<ImportJobResponse>), AppError> {
    let job = retry_import_service(state.inner.clone(), id, admin.0.user_id).await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn get_import_errors(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedImportErrorResponse>, AppError> {
    if import_jobs::find_import_job_by_id(&state.inner.pool, id)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound(format!("Import job {id} not found")));
    }
    let page = params.page.max(1);
    let per_page = params.per_page.clamp(1, 100);
    let total = import_jobs::count_import_errors(&state.inner.pool, id).await?;
    let data = import_jobs::find_import_errors(&state.inner.pool, id, page, per_page).await?;
    Ok(Json(PaginatedImportErrorResponse {
        data,
        page,
        per_page,
        total,
    }))
}

#[derive(Debug, Serialize)]
struct PaginatedImportErrorResponse {
    data: Vec<ImportJobError>,
    page: u32,
    per_page: u32,
    total: u32,
}

async fn get_import_review_summary(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<ReviewSummary>, AppError> {
    Ok(Json(
        publication::evaluate_activation_eligibility(&state.inner, id).await?,
    ))
}

#[derive(Debug, Deserialize, Default)]
struct ReviewItemParams {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
    q: Option<String>,
    outcome: Option<String>,
    severity: Option<String>,
    cover_status: Option<String>,
    #[serde(default)]
    sample: bool,
}

async fn get_import_review_items(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Query(params): Query<ReviewItemParams>,
) -> Result<Json<PaginatedReviewItems>, AppError> {
    if import_jobs::find_import_job_by_id(&state.inner.pool, id)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound(format!("Import job {id} not found")));
    }
    validate_review_filter(
        params.outcome.as_deref(),
        &[
            "not_processed",
            "created",
            "updated",
            "unchanged",
            "skipped",
            "failed",
        ],
        "outcome",
    )?;
    validate_review_filter(
        params.severity.as_deref(),
        &["info", "warning", "blocking"],
        "severity",
    )?;
    validate_review_filter(
        params.cover_status.as_deref(),
        &[
            "imported",
            "reused",
            "missing_at_source",
            "not_permitted",
            "fetch_failed",
            "invalid",
            "storage_failed",
            "not_checked",
        ],
        "cover_status",
    )?;
    if params.q.as_deref().is_some_and(|query| query.len() > 200) {
        return Err(AppError::BadRequest(
            "Review search must not exceed 200 characters".to_string(),
        ));
    }
    let issue_numbers = if params.sample {
        publication::evaluate_activation_eligibility(&state.inner, id)
            .await?
            .sample_issue_numbers
    } else {
        Vec::new()
    };
    let filter = import_review::ReviewItemFilter {
        query: params.q,
        outcome: params.outcome,
        severity: params.severity,
        cover_status: params.cover_status,
        issue_numbers,
    };
    let page = params.page.max(1);
    let per_page = params.per_page.clamp(1, 100);
    let total = import_review::count_review_items(&state.inner.pool, id, &filter).await?;
    let items =
        import_review::find_review_items(&state.inner.pool, id, &filter, page, per_page).await?;
    Ok(Json(PaginatedReviewItems {
        items,
        total,
        page,
        per_page,
    }))
}

fn validate_review_filter(
    value: Option<&str>,
    allowed: &[&str],
    field: &str,
) -> Result<(), AppError> {
    if let Some(value) = value
        && !allowed.contains(&value)
    {
        return Err(AppError::BadRequest(format!(
            "Unknown review {field} filter '{value}'"
        )));
    }
    Ok(())
}

async fn activate_import(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Json(request): Json<ActivateImportRequest>,
) -> Result<Json<ActivationResponse>, AppError> {
    let response = publication::activate_from_import(
        &state.inner,
        id,
        admin.0.user_id,
        request.acknowledge_warnings,
    )
    .await?;
    tracing::info!(
        job_id = id,
        series_id = response.series_id,
        "Series activated after import review"
    );
    Ok(Json(response))
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
        "SELECT j.id, j.series_id, s.slug AS series_slug, j.adapter_name, j.source_key, j.trigger_type, \
         j.scheduled_for, j.status, j.total_issues, j.imported_issues, j.failed_issues, \
         j.created_issues, j.updated_issues, j.unchanged_issues, j.skipped_issues, \
         j.error_message, j.started_by, j.started_at, j.completed_at, j.created_at, j.updated_at, \
         j.cancel_requested_at, j.retry_of_job_id \
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
            source_key: r.source_key.clone(),
            trigger_type: r.trigger_type.clone(),
            scheduled_for: r.scheduled_for,
            status: r.status.clone(),
            total_issues: r.total_issues,
            imported_issues: r.imported_issues,
            created_issues: r.created_issues,
            updated_issues: r.updated_issues,
            unchanged_issues: r.unchanged_issues,
            skipped_issues: r.skipped_issues,
            failed_issues: r.failed_issues,
            error_message: r.error_message.clone(),
            started_by: r.started_by,
            started_at: r.started_at,
            completed_at: r.completed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            cancel_requested_at: r.cancel_requested_at,
            retry_of_job_id: r.retry_of_job_id,
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
            source_key: "maddraxikon".to_string(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "maddrax");
        assert_eq!(json["display_name"], "Maddrax");
        assert_eq!(json["version"], "0.9");
        assert_eq!(json["source_key"], "maddraxikon");
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

    #[test]
    fn review_filters_reject_unknown_database_enum_values() {
        assert!(validate_review_filter(Some("created"), &["created"], "outcome").is_ok());
        assert!(matches!(
            validate_review_filter(Some("unknown"), &["created"], "outcome"),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn cancellation_noop_reports_the_concurrent_job_state() {
        assert_eq!(
            cancellation_status_after_noop(5, "cancelled", true).unwrap(),
            StatusCode::OK
        );
        assert_eq!(
            cancellation_status_after_noop(5, "running", true).unwrap(),
            StatusCode::ACCEPTED
        );

        let error = cancellation_status_after_noop(5, "completed", false).unwrap_err();
        assert!(matches!(error, AppError::Conflict(_)));
        assert!(error.to_string().contains("reached status 'completed'"));
    }
}
