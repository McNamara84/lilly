use axum::Json;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::Serialize;
use tokio_util::io::ReaderStream;

use super::AppState;
use crate::auth::middleware::{AuthUser, RecentAuthUser};
use crate::db::collection_transfer as db;
use crate::error::AppError;
use crate::models::collection_transfer::{JobResponse, TransferQuery};
use crate::services::{collection_transfer as format, data_transfer};

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct CollectionUpload {
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(utoipa::ToSchema)]
#[schema(value_type = String, format = Binary)]
#[allow(dead_code)]
pub struct TransferDownload(Vec<u8>);

#[derive(Serialize, utoipa::ToSchema)]
#[allow(clippy::struct_excessive_bools)]
#[schema(as = collection_transfer::ReauthenticationOptions)]
pub struct ReauthenticationOptions {
    pub recent_authentication: bool,
    pub password: bool,
    pub google: bool,
    pub github: bool,
}

pub fn openapi_router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new()
        .routes(utoipa_axum::routes!(export_collection))
        .routes(utoipa_axum::routes!(template))
        .routes(utoipa_axum::routes!(preview_import))
        .routes(utoipa_axum::routes!(import_status))
        .routes(utoipa_axum::routes!(commit_import))
        .routes(utoipa_axum::routes!(request_export, list_exports))
        .routes(utoipa_axum::routes!(export_status))
        .routes(utoipa_axum::routes!(download_export))
        .routes(utoipa_axum::routes!(reauthentication_options))
        .layer(DefaultBodyLimit::disable())
}

fn selected_format(query: &TransferQuery) -> Result<&str, AppError> {
    match query.format.as_deref().unwrap_or("json") {
        format @ ("csv" | "json") => Ok(format),
        _ => Err(AppError::BadRequest("Choose csv or json".to_string())),
    }
}

