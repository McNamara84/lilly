use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::{AuthUser, OptionalAuthUser};
use crate::db::{collection, profiles};
use crate::error::AppError;
use crate::models::collection::{CollectionQueryParams, CollectionStatsResponse, SeriesStatsEntry};
use crate::models::profile::{
    OwnProfileResponse, PaginatedPublicCollectionResponse, PublicCollectionEntryResponse,
    PublicProfileResponse, UpdateProfileRequest, UpdateVisibilityRequest, VisibilityResponse,
};
use crate::services::media::{StagedPhoto, process_deletion_key, process_photo};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/me/profile",
            get(get_own_profile).patch(update_own_profile),
        )
        .route(
            "/api/v1/me/profile/visibility",
            patch(update_own_visibility),
        )
        .route(
            "/api/v1/me/profile/avatar",
            post(upload_avatar)
                .delete(delete_avatar)
                .layer(DefaultBodyLimit::max(
                    super::media::HARD_MULTIPART_REQUEST_LIMIT,
                )),
        )
        .route("/api/v1/users/{user_id}/profile", get(get_public_profile))
        .route("/api/v1/users/{user_id}/avatar", get(avatar_content))
        .route(
            "/api/v1/users/{user_id}/collection",
            get(get_public_collection),
        )
        .route(
            "/api/v1/users/{user_id}/collection/stats",
            get(get_public_collection_stats),
        )
}

