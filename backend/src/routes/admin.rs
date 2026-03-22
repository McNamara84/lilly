use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::middleware::AdminUser;
use crate::db::{import_jobs, issues, series};
use crate::error::AppError;
use crate::models::series::{ImportJobResponse, IssueResponse, SeriesResponse};

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
        .route("/api/v1/admin/import/history", get(import_history))
        .route("/api/v1/admin/import/{id}", get(get_import_job))
        .route(
            "/api/v1/admin/import/{id}/series-issues",
            get(get_import_series_issues),
        )
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
    status: String,
    total_issues: u32,
    imported_issues: u32,
    error_message: Option<String>,
    started_by: u32,
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
    // Verify adapter exists
    let adapter = state
        .inner
        .adapter_registry
        .get(&request.adapter)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown adapter: '{}'", request.adapter)))?;

    // Fetch series metadata from the adapter
    let metadata = adapter
        .fetch_series_metadata()
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;

    // Create or find series
    let series_id = match series::find_series_by_slug(&state.inner.pool, &metadata.slug).await? {
        Some(existing) => existing.id,
        None => {
            series::create_series(
                &state.inner.pool,
                &metadata.name,
                &metadata.slug,
                metadata.publisher.as_deref(),
                metadata.genre.as_deref(),
                metadata.frequency.as_deref(),
                metadata.total_issues,
                &metadata.status.to_string(),
                metadata.source_url.as_deref(),
            )
            .await?
        }
    };

    // Atomically check for active imports and create job (prevents race condition)
    let job_id = import_jobs::create_import_job_if_idle(
        &state.inner.pool,
        series_id,
        &request.adapter,
        admin.0.user_id,
    )
    .await?
    .ok_or_else(|| {
        AppError::BadRequest("An import is already running for this series".to_string())
    })?;

    // Spawn background import task
    let pool = state.inner.pool.clone();
    let media_path = state.inner.media_path.clone();
    let adapter_name = request.adapter.clone();
    // Clone the Arc to keep state alive for the spawned task
    let state_inner = state.inner.clone();

    tokio::spawn(async move {
        if let Err(e) = run_import(
            state_inner,
            pool.clone(),
            media_path,
            adapter_name,
            series_id,
            job_id,
        )
        .await
        {
            tracing::error!(job_id, error = %e, "Import task failed");
            if let Err(db_err) = import_jobs::fail_import_job(&pool, job_id, &e.to_string()).await {
                tracing::error!(job_id, error = %db_err, "Failed to mark import job as failed");
            }
        }
    });

    // Return the job immediately
    let job = import_jobs::find_import_job_by_id(&state.inner.pool, job_id)
        .await?
        .ok_or_else(|| {
            AppError::InternalError(anyhow::anyhow!("Failed to retrieve created import job"))
        })?;

    Ok(Json(ImportJobResponse::from_job_with_slug(
        &job,
        metadata.slug,
    )))
}

