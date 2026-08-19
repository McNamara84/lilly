use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::{AuthUser, OptionalAuthUser};
use crate::db::media as media_db;
use crate::error::AppError;
use crate::models::media::{CollectionPhotoResponse, PhotoPolicyResponse, ProcessedPhoto};
use crate::services::media::{MediaError, StagedPhoto, process_deletion_key, process_photo};

pub(super) const HARD_MULTIPART_REQUEST_LIMIT: usize = 5 * 1024 * 1024 + 64 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/collection/{entry_id}/photos", get(list_photos))
        .route(
            "/api/v1/me/collection/{entry_id}/photos",
            post(upload_photo).layer(DefaultBodyLimit::max(HARD_MULTIPART_REQUEST_LIMIT)),
        )
        .route(
            "/api/v1/me/collection/{entry_id}/photos/{photo_id}",
            delete(delete_photo),
        )
        .route(
            "/api/v1/collection-photos/{photo_id}/content",
            get(photo_content),
        )
        .route("/api/v1/media/photo-policy", get(photo_policy))
}

async fn photo_policy(State(state): State<AppState>) -> Json<PhotoPolicyResponse> {
    let config = &state.inner.photo_upload_config;
    Json(PhotoPolicyResponse {
        allowed_media_types: ["image/jpeg", "image/png", "image/webp"],
        max_upload_bytes: config.max_upload_bytes,
        max_photos: config.max_count,
        max_edge: config.max_edge,
    })
}

async fn list_photos(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(entry_id): Path<u32>,
) -> Result<Json<Vec<CollectionPhotoResponse>>, AppError> {
    let photos = media_db::list_entry_photos_for_owner(&state.inner.pool, entry_id, auth.user_id)
        .await?
        .ok_or_else(photo_not_found)?;
    Ok(Json(
        photos.iter().map(CollectionPhotoResponse::from).collect(),
    ))
}

async fn upload_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(entry_id): Path<u32>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<CollectionPhotoResponse>), AppError> {
    match media_db::photo_upload_preflight(
        &state.inner.pool,
        entry_id,
        auth.user_id,
        state.inner.photo_upload_config.max_count,
    )
    .await?
    {
        media_db::PhotoUploadPreflight::Ready => {}
        media_db::PhotoUploadPreflight::NotFound => return Err(photo_not_found()),
        media_db::PhotoUploadPreflight::Full => return Err(photo_limit_reached()),
    }
    let bytes = read_single_photo(
        &mut multipart,
        state.inner.photo_upload_config.max_upload_bytes,
    )
    .await?;
    let config = state.inner.photo_upload_config.clone();
    let processed = tokio::task::spawn_blocking(move || process_photo(&bytes, &config))
        .await
        .map_err(|error| {
            AppError::InternalError(anyhow::anyhow!("Photo processing task failed: {error}"))
        })?
        .map_err(map_media_error)?;
    let staged = state
        .inner
        .media_storage
        .stage(&processed.bytes)
        .await
        .map_err(map_media_error)?;

    let result = persist_photo(&state, auth.user_id, entry_id, &processed, &staged).await;
    match result {
        Ok(photo) => Ok((
            StatusCode::CREATED,
            Json(CollectionPhotoResponse::from(&photo)),
        )),
        Err(error) => {
            state.inner.media_storage.discard_staged(&staged).await;
            Err(error)
        }
    }
}