fn download(bytes: Vec<u8>, name: &str, format: &str) -> Response {
    let content_type = if format == "csv" {
        "text/csv; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}.{format}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    )
        .into_response()
}

#[utoipa::path(get, path = "/api/v1/me/collection/export", operation_id = "collection_transfer_export", tag = "collection_transfer",
    summary = "Download the complete own collection as CSV or JSON", security(("accessCookie" = [])), params(TransferQuery),
    responses((status = 200, description = "Successful operation", content((crate::models::collection_transfer::CollectionDocument = "application/json"), (TransferDownload = "text/csv"))), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn export_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<TransferQuery>,
) -> Result<Response, AppError> {
    let selected = selected_format(&query)?;
    let mut connection = state.inner.pool.acquire().await?;
    let entries = db::export_entries(&mut connection, auth.user_id).await?;
    Ok(download(
        format::encode_document(&entries, selected, &Utc::now().to_rfc3339())?,
        "lilly-collection",
        selected,
    ))
}

#[utoipa::path(get, path = "/api/v1/me/collection/import/template", operation_id = "collection_transfer_template", tag = "collection_transfer",
    summary = "Download a version 1 collection template", security(("accessCookie" = [])), params(TransferQuery),
    responses((status = 200, description = "Successful operation", content((crate::models::collection_transfer::CollectionDocument = "application/json"), (TransferDownload = "text/csv"))), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn template(
    _auth: AuthUser,
    Query(query): Query<TransferQuery>,
) -> Result<Response, AppError> {
    let selected = selected_format(&query)?;
    Ok(download(
        format::encode_document(
            &[format::template_entry()],
            selected,
            &Utc::now().to_rfc3339(),
        )?,
        "lilly-template",
        selected,
    ))
}

#[utoipa::path(post, path = "/api/v1/me/collection/imports/preview", operation_id = "collection_transfer_preview", tag = "collection_transfer",
    summary = "Validate a private import file without changing the collection", security(("accessCookie" = [])), params(TransferQuery),
    request_body(content = CollectionUpload, content_type = "multipart/form-data"),
    responses((status = 201, description = "Import preview created", body = JobResponse), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 413, description = "Configured transfer size exceeded", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn preview_import(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<TransferQuery>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<JobResponse>), AppError> {
    let selected = selected_format(&query)?;
    let mut contents = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("Invalid multipart upload".to_string()))?
    {
        if field.name() != Some("file") || contents.is_some() {
            return Err(AppError::BadRequest(
                "Upload exactly one file field".to_string(),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| AppError::BadRequest("Upload interrupted".to_string()))?
        {
            if bytes.len().saturating_add(chunk.len())
                > state.inner.transfer_config.max_import_bytes
            {
                return Err(AppError::PayloadTooLarge(format!(
                    "Import exceeds {} bytes",
                    state.inner.transfer_config.max_import_bytes
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        contents = Some(bytes);
    }
    let bytes = contents.ok_or_else(|| AppError::BadRequest("A file is required".to_string()))?;
    let now = Utc::now().naive_utc();
    let job = data_transfer::preview(&state.inner, auth.user_id, &bytes, selected, now).await?;
    Ok((
        StatusCode::CREATED,
        Json(db::response(&state.inner.pool, job, query.page.unwrap_or(1), now).await?),
    ))
}

#[utoipa::path(get, path = "/api/v1/me/collection/imports/{id}", operation_id = "collection_transfer_import_status", tag = "collection_transfer",
    summary = "Read own import progress and paginated row results", security(("accessCookie" = [])), params(("id" = String, Path), TransferQuery),
    responses((status = 200, description = "Successful operation", body = JobResponse), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 404, description = "Transfer not found", body = crate::openapi::ApiError), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn import_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<TransferQuery>,
) -> Result<Json<JobResponse>, AppError> {
    let job = db::owned_job(&state.inner.pool, &id, auth.user_id, "import").await?;
    Ok(Json(
        db::response(
            &state.inner.pool,
            job,
            query.page.unwrap_or(1),
            Utc::now().naive_utc(),
        )
        .await?,
    ))
}

#[utoipa::path(post, path = "/api/v1/me/collection/imports/{id}/commit", operation_id = "collection_transfer_commit", tag = "collection_transfer",
    summary = "Confirm an unchanged own preview and queue additive import", security(("accessCookie" = [])), params(("id" = String, Path)),
    responses((status = 202, description = "Transfer queued", body = JobResponse), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 404, description = "Transfer not found", body = crate::openapi::ApiError), (status = 409, description = "Transfer state conflict", body = crate::openapi::ApiError), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn commit_import(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<JobResponse>), AppError> {
    let now = Utc::now().naive_utc();
    let job = data_transfer::commit_import(&state.inner.pool, &id, auth.user_id, now).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(db::response(&state.inner.pool, job, 1, now).await?),
    ))
}

#[utoipa::path(post, path = "/api/v1/me/data-exports", operation_id = "personal_data_export_request", tag = "data_export",
    summary = "Queue a complete personal ZIP export after recent authentication", security(("accessCookie" = [])),
    responses((status = 202, description = "Transfer queued", body = JobResponse), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 409, description = "Transfer state conflict", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn request_export(
    State(state): State<AppState>,
    auth: RecentAuthUser,
) -> Result<(StatusCode, Json<JobResponse>), AppError> {
    let now = Utc::now().naive_utc();
    let job = data_transfer::new_job(auth.0.user_id, "export", "zip", now);
    db::create_job(
        &state.inner.pool,
        &job,
        state.inner.transfer_config.max_jobs_per_day,
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(db::response(&state.inner.pool, job, 1, now).await?),
    ))
}

#[utoipa::path(get, path = "/api/v1/me/data-exports", operation_id = "personal_data_export_list", tag = "data_export",
    summary = "List own recent export jobs", security(("accessCookie" = [])), params(TransferQuery),
    responses((status = 200, description = "Successful operation", body = Vec<JobResponse>), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn list_exports(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<TransferQuery>,
) -> Result<Json<Vec<JobResponse>>, AppError> {
    let page = query.page.unwrap_or(1).clamp(1, 100_000);
    let jobs = sqlx::query_as("SELECT * FROM transfer_jobs WHERE user_id = ? AND kind = 'export' ORDER BY created_at DESC, id DESC LIMIT 20 OFFSET ?")
        .bind(auth.user_id).bind(u64::from(page - 1) * 20).fetch_all(&state.inner.pool).await?;
    let mut responses = Vec::new();
    for job in jobs {
        responses.push(db::response(&state.inner.pool, job, 1, Utc::now().naive_utc()).await?);
    }
    Ok(Json(responses))
}

#[utoipa::path(get, path = "/api/v1/me/data-exports/{id}", operation_id = "personal_data_export_status", tag = "data_export",
    summary = "Read own personal export status and deadline", security(("accessCookie" = [])), params(("id" = String, Path)),
    responses((status = 200, description = "Successful operation", body = JobResponse), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 404, description = "Transfer not found", body = crate::openapi::ApiError), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn export_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, AppError> {
    let job = db::owned_job(&state.inner.pool, &id, auth.user_id, "export").await?;
    Ok(Json(
        db::response(&state.inner.pool, job, 1, Utc::now().naive_utc()).await?,
    ))
}

#[utoipa::path(get, path = "/api/v1/me/data-exports/{id}/download", operation_id = "personal_data_export_download", tag = "data_export",
    summary = "Download own unexpired ZIP after recent authentication", security(("accessCookie" = [])), params(("id" = String, Path)),
    responses((status = 200, description = "Successful operation", body = TransferDownload, content_type = "application/zip"), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 404, description = "Transfer not found", body = crate::openapi::ApiError), (status = 409, description = "Transfer state conflict", body = crate::openapi::ApiError), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn download_export(
    State(state): State<AppState>,
    auth: RecentAuthUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let job = db::owned_job(&state.inner.pool, &id, auth.0.user_id, "export").await?;
    if job.status != "ready"
        || job.files_deleted
        || job.invalidated_at.is_some()
        || job.expires_at <= Utc::now().naive_utc()
    {
        return Err(AppError::ConflictWithCode {
            message: "Export is not available; request a new export".to_string(),
            code: "export_unavailable".to_string(),
        });
    }
    let mut tx = state.inner.pool.begin().await?;
    crate::services::data_export::lock_valid_subjects(&mut tx, &id).await?;
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM transfer_jobs WHERE id = ? AND invalidated_at IS NULL AND expires_at > CURRENT_TIMESTAMP(6) FOR UPDATE")
        .bind(&id).fetch_optional(&mut *tx).await?;
    if status.as_deref() != Some("ready") {
        return Err(AppError::Conflict("Export is unavailable".to_string()));
    }
    let file = tokio::fs::File::open(
        state
            .inner
            .transfer_config
            .job_path(&id)?
            .join("archive.zip"),
    )
    .await
    .map_err(|_| {
        AppError::NotFound("Export file is unavailable; request a new export".to_string())
    })?;
    if file
        .metadata()
        .await
        .map_err(|e| AppError::InternalError(e.into()))?
        .len()
        != job.byte_size
    {
        return Err(AppError::Conflict("Export file is incomplete".to_string()));
    }
    tx.commit().await?;
    let mut response = Response::new(Body::from_stream(ReaderStream::new(file)));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"lilly-personal-data-{id}.zip\"")
            .parse()
            .map_err(|e| AppError::InternalError(anyhow::anyhow!("{e}")))?,
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, job.byte_size.into());
    Ok(response)
}

#[utoipa::path(get, path = "/api/v1/me/reauthentication/options", operation_id = "personal_data_reauthentication_options", tag = "data_export",
    summary = "Read own available reauthentication methods", security(("accessCookie" = [])),
    responses((status = 200, description = "Successful operation", body = ReauthenticationOptions), (status = 401, description = "Authentication required", body = crate::openapi::ApiError), (status = 400, description = "Invalid request", body = crate::openapi::ApiError), (status = 403, description = "Operation forbidden or recent authentication required", body = crate::openapi::ApiError), (status = 429, description = "Transfer or API rate limit exceeded", body = crate::openapi::ApiError), (status = 500, description = "Internal server error", body = crate::openapi::ApiError)))]
async fn reauthentication_options(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ReauthenticationOptions>, AppError> {
    let (password, google, github): (bool, bool, bool) = sqlx::query_as("SELECT password_hash IS NOT NULL, EXISTS(SELECT 1 FROM oauth_identities WHERE user_id = users.id AND provider = 'google'), EXISTS(SELECT 1 FROM oauth_identities WHERE user_id = users.id AND provider = 'github') FROM users WHERE id = ?")
        .bind(auth.user_id).fetch_one(&state.inner.pool).await?;
    let authenticated_at = i64::try_from(auth.auth_time).unwrap_or(i64::MAX);
    let age = Utc::now().timestamp().saturating_sub(authenticated_at);
    Ok(Json(ReauthenticationOptions {
        password,
        google: google
            && state
                .inner
                .oauth_service
                .is_enabled(crate::models::oauth::OAuthProvider::Google),
        github: github
            && state
                .inner
                .oauth_service
                .is_enabled(crate::models::oauth::OAuthProvider::GitHub),
        recent_authentication: authenticated_at > 0
            && (-60..=crate::models::account_erasure::RECENT_AUTH_SECONDS).contains(&age),
    }))
}
