use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::notifications;
use crate::error::AppError;
use crate::models::notifications::{
    NotificationQueryParams, NotificationResponse, PaginatedNotificationsResponse,
    UnreadCountResponse,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/notifications", get(list_notifications))
        .route("/api/v1/me/notifications/unread-count", get(unread_count))
        .route(
            "/api/v1/me/notifications/{notification_id}/read",
            patch(mark_read),
        )
        .route("/api/v1/me/notifications/read-all", post(mark_all_read))
}

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

async fn unread_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UnreadCountResponse>, AppError> {
    Ok(Json(UnreadCountResponse {
        unread_count: notifications::count_notifications(&state.inner.pool, auth.user_id, true)
            .await?,
    }))
}

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
