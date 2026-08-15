use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use sha2::{Digest, Sha256};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::{collection, issues, series};
use crate::error::AppError;
use crate::models::collection::{
    AddCollectionEntryRequest, CollectionEntryResponse, CollectionQueryParams,
    CollectionStatsResponse, CollectionSyncMutation, CollectionSyncRequest, CollectionSyncResponse,
    CollectionSyncResult, CollectionSyncStatus, MAX_SYNC_MUTATIONS,
    OfflineCollectionSnapshotResponse, PaginatedCollectionResponse, SeriesStatsEntry,
    UpdateCollectionEntryRequest, normalize_collection_note, normalize_edition_label,
    validate_collection_sort, validate_condition_grade, validate_missing_collection_sort,
    validate_mutation_id, validate_sort_direction, validate_status_condition,
};
use crate::services::trade_matching;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/collection", get(list_collection))
        .route("/api/v1/me/collection", post(add_to_collection))
        .route(
            "/api/v1/me/collection/offline-snapshot",
            get(offline_snapshot),
        )
        .route("/api/v1/me/collection/sync", post(sync_collection))
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
    validate_collection_query(&params)?;

    // Handle the virtual "missing" status via a separate query path
    if params.status.as_deref() == Some("missing") {
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
                edition_label: None,
                condition_grade: None,
                status: "missing".to_string(),
                notes: None,
                revision: None,
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
// GET /api/v1/me/collection/offline-snapshot
// ---------------------------------------------------------------------------

async fn offline_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<OfflineCollectionSnapshotResponse>, AppError> {
    let catalog_series = series::find_all_series(&state.inner.pool, true).await?;
    let catalog_series = catalog_series
        .into_iter()
        .filter(|item| matches!(item.slug.as_str(), "maddrax" | "john-sinclair"))
        .collect::<Vec<_>>();

    let mut catalog_issues = Vec::new();
    for item in &catalog_series {
        let issue_rows = issues::find_all_issues_by_series(&state.inner.pool, item.id).await?;
        catalog_issues.extend(issues::build_issue_responses(&state.inner.pool, &issue_rows).await?);
    }

    let collection_entries =
        collection::find_all_collection_entries_for_user(&state.inner.pool, auth.user_id)
            .await?
            .iter()
            .map(CollectionEntryResponse::from)
            .collect();
    let generated_at = chrono::Utc::now();

    Ok(Json(OfflineCollectionSnapshotResponse {
        schema_version: 1,
        snapshot_version: generated_at.timestamp_millis().to_string(),
        user_id: auth.user_id,
        generated_at,
        series: catalog_series
            .iter()
            .map(crate::models::series::SeriesResponse::from)
            .collect(),
        issues: catalog_issues,
        collection_entries,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/v1/me/collection/sync
// ---------------------------------------------------------------------------

async fn sync_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CollectionSyncRequest>,
) -> Result<Json<CollectionSyncResponse>, AppError> {
    if body.mutations.len() > MAX_SYNC_MUTATIONS {
        return Err(AppError::BadRequest(format!(
            "A sync request must not contain more than {MAX_SYNC_MUTATIONS} mutations"
        )));
    }

    let mut results = Vec::with_capacity(body.mutations.len());
    for mutation in body.mutations {
        results.push(process_sync_mutation(&state, &auth, mutation).await?);
    }

    Ok(Json(CollectionSyncResponse { results }))
}

async fn process_sync_mutation(
    state: &AppState,
    auth: &AuthUser,
    mutation: CollectionSyncMutation,
) -> Result<CollectionSyncResult, AppError> {
    if let Err(message) = validate_mutation_id(mutation.mutation_id()) {
        return Ok(rejected_sync_result(
            mutation.mutation_id(),
            message,
            "invalid_mutation_id",
        ));
    }

    let fingerprint = mutation_fingerprint(&mutation)?;
    if let Some(result) = find_stored_sync_result(state, auth, &mutation, &fingerprint).await? {
        return Ok(result);
    }

    let result = apply_sync_mutation(state, auth, &mutation).await?;
    persist_sync_result(state, auth, &mutation, &fingerprint, result).await
}

async fn find_stored_sync_result(
    state: &AppState,
    auth: &AuthUser,
    mutation: &CollectionSyncMutation,
    fingerprint: &str,
) -> Result<Option<CollectionSyncResult>, AppError> {
    let mut receipt_transaction = state.inner.pool.begin().await?;
    let receipt = collection::find_mutation_receipt_on_connection(
        &mut receipt_transaction,
        auth.user_id,
        mutation.mutation_id(),
    )
    .await?;
    receipt_transaction.commit().await?;

    Ok(receipt.map(|receipt| {
        if receipt.operation != mutation.operation() || receipt.request_fingerprint != fingerprint {
            rejected_sync_result(
                mutation.mutation_id(),
                "mutation_id was already used for a different request".to_string(),
                "mutation_id_reused",
            )
        } else {
            let mut result = receipt.result_json.0;
            if result.status == CollectionSyncStatus::Applied {
                result.status = CollectionSyncStatus::AlreadyApplied;
            }
            result
        }
    }))
}

async fn apply_sync_mutation(
    state: &AppState,
    auth: &AuthUser,
    mutation: &CollectionSyncMutation,
) -> Result<CollectionSyncResult, AppError> {
    let result = match mutation {
        CollectionSyncMutation::Create { mutation_id, entry } => {
            match add_to_collection(State(state.clone()), auth.clone(), Json(entry.clone())).await {
                Ok((_, Json(entry))) => CollectionSyncResult {
                    mutation_id: mutation_id.clone(),
                    status: CollectionSyncStatus::Applied,
                    entry: Some(entry),
                    error: None,
                    code: None,
                },
                Err(error) => sync_result_from_error(mutation.mutation_id(), error)?,
            }
        }
        CollectionSyncMutation::Update {
            mutation_id,
            entry_id,
            base_revision,
            changes,
        } => {
            let mut changes = changes.clone();
            changes.base_revision = Some(*base_revision);
            match update_entry(
                State(state.clone()),
                auth.clone(),
                Path(*entry_id),
                Json(changes),
            )
            .await
            {
                Ok(Json(entry)) => CollectionSyncResult {
                    mutation_id: mutation_id.clone(),
                    status: CollectionSyncStatus::Applied,
                    entry: Some(entry),
                    error: None,
                    code: None,
                },
                Err(AppError::ConflictWithCode { message, code })
                    if code == "collection_revision_conflict" =>
                {
                    let current = collection::find_entry_row_by_id_and_user(
                        &state.inner.pool,
                        *entry_id,
                        auth.user_id,
                    )
                    .await?;
                    CollectionSyncResult {
                        mutation_id: mutation_id.clone(),
                        status: CollectionSyncStatus::Conflict,
                        entry: current.as_ref().map(CollectionEntryResponse::from),
                        error: Some(message),
                        code: Some(code),
                    }
                }
                Err(error) => sync_result_from_error(mutation.mutation_id(), error)?,
            }
        }
    };

    Ok(result)
}

async fn persist_sync_result(
    state: &AppState,
    auth: &AuthUser,
    mutation: &CollectionSyncMutation,
    fingerprint: &str,
    result: CollectionSyncResult,
) -> Result<CollectionSyncResult, AppError> {
    let mut transaction = state.inner.pool.begin().await?;
    if let Some(receipt) = collection::find_mutation_receipt_on_connection(
        &mut transaction,
        auth.user_id,
        mutation.mutation_id(),
    )
    .await?
    {
        transaction.commit().await?;
        if receipt.operation == mutation.operation() && receipt.request_fingerprint == fingerprint {
            let mut stored = receipt.result_json.0;
            if stored.status == CollectionSyncStatus::Applied {
                stored.status = CollectionSyncStatus::AlreadyApplied;
            }
            return Ok(stored);
        }
        return Ok(rejected_sync_result(
            mutation.mutation_id(),
            "mutation_id was already used for a different request".to_string(),
            "mutation_id_reused",
        ));
    }
    collection::insert_mutation_receipt_on_connection(
        &mut transaction,
        auth.user_id,
        mutation.mutation_id(),
        mutation.operation(),
        fingerprint,
        &result,
    )
    .await?;
    transaction.commit().await?;

    Ok(result)
}

fn mutation_fingerprint(mutation: &CollectionSyncMutation) -> Result<String, AppError> {
    let payload = serde_json::to_vec(mutation).map_err(|error| {
        AppError::InternalError(anyhow::anyhow!(
            "Failed to fingerprint sync mutation: {error}"
        ))
    })?;
    Ok(hex::encode(Sha256::digest(payload)))
}

fn sync_result_from_error(
    mutation_id: &str,
    error: AppError,
) -> Result<CollectionSyncResult, AppError> {
    let (status, message, code) = match error {
        AppError::BadRequest(message)
        | AppError::PayloadTooLarge(message)
        | AppError::NotFound(message) => (CollectionSyncStatus::Rejected, message, None),
        AppError::BadRequestWithCode { message, code } => {
            (CollectionSyncStatus::Rejected, message, Some(code))
        }
        AppError::Validation { fields } => (
            CollectionSyncStatus::Rejected,
            format!("Validation failed: {fields:?}"),
            None,
        ),
        AppError::Conflict(message) => (CollectionSyncStatus::Conflict, message, None),
        AppError::ConflictWithCode { message, code } => {
            (CollectionSyncStatus::Conflict, message, Some(code))
        }
        AppError::Forbidden { message, code } => (CollectionSyncStatus::Rejected, message, code),
        other => return Err(other),
    };

    Ok(CollectionSyncResult {
        mutation_id: mutation_id.to_string(),
        status,
        entry: None,
        error: Some(message),
        code,
    })
}

fn rejected_sync_result(mutation_id: &str, message: String, code: &str) -> CollectionSyncResult {
    CollectionSyncResult {
        mutation_id: mutation_id.to_string(),
        status: CollectionSyncStatus::Rejected,
        entry: None,
        error: Some(message),
        code: Some(code.to_string()),
    }
}

fn validate_collection_query(params: &CollectionQueryParams) -> Result<(), AppError> {
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
    if params.issue_id == Some(0) {
        return Err(AppError::BadRequest(
            "issue_id must be greater than zero".to_string(),
        ));
    }
    if let Some(ref sort) = params.sort {
        validate_collection_sort(sort).map_err(AppError::BadRequest)?;
    }
    if let Some(ref direction) = params.sort_dir {
        validate_sort_direction(direction).map_err(AppError::BadRequest)?;
    }

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
    }
    Ok(())
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
    if body.copy_number == Some(0) {
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
    let edition_label =
        normalize_edition_label(body.edition_label.as_deref()).map_err(AppError::BadRequest)?;

    let mut transaction = state.inner.pool.begin().await?;
    if matches!(status, "duplicate" | "wanted") {
        crate::db::trade_matching::lock_reconciliation_users_for_issues(
            &mut transaction,
            auth.user_id,
            &[body.issue_id],
        )
        .await?;
    }
    let copy_number = match body.copy_number {
        Some(copy_number) => copy_number,
        None => collection::next_copy_number_on_connection(
            &mut transaction,
            auth.user_id,
            body.issue_id,
        )
        .await?
        .ok_or_else(|| AppError::ConflictWithCode {
            message: "No free copy number is available for this issue".to_string(),
            code: "collection_capacity_exceeded".to_string(),
        })?,
    };
    let entry_id = collection::add_entry_on_connection(
        &mut transaction,
        collection::NewCollectionEntry {
            user_id: auth.user_id,
            issue_id: body.issue_id,
            copy_number,
            condition_grade: body.condition_grade.as_deref(),
            status,
            notes,
            edition_label: edition_label.as_deref(),
        },
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

    let row = collection::find_entry_row_by_id_and_user_on_connection(
        &mut transaction,
        entry_id,
        auth.user_id,
    )
    .await?
    .ok_or_else(|| {
        AppError::InternalError(anyhow::anyhow!("Failed to retrieve newly created entry"))
    })?;

    if matches!(status, "duplicate" | "wanted") {
        crate::db::trade_matching::reconcile_user_matches(&mut transaction, auth.user_id).await?;
    }
    transaction.commit().await?;

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
    if body.condition_grade.is_none()
        && body.status.is_none()
        && body.notes.is_none()
        && body.edition_label.is_none()
    {
        return Err(AppError::BadRequest(
            "At least one field (condition_grade, status, notes, or edition_label) must be provided"
                .to_string(),
        ));
    }

    // A present but empty note explicitly clears the stored value. Omitting the
    let notes_param = body
        .notes
        .as_deref()
        .map(|note| normalize_collection_note(Some(note)))
        .transpose()
        .map_err(AppError::BadRequest)?;
    let edition_label_param = body
        .edition_label
        .as_deref()
        .map(|label| normalize_edition_label(Some(label)))
        .transpose()
        .map_err(AppError::BadRequest)?;

    let mut transaction = state.inner.pool.begin().await?;
    if body.status.is_some() || body.condition_grade.is_some() || body.edition_label.is_some() {
        trade_matching::prepare_entry_mutation_in_transaction(
            &mut transaction,
            auth.user_id,
            entry_id,
        )
        .await?;
    }

    let existing = collection::find_entry_by_id_and_user_on_connection(
        &mut transaction,
        entry_id,
        auth.user_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Collection entry {entry_id} not found")))?;
    if let Some(base_revision) = body.base_revision
        && existing.revision != base_revision
    {
        return Err(AppError::ConflictWithCode {
            message: "The collection entry changed on the server".to_string(),
            code: "collection_revision_conflict".to_string(),
        });
    }

    let final_status = body.status.as_deref().unwrap_or(&existing.status);
    let final_condition = body
        .condition_grade
        .as_deref()
        .or(existing.condition_grade.as_deref());
    validate_status_condition(final_status, final_condition).map_err(AppError::BadRequest)?;

    let photo_storage_keys = if final_status == "wanted" && existing.status != "wanted" {
        crate::db::media::enqueue_entry_photo_deletions(&mut transaction, entry_id, auth.user_id)
            .await?
    } else {
        Vec::new()
    };

    collection::update_entry_on_connection(
        &mut transaction,
        entry_id,
        auth.user_id,
        body.condition_grade.as_deref(),
        body.status.as_deref(),
        notes_param,
        edition_label_param
            .as_ref()
            .map(|edition_label| edition_label.as_deref()),
    )
    .await?;

    let row = collection::find_entry_row_by_id_and_user_on_connection(
        &mut transaction,
        entry_id,
        auth.user_id,
    )
    .await?
    .ok_or_else(|| AppError::InternalError(anyhow::anyhow!("Failed to retrieve updated entry")))?;

    if body.status.is_some()
        || body.edition_label.is_some()
        || (body.condition_grade.is_some()
            && matches!(existing.status.as_str(), "duplicate" | "wanted"))
    {
        crate::db::trade_matching::reconcile_user_matches(&mut transaction, auth.user_id).await?;
    }
    transaction.commit().await?;

    for storage_key in photo_storage_keys {
        if let Err(error) = crate::services::media::process_deletion_key(
            &state.inner.pool,
            &state.inner.media_storage,
            &storage_key,
        )
        .await
        {
            tracing::warn!(entry_id, error = %error, "Collection photo deletion queued for retry");
        }
    }

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
    let mut transaction = state.inner.pool.begin().await?;
    trade_matching::prepare_entry_mutation_in_transaction(&mut transaction, auth.user_id, entry_id)
        .await?;
    let photo_storage_keys =
        crate::db::media::enqueue_entry_photo_deletions(&mut transaction, entry_id, auth.user_id)
            .await?;
    let deleted =
        collection::delete_entry_on_connection(&mut transaction, entry_id, auth.user_id).await?;

    if !deleted {
        return Err(AppError::NotFound(format!(
            "Collection entry {entry_id} not found"
        )));
    }

    crate::db::trade_matching::reconcile_user_matches(&mut transaction, auth.user_id).await?;
    transaction.commit().await?;

    for storage_key in photo_storage_keys {
        if let Err(error) = crate::services::media::process_deletion_key(
            &state.inner.pool,
            &state.inner.media_storage,
            &storage_key,
        )
        .await
        {
            tracing::warn!(entry_id, error = %error, "Collection photo deletion queued for retry");
        }
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
        total_physical_owned: stats.total_physical_owned as u32,
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
        .map(|total| (f64::from(owned.min(total)) / f64::from(total)) * 100.0)
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
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use lilly_importer_core::adapter::AdapterRegistry;
    use sqlx::mysql::MySqlPoolOptions;

    use super::*;
    use crate::models::collection::{
        SeriesStatsEntry, validate_collection_sort, validate_condition_grade,
        validate_sort_direction, validate_status,
    };
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;

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
        assert_eq!(calculate_progress(250, Some(200)), Some(100.0));
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

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn edition_crud_and_copy_allocation_work_against_mariadb() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        crate::db::migrate_test_database(&pool)
            .await
            .expect("test migrations must succeed");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let user_id = inserted_id(
            &sqlx::query("INSERT INTO users (email, display_name) VALUES (?, 'Edition Tester')")
                .bind(format!("collection-editions-{suffix}@example.test"))
                .execute(&pool)
                .await
                .expect("user fixture must be inserted"),
        );
        let series_id = inserted_id(
            &sqlx::query("INSERT INTO series (name, slug, active) VALUES (?, ?, TRUE)")
                .bind(format!("Collection Edition Test {suffix}"))
                .bind(format!("collection-edition-test-{suffix}"))
                .execute(&pool)
                .await
                .expect("series fixture must be inserted"),
        );
        let issue_id = insert_issue(&pool, series_id, 1, "Edition Target").await;
        let other_issue_id = insert_issue(&pool, series_id, 2, "Other Target").await;
        let media_root = std::env::temp_dir().join(format!("lilly-collection-edition-{suffix}"));
        let state = test_state(pool.clone(), media_root.clone());
        let auth = AuthUser {
            user_id,
            display_name: "Edition Tester".to_string(),
            role: "user".to_string(),
        };

        let (_, Json(first)) = add_to_collection(
            State(state.clone()),
            auth.clone(),
            Json(AddCollectionEntryRequest {
                issue_id,
                condition_grade: Some("Z1".to_string()),
                status: Some("owned".to_string()),
                notes: None,
                copy_number: None,
                edition_label: Some("  1. Auflage  ".to_string()),
            }),
        )
        .await
        .expect("first edition must be added");
        assert_eq!(first.copy_number, Some(1));
        assert_eq!(first.edition_label.as_deref(), Some("1. Auflage"));

        let (_, Json(second)) = add_to_collection(
            State(state.clone()),
            auth.clone(),
            Json(AddCollectionEntryRequest {
                issue_id,
                condition_grade: Some("Z2".to_string()),
                status: Some("duplicate".to_string()),
                notes: None,
                copy_number: None,
                edition_label: Some("Variantcover".to_string()),
            }),
        )
        .await
        .expect("second edition must be added");
        assert_eq!(second.copy_number, Some(2));

        let duplicate_copy = add_to_collection(
            State(state.clone()),
            auth.clone(),
            Json(AddCollectionEntryRequest {
                issue_id,
                condition_grade: Some("Z3".to_string()),
                status: Some("owned".to_string()),
                notes: None,
                copy_number: Some(2),
                edition_label: None,
            }),
        )
        .await;
        assert!(matches!(duplicate_copy, Err(AppError::BadRequest(_))));

        delete_entry(State(state.clone()), auth.clone(), Path(first.id))
            .await
            .expect("first edition must be deleted");
        let (_, Json(reallocated)) = add_to_collection(
            State(state.clone()),
            auth.clone(),
            Json(AddCollectionEntryRequest {
                issue_id,
                condition_grade: Some("Z0".to_string()),
                status: Some("owned".to_string()),
                notes: None,
                copy_number: None,
                edition_label: Some("Neuauflage".to_string()),
            }),
        )
        .await
        .expect("copy number hole must be reused");
        assert_eq!(reallocated.copy_number, Some(1));

        let Json(cleared) = update_entry(
            State(state.clone()),
            auth.clone(),
            Path(second.id),
            Json(UpdateCollectionEntryRequest {
                condition_grade: None,
                status: None,
                notes: None,
                edition_label: Some("   ".to_string()),
                base_revision: None,
            }),
        )
        .await
        .expect("edition label must be clearable");
        assert!(cleared.edition_label.is_none());

        let _other = add_to_collection(
            State(state.clone()),
            auth.clone(),
            Json(AddCollectionEntryRequest {
                issue_id: other_issue_id,
                condition_grade: Some("Z2".to_string()),
                status: Some("owned".to_string()),
                notes: None,
                copy_number: None,
                edition_label: None,
            }),
        )
        .await
        .expect("other issue fixture must be added");
        let Json(filtered) = list_collection(
            State(state.clone()),
            auth.clone(),
            Query(CollectionQueryParams {
                issue_id: Some(issue_id),
                ..CollectionQueryParams::default()
            }),
        )
        .await
        .expect("issue copies must be filtered");
        assert_eq!(filtered.total, 2);
        assert!(filtered.data.iter().all(|entry| entry.issue_id == issue_id));

        let overlong = add_to_collection(
            State(state.clone()),
            auth.clone(),
            Json(AddCollectionEntryRequest {
                issue_id,
                condition_grade: Some("Z2".to_string()),
                status: Some("owned".to_string()),
                notes: None,
                copy_number: None,
                edition_label: Some(
                    "📚".repeat(crate::models::collection::MAX_EDITION_LABEL_LENGTH + 1),
                ),
            }),
        )
        .await;
        assert!(matches!(overlong, Err(AppError::BadRequest(_))));

        let sync_issue_id = insert_issue(&pool, series_id, 3, "Offline Sync Target").await;
        let create_mutation = CollectionSyncMutation::Create {
            mutation_id: "18b89e92-36d8-4e12-a5c2-6f79a78fb929".to_string(),
            entry: AddCollectionEntryRequest {
                issue_id: sync_issue_id,
                condition_grade: Some("Z2".to_string()),
                status: Some("owned".to_string()),
                notes: Some("offline angelegt".to_string()),
                copy_number: None,
                edition_label: None,
            },
        };
        let Json(first_sync) = sync_collection(
            State(state.clone()),
            auth.clone(),
            Json(CollectionSyncRequest {
                mutations: vec![create_mutation.clone()],
            }),
        )
        .await
        .expect("offline create must sync");
        assert_eq!(first_sync.results[0].status, CollectionSyncStatus::Applied);
        let synced_entry = first_sync.results[0]
            .entry
            .clone()
            .expect("applied mutation must return the entry");
        assert_eq!(synced_entry.revision, Some(1));

        let Json(retried_sync) = sync_collection(
            State(state.clone()),
            auth.clone(),
            Json(CollectionSyncRequest {
                mutations: vec![create_mutation],
            }),
        )
        .await
        .expect("identical retry must succeed");
        assert_eq!(
            retried_sync.results[0].status,
            CollectionSyncStatus::AlreadyApplied
        );
        let sync_copy_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM collection_entries WHERE user_id = ? AND issue_id = ?",
        )
        .bind(user_id)
        .bind(sync_issue_id)
        .fetch_one(&pool)
        .await
        .expect("synced entry count must be readable");
        assert_eq!(sync_copy_count, 1);

        let Json(reused_id) = sync_collection(
            State(state.clone()),
            auth.clone(),
            Json(CollectionSyncRequest {
                mutations: vec![CollectionSyncMutation::Create {
                    mutation_id: "18b89e92-36d8-4e12-a5c2-6f79a78fb929".to_string(),
                    entry: AddCollectionEntryRequest {
                        issue_id: sync_issue_id,
                        condition_grade: Some("Z4".to_string()),
                        status: Some("owned".to_string()),
                        notes: None,
                        copy_number: None,
                        edition_label: None,
                    },
                }],
            }),
        )
        .await
        .expect("reused ID must produce an item result");
        assert_eq!(reused_id.results[0].status, CollectionSyncStatus::Rejected);
        assert_eq!(
            reused_id.results[0].code.as_deref(),
            Some("mutation_id_reused")
        );

        let Json(conflict) = sync_collection(
            State(state.clone()),
            auth.clone(),
            Json(CollectionSyncRequest {
                mutations: vec![CollectionSyncMutation::Update {
                    mutation_id: "c3fccac0-0ed0-4df5-a743-ab2872790eb5".to_string(),
                    entry_id: synced_entry.id,
                    base_revision: 0,
                    changes: UpdateCollectionEntryRequest {
                        condition_grade: Some("Z1".to_string()),
                        status: None,
                        notes: None,
                        edition_label: None,
                        base_revision: None,
                    },
                }],
            }),
        )
        .await
        .expect("stale update must produce a conflict result");
        assert_eq!(conflict.results[0].status, CollectionSyncStatus::Conflict);
        assert_eq!(
            conflict.results[0]
                .entry
                .as_ref()
                .and_then(|entry| entry.revision),
            Some(1)
        );

        let Json(applied_update) = sync_collection(
            State(state.clone()),
            auth.clone(),
            Json(CollectionSyncRequest {
                mutations: vec![CollectionSyncMutation::Update {
                    mutation_id: "cd591217-c507-4cc3-b876-2864cf9c7526".to_string(),
                    entry_id: synced_entry.id,
                    base_revision: 1,
                    changes: UpdateCollectionEntryRequest {
                        condition_grade: Some("Z1".to_string()),
                        status: None,
                        notes: None,
                        edition_label: None,
                        base_revision: None,
                    },
                }],
            }),
        )
        .await
        .expect("current update must be applied");
        assert_eq!(
            applied_update.results[0]
                .entry
                .as_ref()
                .and_then(|entry| entry.revision),
            Some(2)
        );

        let Json(snapshot) = offline_snapshot(State(state), auth)
            .await
            .expect("offline snapshot must load");
        assert_eq!(snapshot.user_id, user_id);
        assert_eq!(snapshot.schema_version, 1);
        assert!(
            snapshot
                .collection_entries
                .iter()
                .any(|entry| entry.id == synced_entry.id && entry.revision == Some(2))
        );

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("user fixture must be deleted");
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series fixture must be deleted");
        let _ = tokio::fs::remove_dir_all(media_root).await;
    }

    fn test_state(pool: sqlx::MySqlPool, media_root: PathBuf) -> AppState {
        let media_storage = crate::services::media::MediaStorage::new(&media_root);
        AppState {
            inner: Arc::new(AppStateInner {
                pool,
                jwt_secret: "collection-test-secret".to_string(),
                jwt_access_expiry: 900,
                jwt_refresh_expiry: 2_592_000,
                password_reset_ttl_seconds: 3_600,
                email_service: EmailService::Log {
                    from: "test@lilly.app".to_string(),
                },
                app_base_url: "http://localhost".to_string(),
                cookie_secure: false,
                oauth_service: crate::services::oauth::OAuthService::disabled(),
                privacy_policy_version: "test-v1".to_string(),
                adapter_registry: AdapterRegistry::new(),
                media_path: media_root,
                media_url_prefix: "/media".to_string(),
                photo_upload_config: crate::config::PhotoUploadConfig::default(),
                media_storage,
                import_scheduler_config: ImportSchedulerConfig {
                    enabled: false,
                    schedule: "0 10 6 * * Sat *".to_string(),
                    timezone: "Europe/Berlin".to_string(),
                    adapters: Vec::new(),
                },
                request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
            }),
        }
    }

    async fn insert_issue(
        pool: &sqlx::MySqlPool,
        series_id: u32,
        issue_number: u32,
        title: &str,
    ) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)")
                .bind(series_id)
                .bind(issue_number)
                .bind(title)
                .execute(pool)
                .await
                .expect("issue fixture must be inserted"),
        )
    }

    fn inserted_id(result: &sqlx::mysql::MySqlQueryResult) -> u32 {
        result
            .last_insert_id()
            .try_into()
            .expect("fixture ID must fit into u32")
    }
}
