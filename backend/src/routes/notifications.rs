use axum::Json;
#[cfg(test)]
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::notifications;
use crate::error::AppError;
use crate::models::notifications::{
    NotificationQueryParams, NotificationResponse, PaginatedNotificationsResponse,
    UnreadCountResponse,
};

#[cfg(test)]
#[allow(dead_code)]
pub fn router() -> Router<AppState> {
    openapi_router().into()
}

pub fn openapi_router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new()
        .routes(utoipa_axum::routes!(list_notifications))
        .routes(utoipa_axum::routes!(unread_count))
        .routes(utoipa_axum::routes!(mark_read))
        .routes(utoipa_axum::routes!(mark_all_read))
}

#[utoipa::path(
    get,
    path = "/api/v1/me/notifications",
    operation_id = "notifications_list_notifications",
    tag = "notifications",
    summary = "List notifications",
    description = "List notifications. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    params(NotificationQueryParams),
    responses(
        (status = 200, description = "Successful operation", body = PaginatedNotificationsResponse),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<NotificationQueryParams>,
) -> Result<Json<PaginatedNotificationsResponse>, AppError> {
    let total =
        notifications::count_notifications(&state.inner.pool, auth.user_id, params.unread_only)
            .await?;
    let data = notifications::find_notifications(&state.inner.pool, auth.user_id, &params)
        .await?
        .into_iter()
        .map(NotificationResponse::from)
        .collect();
    Ok(Json(PaginatedNotificationsResponse {
        data,
        page: params.pagination.page(),
        per_page: params.pagination.per_page(),
        total,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/me/notifications/unread-count",
    operation_id = "notifications_unread_count",
    tag = "notifications",
    summary = "Unread count",
    description = "Unread count. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    responses(
        (status = 200, description = "Successful operation", body = UnreadCountResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn unread_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UnreadCountResponse>, AppError> {
    Ok(Json(UnreadCountResponse {
        unread_count: notifications::count_notifications(&state.inner.pool, auth.user_id, true)
            .await?,
    }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/me/notifications/{notification_id}/read",
    operation_id = "notifications_mark_read",
    tag = "notifications",
    summary = "Mark read",
    description = "Mark read. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    params(("notification_id" = u32, Path, description = "notification id", minimum = 1)),
    responses(
        (status = 204, description = "Successful operation"),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(notification_id): Path<u32>,
) -> Result<StatusCode, AppError> {
    if notification_id == 0
        || !notifications::mark_notification_read(&state.inner.pool, auth.user_id, notification_id)
            .await?
    {
        return Err(resource_not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/me/notifications/read-all",
    operation_id = "notifications_mark_all_read",
    tag = "notifications",
    summary = "Mark all read",
    description = "Mark all read. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    responses(
        (status = 204, description = "Successful operation"),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn mark_all_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {
    notifications::mark_all_notifications_read(&state.inner.pool, auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn resource_not_found() -> AppError {
    AppError::NotFound("Resource not found".to_string())
}