async fn persist_photo(
    state: &AppState,
    user_id: u32,
    entry_id: u32,
    processed: &ProcessedPhoto,
    staged: &StagedPhoto,
) -> Result<crate::models::media::CollectionPhotoRow, AppError> {
    let mut transaction = state.inner.pool.begin().await?;
    if !media_db::lock_uploadable_entry(&mut transaction, entry_id, user_id).await? {
        return Err(photo_not_found());
    }
    let slot = media_db::first_free_slot(
        &mut transaction,
        entry_id,
        state.inner.photo_upload_config.max_count,
    )
    .await?
    .ok_or_else(photo_limit_reached)?;
    let byte_size = u32::try_from(processed.bytes.len()).map_err(|_| {
        AppError::PayloadTooLarge("Processed photo is too large to store".to_string())
    })?;
    let photo_id = media_db::insert_photo(
        &mut transaction,
        entry_id,
        &staged.storage_key,
        processed.media_type,
        byte_size,
        processed.width,
        processed.height,
        slot,
    )
    .await
    .map_err(|error| {
        if let sqlx::Error::Database(ref database_error) = error
            && database_error.kind() == sqlx::error::ErrorKind::UniqueViolation
        {
            return AppError::Conflict(
                "A concurrent upload filled the remaining photo slot".to_string(),
            );
        }
        AppError::from(error)
    })?;
    state
        .inner
        .media_storage
        .commit(staged)
        .await
        .map_err(map_media_error)?;
    transaction.commit().await?;

    media_db::find_photo(&state.inner.pool, photo_id)
        .await?
        .ok_or_else(|| {
            AppError::InternalError(anyhow::anyhow!(
                "Newly persisted photo could not be retrieved"
            ))
        })
}

