use axum::Json;
#[cfg(test)]
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::error::AppError;
use crate::models::messaging::{
    MarkThreadReadRequest, MessageListResponse, MessagePageParams, MessageResponse,
    PaginatedThreadsResponse, SendMessageRequest, ThreadPageParams,
};
use crate::services::messaging;

#[cfg(test)]
#[allow(dead_code)]
pub fn router() -> Router<AppState> {
    openapi_router().into()
}

pub fn openapi_router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new()
        .routes(utoipa_axum::routes!(list_threads))
        .routes(utoipa_axum::routes!(list_messages, send_message))
        .routes(utoipa_axum::routes!(mark_thread_read))
}

#[utoipa::path(
    get,
    path = "/api/v1/me/messages",
    operation_id = "messages_list_threads",
    tag = "messages",
    summary = "List threads",
    description = "List threads. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    params(ThreadPageParams),
    responses(
        (status = 200, description = "Successful operation", body = PaginatedThreadsResponse),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 409, description = "State conflict", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn list_threads(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<ThreadPageParams>,
) -> Result<Json<PaginatedThreadsResponse>, AppError> {
    Ok(Json(
        messaging::list_threads(&state.inner.pool, auth.user_id, &params).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/me/messages/{thread_id}",
    operation_id = "messages_list_messages",
    tag = "messages",
    summary = "List messages",
    description = "List messages. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    params(("thread_id" = u32, Path, description = "thread id", minimum = 1), MessagePageParams),
    responses(
        (status = 200, description = "Successful operation", body = MessageListResponse),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 409, description = "State conflict", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(thread_id): Path<u32>,
    Query(params): Query<MessagePageParams>,
) -> Result<Json<MessageListResponse>, AppError> {
    Ok(Json(
        messaging::list_messages(&state.inner.pool, auth.user_id, thread_id, &params).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/me/messages/{thread_id}",
    operation_id = "messages_send_message",
    tag = "messages",
    summary = "Send message",
    description = "Send a message within a trade in which the caller participates. A repeated client message ID returns the existing message with 200 instead of 201. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    params(("thread_id" = u32, Path, description = "thread id", minimum = 1)),
    request_body = SendMessageRequest,
    responses(
        (status = 200, description = "Successful operation", body = MessageResponse),
        (status = 201, description = "Successful operation", body = MessageResponse),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 409, description = "State conflict", body = crate::openapi::ApiError),
        (status = 413, description = "Request too large", body = crate::openapi::ApiError),
        (status = 415, description = "Unsupported content type", body = crate::openapi::ApiError),
        (status = 422, description = "Invalid request shape", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn send_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(thread_id): Path<u32>,
    Json(body): Json<SendMessageRequest>,
) -> Result<(StatusCode, Json<MessageResponse>), AppError> {
    let (message, created) =
        messaging::send_message(&state.inner.pool, auth.user_id, thread_id, &body).await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(message),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/me/messages/{thread_id}/read",
    operation_id = "messages_mark_thread_read",
    tag = "messages",
    summary = "Mark thread read",
    description = "Mark thread read. Requires an active account; resource ownership and visibility are checked by the backend.",
    security(("accessCookie" = [])),
    params(("thread_id" = u32, Path, description = "thread id", minimum = 1)),
    request_body = MarkThreadReadRequest,
    responses(
        (status = 204, description = "Successful operation"),
        (status = 400, description = "Invalid request", body = crate::openapi::ApiError),
        (status = 401, description = "Authentication required", body = crate::openapi::ApiError),
        (status = 403, description = "Operation forbidden", body = crate::openapi::ApiError),
        (status = 404, description = "Resource not found or not visible", body = crate::openapi::ApiError),
        (status = 409, description = "State conflict", body = crate::openapi::ApiError),
        (status = 413, description = "Request too large", body = crate::openapi::ApiError),
        (status = 415, description = "Unsupported content type", body = crate::openapi::ApiError),
        (status = 422, description = "Invalid request shape", body = crate::openapi::ApiError),
        (status = 429, description = "Rate limit exceeded", body = crate::openapi::ApiError, headers(("Retry-After" = u64, description = "Seconds until retry"))),
        (status = 500, description = "Internal server error", body = crate::openapi::ApiError)
    )
)]
async fn mark_thread_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(thread_id): Path<u32>,
    Json(body): Json<MarkThreadReadRequest>,
) -> Result<StatusCode, AppError> {
    messaging::mark_thread_read(
        &state.inner.pool,
        auth.user_id,
        thread_id,
        body.through_message_id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Method, Request, header};
    use lilly_importer_core::adapter::AdapterRegistry;
    use sqlx::mysql::MySqlPoolOptions;
    use tower::ServiceExt;

    use super::*;
    use crate::auth::jwt;
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;

    const TEST_JWT_SECRET: &str = "message-route-test-secret";

    #[tokio::test]
    async fn message_and_notification_routes_require_authentication() {
        let app = test_router();
        let requests = [
            request(Method::GET, "/api/v1/me/messages", "", None),
            request(Method::GET, "/api/v1/me/messages/1", "", None),
            request(
                Method::POST,
                "/api/v1/me/messages/1",
                r#"{"client_message_id":"123e4567-e89b-12d3-a456-426614174000","content":"Hi"}"#,
                None,
            ),
            request(
                Method::PATCH,
                "/api/v1/me/messages/1/read",
                r#"{"through_message_id":1}"#,
                None,
            ),
            request(Method::GET, "/api/v1/me/notifications", "", None),
            request(
                Method::GET,
                "/api/v1/me/notifications/unread-count",
                "",
                None,
            ),
            request(Method::PATCH, "/api/v1/me/notifications/1/read", "", None),
            request(Method::POST, "/api/v1/me/notifications/read-all", "", None),
        ];

        for request in requests {
            let response = app
                .clone()
                .oneshot(request)
                .await
                .expect("protected route request must complete");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn message_and_notification_routes_validate_inputs_before_database_access() {
        let app = test_router();
        let invalid_requests = [
            request(
                Method::POST,
                "/api/v1/me/messages/1",
                r#"{"client_message_id":"bad","content":"Hi"}"#,
                Some(1),
            ),
            request(
                Method::POST,
                "/api/v1/me/messages/1",
                r#"{"client_message_id":"123e4567-e89b-12d3-a456-426614174000","content":"  "}"#,
                Some(1),
            ),
            request(
                Method::PATCH,
                "/api/v1/me/messages/1/read",
                r#"{"through_message_id":0}"#,
                Some(1),
            ),
            request(Method::GET, "/api/v1/me/messages/not-a-number", "", Some(1)),
        ];
        for request in invalid_requests {
            let response = app
                .clone()
                .oneshot(request)
                .await
                .expect("invalid request must complete");
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }

        let missing = app
            .oneshot(request(
                Method::PATCH,
                "/api/v1/me/notifications/0/read",
                "",
                Some(1),
            ))
            .await
            .expect("zero notification request must complete");
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    fn test_router() -> Router {
        let pool = MySqlPoolOptions::new()
            .connect_lazy("mysql://test:test@localhost/test")
            .expect("lazy test pool URL must be valid");
        router()
            .merge(crate::routes::notifications::router())
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
                    media_path: PathBuf::from("/tmp/lilly-message-route-tests"),
                    media_url_prefix: "/media".to_string(),
                    photo_upload_config: crate::config::PhotoUploadConfig::default(),
                    media_storage: crate::services::media::MediaStorage::new(std::path::Path::new(
                        "/tmp/lilly-message-route-tests",
                    )),
                    erasure_ledger: crate::services::account_erasure::ErasureLedger::new(
                        "/tmp/lilly-message-route-tests-erasure-ledger",
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

    fn request(method: Method, uri: &str, body: &str, user_id: Option<u32>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(user_id) = user_id {
            let token = jwt::create_token(user_id, "Route Tester", "user", TEST_JWT_SECRET, 3_600)
                .expect("test access token must be created");
            builder = builder.header(header::COOKIE, format!("access_token={token}"));
        }
        if !body.is_empty() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        builder
            .body(Body::from(body.to_string()))
            .expect("test request must be valid")
    }
}
