use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::collection;
use crate::error::AppError;
use crate::models::collection::{
    validate_condition_grade, validate_status, AddCollectionEntryRequest, CollectionEntryResponse,
    CollectionQueryParams, CollectionStatsResponse, PaginatedCollectionResponse, SeriesStatsEntry,
    UpdateCollectionEntryRequest,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/collection", get(list_collection))
        .route("/api/v1/me/collection", post(add_to_collection))
        .route("/api/v1/me/collection/{id}", patch(update_entry))
        .route("/api/v1/me/collection/{id}", delete(delete_entry))
        .route(
            "/api/v1/me/collection/by-issue/{issue_id}",
            get(get_entry_by_issue),
        )
        .route("/api/v1/me/collection/stats", get(collection_stats))
}

// ---------------------------------------------------------------------------
// GET /api/v1/me/collection
// ---------------------------------------------------------------------------

async fn list_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<CollectionQueryParams>,
) -> Result<Json<PaginatedCollectionResponse>, AppError> {
    // Validate optional filter values
    if let Some(ref status) = params.status {
        if status != "missing" && status != "owned" && status != "duplicate" && status != "wanted" {
            return Err(AppError::BadRequest(format!(
                "Invalid status filter '{status}'. Must be one of: owned, duplicate, wanted, missing"
            )));
        }
    }
    if let Some(ref g) = params.condition_min {
        validate_condition_grade(g).map_err(AppError::BadRequest)?;
    }
    if let Some(ref g) = params.condition_max {
        validate_condition_grade(g).map_err(AppError::BadRequest)?;
    }
    // Both condition bounds must be provided together or not at all
    if params.condition_min.is_some() != params.condition_max.is_some() {
        return Err(AppError::BadRequest(
            "condition_min and condition_max must be provided together".to_string(),
        ));
    }

    // Handle the virtual "missing" status via a separate query path
    if params.status.as_deref() == Some("missing") {
        let series_slug = params.series_slug.as_deref().ok_or_else(|| {
            AppError::BadRequest(
                "series_slug is required when filtering by status=missing".to_string(),
            )
        })?;

        let per_page = params.per_page.clamp(1, 100);
        let page = params.page.max(1);

        let total =
            collection::count_missing_issues(&state.inner.pool, auth.user_id, series_slug).await?;

        let missing = collection::find_missing_issues(
            &state.inner.pool,
            auth.user_id,
            series_slug,
            page,
            per_page,
        )
        .await?;

        let data = missing
            .iter()
            .map(|m| CollectionEntryResponse {
                id: 0,
                issue_id: m.issue_id,
                issue_number: m.issue_number,
                title: m.title.clone(),
                series_id: m.series_id,
                series_name: m.series_name.clone(),
                series_slug: m.series_slug.clone(),
                cover_url: m.cover_url.clone(),
                cover_local_path: m.cover_local_path.clone(),
                copy_number: None,
                condition_grade: None,
                status: "missing".to_string(),
                notes: None,
                created_at: None,
                updated_at: None,
            })
            .collect();

        return Ok(Json(PaginatedCollectionResponse {
            data,
            page,
            per_page,
            total,
        }));
    }

    let total =
        collection::count_collection_entries(&state.inner.pool, auth.user_id, &params).await?;

    let entries =
        collection::find_collection_entries(&state.inner.pool, auth.user_id, &params).await?;

    let data = entries.iter().map(CollectionEntryResponse::from).collect();

    Ok(Json(PaginatedCollectionResponse {
        data,
        page: params.page.max(1),
        per_page: params.per_page.clamp(1, 100),
        total,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/v1/me/collection
// ---------------------------------------------------------------------------

async fn add_to_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AddCollectionEntryRequest>,
) -> Result<(StatusCode, Json<CollectionEntryResponse>), AppError> {
    // Validate fields
    validate_condition_grade(&body.condition_grade).map_err(AppError::BadRequest)?;
    let status = body.status.as_deref().unwrap_or("owned");
    validate_status(status).map_err(AppError::BadRequest)?;
    let copy_number = body.copy_number.unwrap_or(1);
    if copy_number < 1 {
        return Err(AppError::BadRequest(
            "copy_number must be at least 1".to_string(),
        ));
    }

    // Ensure the issue exists and belongs to an active series
    if !collection::is_issue_in_active_series(&state.inner.pool, body.issue_id).await? {
        return Err(AppError::NotFound(format!(
            "Issue {} not found",
            body.issue_id
        )));
    }

    let entry_id = collection::add_entry(
        &state.inner.pool,
        auth.user_id,
        body.issue_id,
        copy_number,
        &body.condition_grade,
        status,
        body.notes.as_deref(),
    )
    .await
    .map_err(|e| {
        // Detect duplicate key violation (MariaDB/MySQL errno 1062)
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.code().as_deref() == Some("23000")
                || db_err
                    .kind()
                    == sqlx::error::ErrorKind::UniqueViolation
            {
                return AppError::BadRequest(
                    "Duplicate entry: this issue with the same copy number already exists in your collection".to_string(),
                );
            }
        }
        AppError::from(e)
    })?;

    let row = collection::find_entry_row_by_id_and_user(&state.inner.pool, entry_id, auth.user_id)
        .await?
        .ok_or_else(|| {
            AppError::InternalError(anyhow::anyhow!("Failed to retrieve newly created entry"))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CollectionEntryResponse::from(&row)),
    ))
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/me/collection/:id
// ---------------------------------------------------------------------------

async fn update_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(entry_id): Path<u32>,
    Json(body): Json<UpdateCollectionEntryRequest>,
) -> Result<Json<CollectionEntryResponse>, AppError> {
    // Validate optional fields
    if let Some(ref grade) = body.condition_grade {
        validate_condition_grade(grade).map_err(AppError::BadRequest)?;
    }
    if let Some(ref s) = body.status {
        validate_status(s).map_err(AppError::BadRequest)?;
    }

    // Reject empty updates — at least one field must be provided
    if body.condition_grade.is_none() && body.status.is_none() && body.notes.is_none() {
        return Err(AppError::BadRequest(
            "At least one field (condition_grade, status, or notes) must be provided".to_string(),
        ));
    }

    // Ensure the entry exists and belongs to the user
    collection::find_entry_by_id_and_user(&state.inner.pool, entry_id, auth.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection entry {entry_id} not found")))?;

    // Convert Option<String> → Option<Option<&str>> for notes
    let notes_param: Option<Option<&str>> = if body.notes.is_some() {
        Some(body.notes.as_deref())
    } else {
        None
    };

    collection::update_entry(
        &state.inner.pool,
        entry_id,
        auth.user_id,
        body.condition_grade.as_deref(),
        body.status.as_deref(),
        notes_param,
    )
    .await?;

    let row = collection::find_entry_row_by_id_and_user(&state.inner.pool, entry_id, auth.user_id)
        .await?
        .ok_or_else(|| {
            AppError::InternalError(anyhow::anyhow!("Failed to retrieve updated entry"))
        })?;

    Ok(Json(CollectionEntryResponse::from(&row)))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/me/collection/:id
// ---------------------------------------------------------------------------

async fn delete_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(entry_id): Path<u32>,
) -> Result<StatusCode, AppError> {
    let deleted = collection::delete_entry(&state.inner.pool, entry_id, auth.user_id).await?;

    if !deleted {
        return Err(AppError::NotFound(format!(
            "Collection entry {entry_id} not found"
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/v1/me/collection/by-issue/:issue_id
// ---------------------------------------------------------------------------

async fn get_entry_by_issue(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(issue_id): Path<u32>,
) -> Result<Json<Option<CollectionEntryResponse>>, AppError> {
    let row =
        collection::find_entry_row_by_issue_and_user(&state.inner.pool, issue_id, auth.user_id)
            .await?;
    Ok(Json(row.as_ref().map(CollectionEntryResponse::from)))
}

// ---------------------------------------------------------------------------
// GET /api/v1/me/collection/stats
// ---------------------------------------------------------------------------

#[allow(clippy::similar_names)]
async fn collection_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<CollectionStatsResponse>, AppError> {
    let stats = collection::get_collection_stats(&state.inner.pool, auth.user_id).await?;
    let series = collection::get_series_stats(&state.inner.pool, auth.user_id).await?;

    // Calculate total issues across all active series the user collects
    let total_issues_in_series: u32 = series
        .iter()
        .map(|s| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                s.total_in_series as u32
            }
        })
        .sum();

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total_owned = stats.total_owned as u32;

    let overall_progress = if total_issues_in_series > 0 {
        (f64::from(total_owned) / f64::from(total_issues_in_series)) * 100.0
    } else {
        0.0
    };

    let series_stats = series
        .iter()
        .map(|s| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let total = s.total_in_series as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let owned = s.owned_count as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let duplicate = s.duplicate_count as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let wanted = s.wanted_count as u32;

            let progress = if total > 0 {
                (f64::from(owned) / f64::from(total)) * 100.0
            } else {
                0.0
            };

            SeriesStatsEntry {
                series_id: s.series_id,
                series_name: s.series_name.clone(),
                series_slug: s.series_slug.clone(),
                total_in_series: total,
                owned_count: owned,
                duplicate_count: duplicate,
                wanted_count: wanted,
                progress_percent: progress,
            }
        })
        .collect();

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(Json(CollectionStatsResponse {
        total_issues: total_issues_in_series,
        total_owned,
        total_duplicate: stats.total_duplicate as u32,
        total_wanted: stats.total_wanted as u32,
        overall_progress_percent: overall_progress,
        series_stats,
    }))
}

#[cfg(test)]
mod tests {
    use crate::models::collection::{validate_condition_grade, validate_status};

    #[test]
    fn test_status_filter_values() {
        // Valid collection statuses
        for s in &["owned", "duplicate", "wanted"] {
            assert!(validate_status(s).is_ok());
        }
        // "missing" is virtual (not stored) → rejected by validate_status
        assert!(validate_status("missing").is_err());
    }

    #[test]
    fn test_condition_grade_filter_values() {
        for g in &["Z0", "Z1", "Z2", "Z3", "Z4", "Z5"] {
            assert!(validate_condition_grade(g).is_ok());
        }
        assert!(validate_condition_grade("Z6").is_err());
    }
}