async fn update_own_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<OwnProfileResponse>, AppError> {
    let normalized = body
        .normalize()
        .map_err(|fields| AppError::Validation { fields })?;
    profiles::update_profile(
        &state.inner.pool,
        auth.user_id,
        &normalized.display_name,
        normalized.location.as_deref(),
    )
    .await?;
    // Fetching the row after the update distinguishes a missing account while
    // keeping an idempotent PATCH successful when MySQL reports zero changes.
    get_own_profile(State(state), auth).await
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

async fn upload_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<OwnProfileResponse>, AppError> {
    let bytes = super::media::read_single_photo(
        &mut multipart,
        state.inner.photo_upload_config.max_upload_bytes,
    )
    .await?;
    let config = state.inner.photo_upload_config.clone();
    let processed = tokio::task::spawn_blocking(move || process_photo(&bytes, &config))
        .await
        .map_err(|error| {
            AppError::InternalError(anyhow::anyhow!("Avatar processing task failed: {error}"))
        })?
        .map_err(super::media::map_media_error)?;
    let staged = state
        .inner
        .media_storage
        .stage(&processed.bytes)
        .await
        .map_err(super::media::map_media_error)?;

    if let Err(error) = persist_avatar(&state, auth.user_id, &staged).await {
        state.inner.media_storage.discard_staged(&staged).await;
        return Err(error);
    }
    get_own_profile(State(state), auth).await
}

async fn persist_avatar(
    state: &AppState,
    user_id: u32,
    staged: &StagedPhoto,
) -> Result<(), AppError> {
    let mut transaction = state.inner.pool.begin().await?;
    let old_storage_key =
        profiles::replace_avatar(&mut transaction, user_id, Some(staged.storage_key.as_str()))
            .await?
            .ok_or_else(private_resource_not_found)?;
    state
        .inner
        .media_storage
        .commit(staged)
        .await
        .map_err(super::media::map_media_error)?;
    transaction.commit().await?;

    if let Some(old_storage_key) = old_storage_key
        && let Err(error) = process_deletion_key(
            &state.inner.pool,
            &state.inner.media_storage,
            &old_storage_key,
        )
        .await
    {
        tracing::warn!(user_id, error = %error, "Avatar deletion queued for retry");
    }
    Ok(())
}

async fn delete_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {
    let mut transaction = state.inner.pool.begin().await?;
    let old_storage_key = profiles::replace_avatar(&mut transaction, auth.user_id, None)
        .await?
        .ok_or_else(private_resource_not_found)?;
    transaction.commit().await?;

    if let Some(old_storage_key) = old_storage_key
        && let Err(error) = process_deletion_key(
            &state.inner.pool,
            &state.inner.media_storage,
            &old_storage_key,
        )
        .await
    {
        tracing::warn!(user_id = auth.user_id, error = %error, "Avatar deletion queued for retry");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn avatar_content(
    State(state): State<AppState>,
    OptionalAuthUser(auth): OptionalAuthUser,
    Path(user_id): Path<u32>,
) -> Result<Response, AppError> {
    let avatar = profiles::find_avatar(&state.inner.pool, user_id)
        .await?
        .ok_or_else(private_resource_not_found)?;
    let is_owner = auth.is_some_and(|user| user.user_id == avatar.user_id);
    if !is_owner && !avatar.profile_public {
        return Err(private_resource_not_found());
    }
    let bytes = state
        .inner
        .media_storage
        .read(&avatar.storage_key)
        .await
        .map_err(|error| {
            tracing::error!(user_id, error = %error, "Stored avatar could not be read");
            AppError::InternalError(anyhow::anyhow!("Stored avatar is unavailable"))
        })?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::CACHE_CONTROL, "private, no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(bytes))
        .map_err(|error| AppError::InternalError(error.into()))
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
    ensure_public_profile_statistics(&state, user_id).await?;
    Ok(Json(build_collection_stats(&state, user_id).await?))
}

async fn ensure_public_profile_statistics(state: &AppState, user_id: u32) -> Result<(), AppError> {
    if profiles::is_profile_and_collection_public(&state.inner.pool, user_id).await? {
        Ok(())
    } else {
        Err(private_resource_not_found())
    }
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
        total_physical_owned: stats.total_physical_owned as u32,
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
    use std::io::Cursor;
    use std::path::{Path as FsPath, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use axum::response::Response;
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    use lilly_importer_core::AdapterRegistry;
    use serde_json::Value;
    use sqlx::MySqlPool;
    use sqlx::mysql::{MySqlPoolOptions, MySqlQueryResult};
    use tower::ServiceExt;

    use super::*;
    use crate::auth::jwt;
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;
    use crate::services::media::{MediaStorage, reconcile_storage};

    const TEST_JWT_SECRET: &str = "profile-route-test-secret-with-safe-length";

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

    #[test]
    fn progress_is_bounded_and_absent_without_a_total() {
        assert_eq!(calculate_progress(25, Some(20)), Some(100.0));
        assert_eq!(calculate_progress(0, Some(0)), None);
        assert_eq!(calculate_progress(1, None), None);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn profile_avatar_and_public_statistics_work_end_to_end() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(10)
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
        let media_root = std::env::temp_dir().join(format!("lilly-profile-route-{suffix}"));
        let user_id = insert_user(&pool, &format!("profile-{suffix}@example.test")).await;
        let other_user_id =
            insert_user(&pool, &format!("profile-other-{suffix}@example.test")).await;
        let series_id = insert_series(&pool, &format!("profile-series-{suffix}")).await;
        let first_issue_id = insert_issue(&pool, series_id, 1).await;
        let second_issue_id = insert_issue(&pool, series_id, 2).await;
        insert_entry(&pool, user_id, first_issue_id, 1, "owned").await;
        insert_entry(&pool, user_id, first_issue_id, 2, "duplicate").await;
        insert_entry(&pool, user_id, second_issue_id, 1, "wanted").await;
        let app = test_router(pool.clone(), &media_root);

        let update = app
            .clone()
            .oneshot(json_request(
                Method::PATCH,
                "/api/v1/me/profile",
                r#"{"display_name":"  Sammlerin 📚  ","location":"  Berlin  "}"#,
                user_id,
            ))
            .await
            .unwrap();
        assert_eq!(update.status(), StatusCode::OK);
        let updated_profile = response_json(update).await;
        assert_eq!(updated_profile["display_name"], "Sammlerin 📚");
        assert_eq!(updated_profile["location"], "Berlin");
        assert!(updated_profile.get("avatar_path").is_none());
        assert!(updated_profile["avatar_url"].is_null());

        let idempotent_update = app
            .clone()
            .oneshot(json_request(
                Method::PATCH,
                "/api/v1/me/profile",
                r#"{"display_name":"Sammlerin 📚","location":"Berlin"}"#,
                user_id,
            ))
            .await
            .unwrap();
        assert_eq!(idempotent_update.status(), StatusCode::OK);

        let invalid = app
            .clone()
            .oneshot(json_request(
                Method::PATCH,
                "/api/v1/me/profile",
                r#"{"display_name":" X ","location":null}"#,
                user_id,
            ))
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert!(response_json(invalid).await["fields"]["display_name"].is_string());

        let unauthenticated_update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PATCH)
                    .uri("/api/v1/me/profile")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"display_name":"Nope","location":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated_update.status(), StatusCode::UNAUTHORIZED);

        let first_upload = app
            .clone()
            .oneshot(multipart_avatar_request(
                &encoded_png([20, 40, 60, 255]),
                user_id,
            ))
            .await
            .unwrap();
        assert_eq!(first_upload.status(), StatusCode::OK);
        let upload_profile = response_json(first_upload).await;
        assert_eq!(
            upload_profile["avatar_url"],
            format!("/api/v1/users/{user_id}/avatar")
        );
        assert!(upload_profile.get("avatar_path").is_none());
        let first_key = avatar_storage_key(&pool, user_id).await.unwrap();
        assert!(!first_key.starts_with('/'));
        assert!(media_root.join("user-photos").join(&first_key).exists());

        let private_avatar = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/users/{user_id}/avatar"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(private_avatar.status(), StatusCode::NOT_FOUND);
        let owner_avatar = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/users/{user_id}/avatar"),
                Some(user_id),
            ))
            .await
            .unwrap();
        assert_eq!(owner_avatar.status(), StatusCode::OK);
        assert_eq!(owner_avatar.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(
            owner_avatar.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(owner_avatar.headers()["x-content-type-options"], "nosniff");

        let second_upload = app
            .clone()
            .oneshot(multipart_avatar_request(
                &encoded_png([80, 100, 120, 255]),
                user_id,
            ))
            .await
            .unwrap();
        assert_eq!(second_upload.status(), StatusCode::OK);
        let second_key = avatar_storage_key(&pool, user_id).await.unwrap();
        assert_ne!(first_key, second_key);
        assert!(!media_root.join("user-photos").join(&first_key).exists());
        assert!(media_root.join("user-photos").join(&second_key).exists());
        let reconciliation = reconcile_storage(&pool, &MediaStorage::new(&media_root))
            .await
            .unwrap();
        assert_eq!(reconciliation.removed_orphans, 0);

        set_visibility(&pool, user_id, false, true).await;
        let collection_without_profile = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/users/{user_id}/collection"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(collection_without_profile.status(), StatusCode::OK);
        let hidden_profile_stats = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/users/{user_id}/collection/stats"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(hidden_profile_stats.status(), StatusCode::NOT_FOUND);

        set_visibility(&pool, user_id, true, true).await;
        let public_profile = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/users/{user_id}/profile"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(public_profile.status(), StatusCode::OK);
        let public_profile = response_json(public_profile).await;
        assert_eq!(
            public_profile["avatar_url"],
            format!("/api/v1/users/{user_id}/avatar")
        );
        for private_field in ["email", "role", "avatar_path"] {
            assert!(public_profile.get(private_field).is_none());
        }
        let public_avatar = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/users/{user_id}/avatar"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(public_avatar.status(), StatusCode::OK);

        let public_stats = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/users/{user_id}/collection/stats"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(public_stats.status(), StatusCode::OK);
        let public_stats = response_json(public_stats).await;
        assert_eq!(public_stats["total_physical_owned"], 2);
        assert_eq!(public_stats["total_owned"], 1);
        assert_eq!(public_stats["total_duplicate"], 1);
        assert_eq!(public_stats["total_wanted"], 1);
        assert_eq!(public_stats["series_stats"][0]["owned_count"], 1);
        assert_eq!(public_stats["series_stats"][0]["progress_percent"], 10.0);

        let foreign_delete = app
            .clone()
            .oneshot(basic_request(
                Method::DELETE,
                "/api/v1/me/profile/avatar",
                Some(other_user_id),
            ))
            .await
            .unwrap();
        assert_eq!(foreign_delete.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            avatar_storage_key(&pool, user_id).await.as_deref(),
            Some(second_key.as_str())
        );

        let delete = app
            .clone()
            .oneshot(basic_request(
                Method::DELETE,
                "/api/v1/me/profile/avatar",
                Some(user_id),
            ))
            .await
            .unwrap();
        assert_eq!(delete.status(), StatusCode::NO_CONTENT);
        assert!(avatar_storage_key(&pool, user_id).await.is_none());
        assert!(!media_root.join("user-photos").join(&second_key).exists());
        let repeated_delete = app
            .clone()
            .oneshot(basic_request(
                Method::DELETE,
                "/api/v1/me/profile/avatar",
                Some(user_id),
            ))
            .await
            .unwrap();
        assert_eq!(repeated_delete.status(), StatusCode::NO_CONTENT);

        sqlx::query("DELETE FROM users WHERE id IN (?, ?)")
            .bind(user_id)
            .bind(other_user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(media_root).await;
    }

    fn test_router(pool: MySqlPool, media_root: &FsPath) -> Router {
        let storage = MediaStorage::new(media_root);
        Router::new().merge(router()).with_state(AppState {
            inner: Arc::new(AppStateInner {
                pool,
                jwt_secret: TEST_JWT_SECRET.to_string(),
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
                media_path: PathBuf::from(media_root),
                media_url_prefix: "/media".to_string(),
                photo_upload_config: crate::config::PhotoUploadConfig::default(),
                media_storage: storage,
                import_scheduler_config: ImportSchedulerConfig {
                    enabled: false,
                    schedule: "0 10 6 * * Sat *".to_string(),
                    timezone: "Europe/Berlin".to_string(),
                    adapters: Vec::new(),
                },
                request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
            }),
        })
    }

    fn basic_request(method: Method, uri: &str, user_id: Option<u32>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(user_id) = user_id {
            builder = builder.header(header::COOKIE, auth_cookie(user_id));
        }
        builder.body(Body::empty()).unwrap()
    }

    fn json_request(method: Method, uri: &str, body: &str, user_id: u32) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, auth_cookie(user_id))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn multipart_avatar_request(bytes: &[u8], user_id: u32) -> Request<Body> {
        const BOUNDARY: &str = "lilly-avatar-boundary";
        let mut body = format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"avatar.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/me/profile/avatar")
            .header(header::COOKIE, auth_cookie(user_id))
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(body))
            .unwrap()
    }

    fn auth_cookie(user_id: u32) -> String {
        let token =
            jwt::create_token(user_id, "Profile Tester", "user", TEST_JWT_SECRET, 3_600).unwrap();
        format!("access_token={token}")
    }

    async fn response_json(response: Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn encoded_png(rgba: [u8; 4]) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(8, 8, Rgba(rgba)));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    async fn insert_user(pool: &MySqlPool, email: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO users (email, display_name) VALUES (?, 'Profile Tester')")
                .bind(email)
                .execute(pool)
                .await
                .unwrap(),
        )
    }

    async fn insert_series(pool: &MySqlPool, slug: &str) -> u32 {
        inserted_id(
            &sqlx::query(
                "INSERT INTO series (name, slug, total_issues, active) VALUES (?, ?, 10, TRUE)",
            )
            .bind(format!("Profile Test {slug}"))
            .bind(slug)
            .execute(pool)
            .await
            .unwrap(),
        )
    }

    async fn insert_issue(pool: &MySqlPool, series_id: u32, issue_number: u32) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)")
                .bind(series_id)
                .bind(issue_number)
                .bind(format!("Profile Issue {issue_number}"))
                .execute(pool)
                .await
                .unwrap(),
        )
    }

    async fn insert_entry(
        pool: &MySqlPool,
        user_id: u32,
        issue_id: u32,
        copy_number: u8,
        status: &str,
    ) {
        sqlx::query(
            "INSERT INTO collection_entries \
             (user_id, issue_id, copy_number, condition_grade, status) \
             VALUES (?, ?, ?, 'Z1', ?)",
        )
        .bind(user_id)
        .bind(issue_id)
        .bind(copy_number)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_visibility(
        pool: &MySqlPool,
        user_id: u32,
        profile_public: bool,
        collection_public: bool,
    ) {
        sqlx::query("UPDATE users SET profile_public = ?, collection_public = ? WHERE id = ?")
            .bind(profile_public)
            .bind(collection_public)
            .bind(user_id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn avatar_storage_key(pool: &MySqlPool, user_id: u32) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>("SELECT avatar_path FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[allow(clippy::cast_possible_truncation)]
    fn inserted_id(result: &MySqlQueryResult) -> u32 {
        result.last_insert_id() as u32
    }
}
