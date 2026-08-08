use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::{collection, profiles};
use crate::error::AppError;
use crate::models::collection::{CollectionQueryParams, CollectionStatsResponse, SeriesStatsEntry};
use crate::models::profile::{
    OwnProfileResponse, PaginatedPublicCollectionResponse, PublicCollectionEntryResponse,
    PublicProfileResponse, UpdateVisibilityRequest, VisibilityResponse,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/profile", get(get_own_profile))
        .route(
            "/api/v1/me/profile/visibility",
            patch(update_own_visibility),
        )
        .route("/api/v1/users/{user_id}/profile", get(get_public_profile))
        .route(
            "/api/v1/users/{user_id}/collection",
            get(get_public_collection),
        )
        .route(
            "/api/v1/users/{user_id}/collection/stats",
            get(get_public_collection_stats),
        )
}

async fn get_own_profile(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<OwnProfileResponse>, AppError> {
    let profile = profiles::find_own_profile(&state.inner.pool, auth.user_id)
        .await?
        .ok_or_else(private_resource_not_found)?;
    Ok(Json(profile.into()))
}

async fn update_own_visibility(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateVisibilityRequest>,
) -> Result<Json<VisibilityResponse>, AppError> {
    let updated = profiles::update_visibility(
        &state.inner.pool,
        auth.user_id,
        body.profile_public,
        body.collection_public,
    )
    .await?;

    if !updated {
        return Err(private_resource_not_found());
    }

    Ok(Json(VisibilityResponse {
        profile_public: body.profile_public,
        collection_public: body.collection_public,
    }))
}

async fn get_public_profile(
    State(state): State<AppState>,
    Path(user_id): Path<u32>,
) -> Result<Json<PublicProfileResponse>, AppError> {
    let profile = profiles::find_public_profile(&state.inner.pool, user_id)
        .await?
        .ok_or_else(private_resource_not_found)?;
    Ok(Json(profile.into()))
}

async fn get_public_collection(
    State(state): State<AppState>,
    Path(user_id): Path<u32>,
    Query(mut params): Query<CollectionQueryParams>,
) -> Result<Json<PaginatedPublicCollectionResponse>, AppError> {
    ensure_public_collection(&state, user_id).await?;

    // Public collection browsing initially supports pagination only. Ignore no
    // filters silently: reject them so the public contract cannot accidentally
    // expose virtual missing entries or unsupported query behavior.
    if params.series_slug.is_some()
        || params.status.is_some()
        || params.issue_number.is_some()
        || params.condition.is_some()
        || params.condition_min.is_some()
        || params.condition_max.is_some()
        || params.title.is_some()
        || params.author.is_some()
        || params.sort.is_some()
        || params.sort_dir.is_some()
        || params.q.is_some()
    {
        return Err(AppError::BadRequest(
            "Public collection currently supports page and per_page only".to_string(),
        ));
    }

    params.page = params.page.max(1);
    params.per_page = params.per_page.clamp(1, 100);

    let total = collection::count_collection_entries(&state.inner.pool, user_id, &params).await?;
    let entries = collection::find_collection_entries(&state.inner.pool, user_id, &params).await?;
    let data = entries
        .iter()
        .map(PublicCollectionEntryResponse::from)
        .collect();

    Ok(Json(PaginatedPublicCollectionResponse {
        data,
        page: params.page,
        per_page: params.per_page,
        total,
    }))
}

async fn get_public_collection_stats(
    State(state): State<AppState>,
    Path(user_id): Path<u32>,
) -> Result<Json<CollectionStatsResponse>, AppError> {
    ensure_public_collection(&state, user_id).await?;
    Ok(Json(build_collection_stats(&state, user_id).await?))
}

async fn ensure_public_collection(state: &AppState, user_id: u32) -> Result<(), AppError> {
    if profiles::is_collection_public(&state.inner.pool, user_id).await? {
        Ok(())
    } else {
        Err(private_resource_not_found())
    }
}

#[allow(clippy::similar_names)]
async fn build_collection_stats(
    state: &AppState,
    user_id: u32,
) -> Result<CollectionStatsResponse, AppError> {
    let stats = collection::get_collection_stats(&state.inner.pool, user_id).await?;
    let series = collection::get_series_stats(&state.inner.pool, user_id).await?;

    let series_stats = series
        .iter()
        .map(|row| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let imported = row.imported_total as u32;
            let total = match row.declared_total {
                Some(declared) => Some(declared.max(imported)),
                None if imported > 0 => Some(imported),
                None => None,
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let owned = row.owned_count as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let duplicate = row.duplicate_count as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let wanted = row.wanted_count as u32;

            SeriesStatsEntry {
                series_id: row.series_id,
                series_name: row.series_name.clone(),
                series_slug: row.series_slug.clone(),
                total_in_series: total,
                owned_count: owned,
                duplicate_count: duplicate,
                wanted_count: wanted,
                progress_percent: calculate_progress(owned, total),
            }
        })
        .collect::<Vec<_>>();

    let (total_issues, overall_progress_percent) = calculate_overall_stats(&series_stats);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(CollectionStatsResponse {
        total_issues,
        total_owned: stats.total_owned as u32,
        total_duplicate: stats.total_duplicate as u32,
        total_wanted: stats.total_wanted as u32,
        overall_progress_percent,
        series_stats,
    })
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

    let total = series_stats.iter().fold(0u32, |sum, series| {
        sum.saturating_add(series.total_in_series.unwrap_or_default())
    });
    let owned = series_stats
        .iter()
        .fold(0u32, |sum, series| sum.saturating_add(series.owned_count));
    (Some(total), calculate_progress(owned, Some(total)))
}

fn private_resource_not_found() -> AppError {
    AppError::NotFound("Resource not found".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_resource_error_is_generic() {
        let response = axum::response::IntoResponse::into_response(private_resource_not_found());
        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn overall_stats_require_known_totals() {
        let unknown = SeriesStatsEntry {
            series_id: 1,
            series_name: "Series".to_string(),
            series_slug: "series".to_string(),
            total_in_series: None,
            owned_count: 1,
            duplicate_count: 0,
            wanted_count: 0,
            progress_percent: None,
        };
        assert_eq!(calculate_overall_stats(&[unknown]), (None, None));
    }

    #[test]
    fn overall_stats_calculate_known_series() {
        let known = SeriesStatsEntry {
            series_id: 1,
            series_name: "Series".to_string(),
            series_slug: "series".to_string(),
            total_in_series: Some(20),
            owned_count: 5,
            duplicate_count: 1,
            wanted_count: 2,
            progress_percent: Some(25.0),
        };
        assert_eq!(calculate_overall_stats(&[known]), (Some(20), Some(25.0)));
    }
}