async fn delete_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((entry_id, photo_id)): Path<(u32, u32)>,
) -> Result<StatusCode, AppError> {
    let mut transaction = state.inner.pool.begin().await?;
    let storage_key = media_db::enqueue_and_delete_owned_photo(
        &mut transaction,
        entry_id,
        photo_id,
        auth.user_id,
    )
    .await?
    .ok_or_else(photo_not_found)?;
    transaction.commit().await?;

    if let Err(error) =
        process_deletion_key(&state.inner.pool, &state.inner.media_storage, &storage_key).await
    {
        tracing::warn!(photo_id, entry_id, error = %error, "Photo deletion queued for retry");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn photo_content(
    State(state): State<AppState>,
    OptionalAuthUser(auth): OptionalAuthUser,
    Path(photo_id): Path<u32>,
) -> Result<Response, AppError> {
    let photo = media_db::find_photo(&state.inner.pool, photo_id)
        .await?
        .ok_or_else(photo_not_found)?;
    let is_owner = auth.is_some_and(|user| user.user_id == photo.owner_user_id);
    if !is_owner && !photo.collection_public {
        return Err(photo_not_found());
    }

    let bytes = state
        .inner
        .media_storage
        .read(&photo.storage_key)
        .await
        .map_err(|error| {
            tracing::error!(photo_id, error = %error, "Stored photo could not be read");
            AppError::InternalError(anyhow::anyhow!("Stored photo is unavailable"))
        })?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, photo.media_type)
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::CACHE_CONTROL, "private, no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(bytes))
        .map_err(|error| AppError::InternalError(error.into()))
}

pub(super) async fn read_single_photo(
    multipart: &mut Multipart,
    max_upload_bytes: usize,
) -> Result<Vec<u8>, AppError> {
    let mut photo = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("Invalid multipart photo upload".to_string()))?
    {
        if field.name() != Some("photo") || photo.is_some() {
            return Err(AppError::BadRequest(
                "Upload exactly one file in the 'photo' field".to_string(),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| AppError::BadRequest("Could not read uploaded photo".to_string()))?
        {
            if bytes
                .len()
                .checked_add(chunk.len())
                .is_none_or(|length| length > max_upload_bytes)
            {
                return Err(AppError::PayloadTooLarge(format!(
                    "Photo exceeds the {max_upload_bytes} byte upload limit"
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        photo = Some(bytes);
    }
    photo.ok_or_else(|| AppError::BadRequest("No photo file was uploaded".to_string()))
}

pub(super) fn map_media_error(error: MediaError) -> AppError {
    match error {
        MediaError::TooLarge => AppError::PayloadTooLarge(error.to_string()),
        MediaError::Unsupported | MediaError::Invalid => AppError::BadRequest(error.to_string()),
        MediaError::Storage(_) => AppError::InternalError(error.into()),
    }
}

fn photo_not_found() -> AppError {
    AppError::NotFound("Photo not found".to_string())
}

fn photo_limit_reached() -> AppError {
    AppError::Conflict("This collection entry already has four photos".to_string())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::{Path as FsPath, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, header};
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    use lilly_importer_core::AdapterRegistry;
    use serde_json::Value;
    use sqlx::MySqlPool;
    use sqlx::mysql::MySqlPoolOptions;
    use tower::ServiceExt;

    use super::*;
    use crate::auth::jwt;
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;
    use crate::services::media::{MediaStorage, reconcile_storage};

    const TEST_JWT_SECRET: &str = "media-route-test-secret-with-safe-length";

    #[test]
    fn media_errors_map_to_stable_http_error_classes() {
        assert!(matches!(
            map_media_error(MediaError::TooLarge),
            AppError::PayloadTooLarge(_)
        ));
        assert!(matches!(
            map_media_error(MediaError::Unsupported),
            AppError::BadRequest(_)
        ));
        assert!(matches!(
            map_media_error(MediaError::Invalid),
            AppError::BadRequest(_)
        ));
    }

    #[test]
    fn hard_request_limit_allows_only_small_multipart_overhead() {
        assert_eq!(HARD_MULTIPART_REQUEST_LIMIT, 5_308_416);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn photo_routes_enforce_ownership_privacy_limits_and_cleanup() {
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
        let media_root = std::env::temp_dir().join(format!("lilly-media-route-{suffix}"));
        let owner_id = insert_user(&pool, &format!("photo-owner-{suffix}@example.test")).await;
        let other_id = insert_user(&pool, &format!("photo-other-{suffix}@example.test")).await;
        let series_id = insert_series(&pool, &format!("photo-series-{suffix}")).await;
        let issue_id = insert_issue(&pool, series_id, 1).await;
        let concurrent_issue_id = insert_issue(&pool, series_id, 2).await;
        let entry_id = insert_entry(&pool, owner_id, issue_id).await;
        let concurrent_entry_id = insert_entry(&pool, owner_id, concurrent_issue_id).await;
        let wanted_issue_id = insert_issue(&pool, series_id, 3).await;
        let wanted_entry_id = insert_wanted_entry(&pool, owner_id, wanted_issue_id).await;
        let status_change_issue_id = insert_issue(&pool, series_id, 4).await;
        let status_change_entry_id = insert_entry(&pool, owner_id, status_change_issue_id).await;
        let app = test_router(pool.clone(), &media_root);
        let png = encoded_png();

        let policy = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                "/api/v1/media/photo-policy",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(policy.status(), StatusCode::OK);
        assert_eq!(response_json(policy).await["max_photos"], 4);

        let foreign_list = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/me/collection/{entry_id}/photos"),
                Some(other_id),
            ))
            .await
            .unwrap();
        assert_eq!(foreign_list.status(), StatusCode::NOT_FOUND);

        let foreign_upload = app
            .clone()
            .oneshot(multipart_request(entry_id, b"not an image", other_id))
            .await
            .unwrap();
        assert_eq!(foreign_upload.status(), StatusCode::NOT_FOUND);

        let missing_upload = app
            .clone()
            .oneshot(multipart_request(u32::MAX, b"not an image", owner_id))
            .await
            .unwrap();
        assert_eq!(missing_upload.status(), StatusCode::NOT_FOUND);

        let wanted_upload = app
            .clone()
            .oneshot(multipart_request(
                wanted_entry_id,
                b"not an image",
                owner_id,
            ))
            .await
            .unwrap();
        assert_eq!(wanted_upload.status(), StatusCode::NOT_FOUND);

        let status_change_upload = app
            .clone()
            .oneshot(multipart_request(status_change_entry_id, &png, owner_id))
            .await
            .unwrap();
        assert_eq!(status_change_upload.status(), StatusCode::CREATED);
        let status_change_storage_key = storage_keys(&pool, status_change_entry_id)
            .await
            .pop()
            .unwrap();
        assert!(
            media_root
                .join("user-photos")
                .join(&status_change_storage_key)
                .exists()
        );
        let change_to_wanted = app
            .clone()
            .oneshot(json_request(
                Method::PATCH,
                &format!("/api/v1/me/collection/{status_change_entry_id}"),
                r#"{"status":"wanted"}"#,
                owner_id,
            ))
            .await
            .unwrap();
        assert_eq!(change_to_wanted.status(), StatusCode::OK);
        assert_eq!(photo_count(&pool, status_change_entry_id).await, 0);
        assert!(
            !media_root
                .join("user-photos")
                .join(&status_change_storage_key)
                .exists()
        );

        let oversized =
            vec![0_u8; crate::config::PhotoUploadConfig::default().max_upload_bytes + 1];
        let oversized_response = app
            .clone()
            .oneshot(multipart_request(entry_id, &oversized, owner_id))
            .await
            .unwrap();
        assert_eq!(oversized_response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(photo_count(&pool, entry_id).await, 0);

        let malformed = app
            .clone()
            .oneshot(multipart_request(entry_id, b"not an image", owner_id))
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(photo_count(&pool, entry_id).await, 0);

        let mut photo_ids = Vec::new();
        for _ in 0..4 {
            let response = app
                .clone()
                .oneshot(multipart_request(entry_id, &png, owner_id))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            photo_ids.push(
                response_json(response).await["id"]
                    .as_u64()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            );
        }
        assert_eq!(photo_count(&pool, entry_id).await, 4);
        let fifth = app
            .clone()
            .oneshot(multipart_request(entry_id, b"not an image", owner_id))
            .await
            .unwrap();
        assert_eq!(fifth.status(), StatusCode::CONFLICT);

        let private_content = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/collection-photos/{}/content", photo_ids[0]),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(private_content.status(), StatusCode::NOT_FOUND);
        let owner_content = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/collection-photos/{}/content", photo_ids[0]),
                Some(owner_id),
            ))
            .await
            .unwrap();
        assert_eq!(owner_content.status(), StatusCode::OK);
        assert_eq!(owner_content.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(owner_content.headers()["x-content-type-options"], "nosniff");

        sqlx::query("UPDATE users SET collection_public = TRUE WHERE id = ?")
            .bind(owner_id)
            .execute(&pool)
            .await
            .unwrap();
        let public_content = app
            .clone()
            .oneshot(basic_request(
                Method::GET,
                &format!("/api/v1/collection-photos/{}/content", photo_ids[0]),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(public_content.status(), StatusCode::OK);
        sqlx::query("UPDATE users SET collection_public = FALSE WHERE id = ?")
            .bind(owner_id)
            .execute(&pool)
            .await
            .unwrap();

        let foreign_delete = app
            .clone()
            .oneshot(basic_request(
                Method::DELETE,
                &format!("/api/v1/me/collection/{entry_id}/photos/{}", photo_ids[0]),
                Some(other_id),
            ))
            .await
            .unwrap();
        assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);
        let deleted_storage_key = media_db::find_photo(&pool, photo_ids[0])
            .await
            .unwrap()
            .unwrap()
            .storage_key;
        let own_delete = app
            .clone()
            .oneshot(basic_request(
                Method::DELETE,
                &format!("/api/v1/me/collection/{entry_id}/photos/{}", photo_ids[0]),
                Some(owner_id),
            ))
            .await
            .unwrap();
        assert_eq!(own_delete.status(), StatusCode::NO_CONTENT);
        assert_eq!(photo_count(&pool, entry_id).await, 3);
        assert!(
            !media_root
                .join("user-photos")
                .join(deleted_storage_key)
                .exists()
        );

        let refill = app
            .clone()
            .oneshot(multipart_request(entry_id, &png, owner_id))
            .await
            .unwrap();
        assert_eq!(refill.status(), StatusCode::CREATED);
        assert_eq!(photo_count(&pool, entry_id).await, 4);

        let mut uploads = Vec::new();
        for _ in 0..5 {
            let app = app.clone();
            let request = multipart_request(concurrent_entry_id, &png, owner_id);
            uploads.push(tokio::spawn(async move {
                app.oneshot(request).await.unwrap().status()
            }));
        }
        let mut created = 0;
        let mut conflicts = 0;
        for upload in uploads {
            match upload.await.unwrap() {
                StatusCode::CREATED => created += 1,
                StatusCode::CONFLICT => conflicts += 1,
                status => panic!("unexpected concurrent upload status {status}"),
            }
        }
        assert_eq!((created, conflicts), (4, 1));
        assert_eq!(photo_count(&pool, concurrent_entry_id).await, 4);

        let entry_storage_keys = storage_keys(&pool, entry_id).await;

        let delete_entry = app
            .oneshot(basic_request(
                Method::DELETE,
                &format!("/api/v1/me/collection/{entry_id}"),
                Some(owner_id),
            ))
            .await
            .unwrap();
        assert_eq!(delete_entry.status(), StatusCode::NO_CONTENT);
        assert_eq!(photo_count(&pool, entry_id).await, 0);
        assert!(
            entry_storage_keys
                .iter()
                .all(|storage_key| !media_root.join("user-photos").join(storage_key).exists())
        );

        let account_storage_keys = storage_keys(&pool, concurrent_entry_id).await;

        sqlx::query("DELETE FROM users WHERE id IN (?, ?)")
            .bind(owner_id)
            .bind(other_id)
            .execute(&pool)
            .await
            .unwrap();
        let reconciliation = reconcile_storage(&pool, &MediaStorage::new(&media_root))
            .await
            .unwrap();
        assert_eq!(reconciliation.removed_orphans, 4);
        assert!(
            account_storage_keys
                .iter()
                .all(|storage_key| !media_root.join("user-photos").join(storage_key).exists())
        );
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(media_root).await;
    }

    fn test_router(pool: MySqlPool, media_root: &FsPath) -> Router {
        let storage = crate::services::media::MediaStorage::new(media_root);
        Router::new()
            .merge(router())
            .merge(crate::routes::collection::router())
            .with_state(AppState {
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
                    erasure_ledger: crate::services::account_erasure::ErasureLedger::new(
                        media_root.join("erasure-ledger"),
                    ),
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

    fn multipart_request(entry_id: u32, bytes: &[u8], user_id: u32) -> Request<Body> {
        const BOUNDARY: &str = "lilly-photo-boundary";
        let mut body = format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"photo.png\"\r\nContent-Type: text/plain\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/me/collection/{entry_id}/photos"))
            .header(header::COOKIE, auth_cookie(user_id))
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(body))
            .unwrap()
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

    fn auth_cookie(user_id: u32) -> String {
        let token =
            jwt::create_token(user_id, "Photo Tester", "user", TEST_JWT_SECRET, 3_600).unwrap();
        format!("access_token={token}")
    }

    async fn response_json(response: Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn encoded_png() -> Vec<u8> {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(8, 4, Rgba([20, 40, 60, 255])));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    async fn insert_user(pool: &MySqlPool, email: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO users (email, display_name) VALUES (?, 'Photo Tester')")
                .bind(email)
                .execute(pool)
                .await
                .unwrap(),
        )
    }

    async fn insert_series(pool: &MySqlPool, slug: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO series (name, slug, active) VALUES (?, ?, TRUE)")
                .bind(format!("Photo Test {slug}"))
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
                .bind(format!("Photo Issue {issue_number}"))
                .execute(pool)
                .await
                .unwrap(),
        )
    }

    async fn insert_entry(pool: &MySqlPool, user_id: u32, issue_id: u32) -> u32 {
        inserted_id(
            &sqlx::query(
                "INSERT INTO collection_entries \
                 (user_id, issue_id, copy_number, condition_grade, status) \
                 VALUES (?, ?, 1, 'Z1', 'owned')",
            )
            .bind(user_id)
            .bind(issue_id)
            .execute(pool)
            .await
            .unwrap(),
        )
    }

    async fn insert_wanted_entry(pool: &MySqlPool, user_id: u32, issue_id: u32) -> u32 {
        inserted_id(
            &sqlx::query(
                "INSERT INTO collection_entries \
                 (user_id, issue_id, copy_number, condition_grade, status) \
                 VALUES (?, ?, 1, NULL, 'wanted')",
            )
            .bind(user_id)
            .bind(issue_id)
            .execute(pool)
            .await
            .unwrap(),
        )
    }

    fn inserted_id(result: &sqlx::mysql::MySqlQueryResult) -> u32 {
        result.last_insert_id().try_into().unwrap()
    }

    async fn photo_count(pool: &MySqlPool, entry_id: u32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM collection_photos WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn storage_keys(pool: &MySqlPool, entry_id: u32) -> Vec<String> {
        sqlx::query_scalar("SELECT storage_key FROM collection_photos WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_all(pool)
            .await
            .unwrap()
    }
}