#[allow(clippy::too_many_lines)]
async fn run_import(
    state_inner: std::sync::Arc<super::AppStateInner>,
    pool: sqlx::MySqlPool,
    media_path: std::path::PathBuf,
    adapter_name: String,
    series_id: u32,
    job_id: u32,
) -> Result<(), anyhow::Error> {
    let adapter = state_inner
        .adapter_registry
        .get(&adapter_name)
        .ok_or_else(|| anyhow::anyhow!("Adapter '{adapter_name}' not found"))?;

    // Fetch issue list
    let issue_numbers = adapter
        .fetch_issue_list()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch issue list: {e}"))?;

    // Determine which issues are new (not yet imported)
    let existing = issues::find_existing_issue_numbers(&pool, series_id).await?;
    let new_issues: Vec<u32> = issue_numbers
        .iter()
        .copied()
        .filter(|n| !existing.contains(n))
        .collect();

    if !existing.is_empty() {
        tracing::info!(
            job_id,
            existing = existing.len(),
            new = new_issues.len(),
            "Incremental import: skipping {} existing issues",
            existing.len()
        );
    }

    let total = u32::try_from(new_issues.len()).unwrap_or(u32::MAX);
    import_jobs::update_import_progress(&pool, job_id, 0, total).await?;

    // Create covers directory
    let cover_dir = media_path
        .join("covers")
        .join(format!("series-{series_id}"));
    tokio::fs::create_dir_all(&cover_dir).await?;

    let mut imported = 0u32;
    for issue_number in &new_issues {
        // Fetch issue details
        let details = match adapter.fetch_issue_details(*issue_number).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(issue_number, error = %e, "Failed to fetch issue details, skipping");
                imported += 1;
                import_jobs::update_import_progress(&pool, job_id, imported, total).await?;
                continue;
            }
        };

        // Fetch cover
        let mut cover_local = None;
        if let Ok(Some(cover_data)) = adapter.fetch_cover(*issue_number).await {
            let ext = if cover_data.content_type.contains("png") {
                "png"
            } else if cover_data.content_type.contains("webp") {
                "webp"
            } else {
                "jpg"
            };
            let cover_path = cover_dir.join(format!("{issue_number}.{ext}"));
            match tokio::fs::write(&cover_path, &cover_data.bytes).await {
                Ok(()) => {
                    let url_prefix = &state_inner.media_url_prefix;
                    cover_local = Some(format!(
                        "{url_prefix}/covers/series-{series_id}/{issue_number}.{ext}"
                    ));
                }
                Err(e) => {
                    tracing::warn!(
                        issue_number,
                        path = %cover_path.display(),
                        error = %e,
                        "Failed to write cover file"
                    );
                }
            }
        }

        // Upsert issue
        if let Err(e) = issues::upsert_issue(
            &pool,
            series_id,
            *issue_number,
            &details.title,
            details.published_at,
            details.cycle.as_deref(),
            None, // cover_url from wiki not stored
            cover_local.as_deref(),
            details.source_wiki_url.as_deref(),
        )
        .await
        {
            tracing::warn!(issue_number, error = %e, "Failed to upsert issue");
            imported += 1;
            import_jobs::update_import_progress(&pool, job_id, imported, total).await?;
            continue;
        }

        // Look up the issue id for setting relations
        let issue_id = match issues::find_issue_id(&pool, series_id, *issue_number).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::warn!(issue_number, "Issue upserted but not found for relations");
                imported += 1;
                import_jobs::update_import_progress(&pool, job_id, imported, total).await?;
                continue;
            }
            Err(e) => {
                tracing::warn!(issue_number, error = %e, "Failed to resolve issue id");
                imported += 1;
                import_jobs::update_import_progress(&pool, job_id, imported, total).await?;
                continue;
            }
        };

        // Set normalized relations
        if let Err(e) = issues::set_issue_persons(&pool, issue_id, &details.authors, "author").await
        {
            tracing::warn!(issue_number, error = %e, "Failed to set authors");
        }
        if let Err(e) =
            issues::set_issue_persons(&pool, issue_id, &details.cover_artists, "cover_artist").await
        {
            tracing::warn!(issue_number, error = %e, "Failed to set cover artists");
        }
        if let Err(e) = issues::set_issue_keywords(&pool, issue_id, &details.keywords).await {
            tracing::warn!(issue_number, error = %e, "Failed to set keywords");
        }
        if let Err(e) = issues::set_issue_notes(&pool, issue_id, &details.notes).await {
            tracing::warn!(issue_number, error = %e, "Failed to set notes");
        }

        imported += 1;
        import_jobs::update_import_progress(&pool, job_id, imported, total).await?;
    }

    import_jobs::complete_import_job(&pool, job_id).await?;

    // Update series total_issues from actual count (existing + newly imported)
    let actual_count = issues::count_issues_by_series(&pool, series_id).await?;
    series::update_series_total_issues(&pool, series_id, actual_count).await?;

    tracing::info!(job_id, imported, total, "Import completed");

    Ok(())
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
        "SELECT j.id, j.series_id, s.slug AS series_slug, j.adapter_name, j.status, \
         j.total_issues, j.imported_issues, j.error_message, j.started_by, \
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
            status: r.status.clone(),
            total_issues: r.total_issues,
            imported_issues: r.imported_issues,
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
