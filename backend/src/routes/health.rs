use axum::Json;
#[cfg(test)]
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

use super::AppState;

#[cfg(test)]
#[allow(dead_code)]
pub fn router() -> Router<AppState> {
    openapi_router().into()
}

pub fn openapi_router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new().routes(utoipa_axum::routes!(health_check))
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    operation_id = "health_health_check",
    tag = "health",
    summary = "Health check",
    description = "Health check.",
    security(),
    responses(
        (status = 200, description = "Successful operation", body = crate::openapi::HealthResponse),
        (status = 503, description = "Database unavailable", body = crate::openapi::HealthResponse)
    )
)]
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.inner.pool)
        .await
        .is_ok();

    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "status": if db_ok { "ok" } else { "degraded" },
            "database": db_ok
        })),
    )
}
