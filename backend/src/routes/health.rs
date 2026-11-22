use axum::Json;
#[cfg(test)]
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use super::AppState;
use crate::openapi::{HealthResponse, LivenessResponse};
use crate::services::account_erasure::ErasureLedger;
use crate::services::media::MediaStorage;

const READINESS_BUDGET: Duration = Duration::from_secs(2);

#[cfg(test)]
#[allow(dead_code)]
pub fn router() -> Router<AppState> {
    openapi_router().into()
}

pub fn openapi_router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new()
        .routes(utoipa_axum::routes!(health_check))
        .routes(utoipa_axum::routes!(liveness))
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    operation_id = "health_health_check",
    tag = "health",
    summary = "Application readiness",
    description = "Checks the database, writable photo and staging directories, media root and erasure ledger within two seconds. Temporary write probes are cleaned up even if the request times out. External integrations are not readiness dependencies. The database field reports only database connectivity.",
    security(),
    responses(
        (status = 200, description = "Successful operation", body = crate::openapi::HealthResponse),
        (status = 503, description = "A required dependency is unavailable or timed out", body = crate::openapi::HealthResponse)
    )
)]
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let (db_ok, storage_ok) = readiness(
        &state.inner.pool,
        &state.inner.media_path,
        &state.inner.erasure_ledger,
        READINESS_BUDGET,
    )
    .await;

    let ready = db_ok && storage_ok;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(HealthResponse {
            status: if ready { "ok" } else { "degraded" }.to_owned(),
            database: db_ok,
        }),
    )
}

async fn bounded_probe(future: impl Future<Output = bool>, budget: Duration) -> bool {
    tokio::time::timeout(budget, future).await.unwrap_or(false)
}

async fn storage_ready(media: &Path, ledger: &ErasureLedger) -> bool {
    tokio::fs::read_dir(media).await.is_ok()
        && MediaStorage::new(media).check_write_access().await.is_ok()
        && ledger.require_existing().await.is_ok()
}

async fn readiness(
    pool: &sqlx::MySqlPool,
    media: &Path,
    ledger: &ErasureLedger,
    budget: Duration,
) -> (bool, bool) {
    // Both probes start together; pool acquisition is included in the same budget.
    tokio::join!(
        bounded_probe(
            async { sqlx::query("SELECT 1").execute(pool).await.is_ok() },
            budget
        ),
        bounded_probe(storage_ready(media, ledger), budget),
    )
}

#[utoipa::path(
    get,
    path = "/api/v1/health/live",
    operation_id = "health_liveness",
    tag = "health",
    summary = "HTTP process liveness",
    description = "Responds without accessing the database, storage or external integrations. Use readiness for deployment and service dependencies.",
    security(),
    responses((status = 200, description = "The HTTP process is responding", body = LivenessResponse))
)]
async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse { status: "ok" })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;

    struct Storage(std::path::PathBuf);

    impl Storage {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("lilly-health-{:016x}", rand::random::<u64>()));
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir_all(root.join("user-photos/.tmp")).unwrap();
            std::fs::write(root.join("ledger"), "").unwrap();
            Self(root)
        }

        fn ledger(&self) -> ErasureLedger {
            ErasureLedger::new(self.0.join("ledger"))
        }
    }

    impl Drop for Storage {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn probes_complete_or_expire_without_leaking_errors() {
        assert!(bounded_probe(async { true }, READINESS_BUDGET).await);
        assert!(!bounded_probe(async { false }, READINESS_BUDGET).await);
        let start = tokio::time::Instant::now();
        assert!(!bounded_probe(std::future::pending(), READINESS_BUDGET).await);
        assert_eq!(start.elapsed(), READINESS_BUDGET);
    }

    #[tokio::test]
    async fn storage_checks_clean_up_probes_and_reject_missing_or_invalid_paths() {
        let storage = Storage::new();
        assert!(storage_ready(&storage.0, &storage.ledger()).await);
        assert_eq!(std::fs::read(storage.0.join("ledger")).unwrap(), b"");
        assert_eq!(std::fs::read_dir(&storage.0).unwrap().count(), 2);
        assert_eq!(
            std::fs::read_dir(storage.0.join("user-photos/.tmp"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            std::fs::read_dir(storage.0.join("user-photos"))
                .unwrap()
                .count(),
            1
        );
        assert!(!storage_ready(&storage.0.join("missing"), &storage.ledger()).await);
        assert!(!storage_ready(&storage.0.join("ledger"), &storage.ledger()).await);
        assert!(!storage_ready(&storage.0, &ErasureLedger::new(&storage.0)).await);
        std::fs::remove_dir(storage.0.join("user-photos/.tmp")).unwrap();
        assert!(!storage_ready(&storage.0, &storage.ledger()).await);
        std::fs::write(storage.0.join("user-photos/.tmp"), "not a directory").unwrap();
        assert!(!storage_ready(&storage.0, &storage.ledger()).await);
        std::fs::remove_file(storage.0.join("ledger")).unwrap();
        assert!(!storage_ready(&storage.0, &storage.ledger()).await);
    }

    #[tokio::test]
    async fn health_routes_bypass_authentication_and_general_rate_limits() {
        let storage = Storage::new();
        let mut state = crate::routes::admin::tests::test_state();
        let inner = Arc::get_mut(&mut state.inner).unwrap();
        inner.pool.close().await;
        inner.media_path.clone_from(&storage.0);
        inner.erasure_ledger = storage.ledger();
        let mut limits = crate::config::RateLimitConfig::default();
        limits.public_api.max_requests = 0;
        inner.request_security =
            crate::services::rate_limit::RequestSecurity::new(limits, Vec::new(), "health-tests");
        let app = crate::api::application(state, false);
        for (path, status, expected) in [
            (
                "/api/v1/health",
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"status":"degraded", "database":false}),
            ),
            (
                "/api/v1/health/live",
                StatusCode::OK,
                json!({"status":"ok"}),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            let value: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap())
                    .unwrap();
            assert_eq!(value, expected);
        }
    }

    #[tokio::test]
    async fn database_readiness_distinguishes_storage_failure_and_pool_exhaustion() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let storage = Storage::new();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        assert_eq!(
            readiness(&pool, &storage.0, &storage.ledger(), READINESS_BUDGET).await,
            (true, true)
        );
        let mut state = crate::routes::admin::tests::test_state();
        let inner = Arc::get_mut(&mut state.inner).unwrap();
        inner.pool = pool.clone();
        inner.media_path.clone_from(&storage.0);
        inner.erasure_ledger = storage.ledger();
        let app = crate::api::application(state, false);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
        assert_eq!(value, json!({"status":"ok", "database":true}));
        assert_eq!(
            readiness(
                &pool,
                &storage.0.join("absent"),
                &storage.ledger(),
                READINESS_BUDGET
            )
            .await,
            (true, false)
        );
        let held_connection = pool.acquire().await.unwrap();
        assert_eq!(
            readiness(
                &pool,
                &storage.0,
                &storage.ledger(),
                Duration::from_millis(20)
            )
            .await,
            (false, true)
        );
        drop(held_connection);
        assert_eq!(
            readiness(&pool, &storage.0, &storage.ledger(), READINESS_BUDGET).await,
            (true, true)
        );
        pool.close().await;
        assert_eq!(
            readiness(&pool, &storage.0, &storage.ledger(), READINESS_BUDGET).await,
            (false, true)
        );
    }
}
