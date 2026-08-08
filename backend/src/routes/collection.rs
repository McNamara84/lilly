use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::collection;
use crate::error::AppError;
use crate::models::collection::{
    AddCollectionEntryRequest, CollectionEntryResponse, CollectionQueryParams,
    CollectionStatsResponse, PaginatedCollectionResponse, SeriesStatsEntry,
    UpdateCollectionEntryRequest, normalize_collection_note, validate_collection_sort,
    validate_condition_grade, validate_missing_collection_sort, validate_sort_direction,
    validate_status_condition,
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
    if let Some(ref status) = params.status
        && status != "missing"
        && status != "owned"
        && status != "duplicate"
        && status != "wanted"
    {
        return Err(AppError::BadRequest(format!(
            "Invalid status filter '{status}'. Must be one of: owned, duplicate, wanted, missing"
        )));
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
    if let Some(ref condition) = params.condition {
        validate_condition_grade(condition).map_err(AppError::BadRequest)?;
    }
    if params.issue_number == Some(0) {
        return Err(AppError::BadRequest(
            "issue_number must be greater than zero".to_string(),
        ));
    }
    if let Some(ref sort) = params.sort {
        validate_collection_sort(sort).map_err(AppError::BadRequest)?;
    }
    if let Some(ref direction) = params.sort_dir {
        validate_sort_direction(direction).map_err(AppError::BadRequest)?;
    }

    // Handle the virtual "missing" status via a separate query path
    if params.status.as_deref() == Some("missing") {
        params.series_slug.as_deref().ok_or_else(|| {
            AppError::BadRequest(
                "series_slug is required when filtering by status=missing".to_string(),
            )
        })?;
        if params.condition.is_some()
            || params.condition_min.is_some()
            || params.condition_max.is_some()
        {
            return Err(AppError::BadRequest(
                "condition filters cannot be used with status=missing".to_string(),
            ));
        }
        if let Some(ref sort) = params.sort {
            validate_missing_collection_sort(sort).map_err(AppError::BadRequest)?;
        }

        let per_page = params.per_page.clamp(1, 100);
        let page = params.page.max(1);

        let total =
            collection::count_missing_issues(&state.inner.pool, auth.user_id, &params).await?;

        let missing =
            collection::find_missing_issues(&state.inner.pool, auth.user_id, &params).await?;

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
    let status = body.status.as_deref().unwrap_or("owned");
    validate_status_condition(status, body.condition_grade.as_deref())
        .map_err(AppError::BadRequest)?;
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

    let notes = normalize_collection_note(body.notes.as_deref()).map_err(AppError::BadRequest)?;

    let entry_id = collection::add_entry(
        &state.inner.pool,
        auth.user_id,
        body.issue_id,
        copy_number,
        body.condition_grade.as_deref(),
        status,
        notes,
    )
    .await
    .map_err(|e| {
        // Detect duplicate key violation
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.kind() == sqlx::error::ErrorKind::UniqueViolation
        {
            return AppError::BadRequest(
                "Duplicate entry: this issue with the same copy number already exists in your collection".to_string(),
            );
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
    // Reject empty updates — at least one field must be provided
    if body.condition_grade.is_none() && body.status.is_none() && body.notes.is_none() {
        return Err(AppError::BadRequest(
            "At least one field (condition_grade, status, or notes) must be provided".to_string(),
        ));
    }

    // Ensure the entry exists and belongs to the user
    let existing = collection::find_entry_by_id_and_user(&state.inner.pool, entry_id, auth.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection entry {entry_id} not found")))?;

    let final_status = body.status.as_deref().unwrap_or(&existing.status);
    let final_condition = body
        .condition_grade
        .as_deref()
        .or(existing.condition_grade.as_deref());
    validate_status_condition(final_status, final_condition).map_err(AppError::BadRequest)?;

    // A present but empty note explicitly clears the stored value. Omitting the
    // field leaves it unchanged, preserving PATCH semantics.
    let notes_param = body
        .notes
        .as_deref()
        .map(|note| normalize_collection_note(Some(note)))
        .transpose()
        .map_err(AppError::BadRequest)?;

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

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total_owned = stats.total_owned as u32;

    let series_stats = series
        .iter()
        .map(|s| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let imported = s.imported_total as u32;
            let total = resolve_series_total(s.declared_total, imported);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let owned = s.owned_count as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let duplicate = s.duplicate_count as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let wanted = s.wanted_count as u32;

            SeriesStatsEntry {
                series_id: s.series_id,
                series_name: s.series_name.clone(),
                series_slug: s.series_slug.clone(),
                total_in_series: total,
                owned_count: owned,
                duplicate_count: duplicate,
                wanted_count: wanted,
                progress_percent: calculate_progress(owned, total),
            }
        })
        .collect::<Vec<_>>();

    let (total_issues, overall_progress) = calculate_overall_stats(&series_stats);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(Json(CollectionStatsResponse {
        total_issues,
        total_owned,
        total_duplicate: stats.total_duplicate as u32,
        total_wanted: stats.total_wanted as u32,
        overall_progress_percent: overall_progress,
        series_stats,
    }))
}

fn resolve_series_total(declared_total: Option<u32>, imported_total: u32) -> Option<u32> {
    match declared_total {
        Some(declared) => Some(declared.max(imported_total)),
        None if imported_total > 0 => Some(imported_total),
        None => None,
    }
}

fn calculate_progress(owned: u32, total: Option<u32>) -> Option<f64> {
    total
        .filter(|total| *total > 0)
        .map(|total| (f64::from(owned) / f64::from(total)) * 100.0)
}

fn calculate_overall_stats(series_stats: &[SeriesStatsEntry]) -> (Option<u32>, Option<f64>) {
    if series_stats.is_empty()
        || series_stats
            .iter()
            .any(|series| series.total_in_series.is_none())
    {
        return (None, None);
    }

    let total_issues = series_stats.iter().fold(0u32, |sum, series| {
        sum.saturating_add(series.total_in_series.unwrap_or_default())
    });
    let total_owned = series_stats.iter().fold(0u32, |sum, series| {
        if series.total_in_series.is_some_and(|total| total > 0) {
            sum.saturating_add(series.owned_count)
        } else {
            sum
        }
    });
    let total_issues = Some(total_issues);

    (total_issues, calculate_progress(total_owned, total_issues))
}

#[cfg(test)]
mod tests {
    use super::{calculate_overall_stats, calculate_progress, resolve_series_total};
    use crate::models::collection::{
        SeriesStatsEntry, validate_collection_sort, validate_condition_grade,
        validate_sort_direction, validate_status,
    };

    fn series_stats(owned_count: u32, total_in_series: Option<u32>) -> SeriesStatsEntry {
        SeriesStatsEntry {
            series_id: 1,
            series_name: "Test series".to_string(),
            series_slug: "test-series".to_string(),
            total_in_series,
            owned_count,
            duplicate_count: 0,
            wanted_count: 0,
            progress_percent: calculate_progress(owned_count, total_in_series),
        }
    }

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
        for g in &["Z0", "Z1", "Z2", "Z3", "Z4"] {
            assert!(validate_condition_grade(g).is_ok());
        }
        assert!(validate_condition_grade("Z5").is_err());
        assert!(validate_condition_grade("Z6").is_err());
    }

    #[test]
    fn test_sort_filter_values() {
        for sort in &[
            "series",
            "issue_number",
            "condition",
            "title",
            "author",
            "added",
        ] {
            assert!(validate_collection_sort(sort).is_ok());
        }
        assert!(validate_collection_sort("unknown").is_err());
        assert!(validate_sort_direction("asc").is_ok());
        assert!(validate_sort_direction("desc").is_ok());
        assert!(validate_sort_direction("random").is_err());
    }

    #[test]
    fn series_total_prefers_the_larger_known_value() {
        assert_eq!(resolve_series_total(Some(620), 600), Some(620));
        assert_eq!(resolve_series_total(Some(600), 620), Some(620));
        assert_eq!(resolve_series_total(Some(0), 0), Some(0));
        assert_eq!(resolve_series_total(None, 620), Some(620));
        assert_eq!(resolve_series_total(None, 0), None);
    }

    #[test]
    fn progress_handles_known_unknown_and_zero_totals() {
        assert_eq!(calculate_progress(50, Some(200)), Some(25.0));
        assert_eq!(calculate_progress(0, Some(0)), None);
        assert_eq!(calculate_progress(0, None), None);
    }

    #[test]
    fn overall_stats_require_every_series_total_to_be_known() {
        let mixed_totals = [series_stats(10, Some(10)), series_stats(0, None)];
        assert_eq!(calculate_overall_stats(&mixed_totals), (None, None));

        let known_totals = [series_stats(10, Some(10)), series_stats(5, Some(10))];
        assert_eq!(
            calculate_overall_stats(&known_totals),
            (Some(20), Some(75.0))
        );

        assert_eq!(calculate_overall_stats(&[]), (None, None));
    }
}
