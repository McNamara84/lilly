use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::error::AppError;
use crate::models::messaging::{
    MarkThreadReadRequest, MessageListResponse, MessagePageParams, MessageResponse,
    PaginatedThreadsResponse, SendMessageRequest, ThreadPageParams,
};
use crate::services::messaging;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/messages", get(list_threads))
        .route(
            "/api/v1/me/messages/{thread_id}",
            get(list_messages).post(send_message),
        )
        .route(
            "/api/v1/me/messages/{thread_id}/read",
            patch(mark_thread_read),
        )
}

async fn list_threads(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<ThreadPageParams>,
) -> Result<Json<PaginatedThreadsResponse>, AppError> {
    Ok(Json(
        messaging::list_threads(&state.inner.pool, auth.user_id, &params).await?,
    ))
}

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
                    email_service: EmailService::Log {
                        from: "test@lilly.app".to_string(),
                    },
                    app_base_url: "http://localhost".to_string(),
                    cookie_secure: false,
                    adapter_registry: AdapterRegistry::new(),
                    media_path: PathBuf::from("/tmp/lilly-message-route-tests"),
                    media_url_prefix: "/media".to_string(),
                    import_scheduler_config: ImportSchedulerConfig {
                        enabled: false,
                        schedule: "0 10 6 * * Sat *".to_string(),
                        timezone: "Europe/Berlin".to_string(),
                        adapters: Vec::new(),
                    },
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
