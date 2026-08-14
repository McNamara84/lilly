use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::db::trades;
use crate::error::AppError;
use crate::models::trade_matching::{
    CreateTradeProposalRequest, PageParams, PaginatedMatchesResponse, PaginatedTradesResponse,
    TradeMatchResponse, TradePageParams, TradeResponse,
};
use crate::models::trades::{
    BulkWantedRequest, BulkWantedResponse, PaginatedTradeOffersResponse,
    PaginatedWantedCandidatesResponse, PaginatedWantedResponse, TradeListQueryParams,
    TradeOfferResponse, WantedCandidateResponse, WantedEntryResponse, normalize_bulk_issue_ids,
};
use crate::services::{trade_matching as matching_service, trades as trade_service};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/trade-offers", get(list_trade_offers))
        .route("/api/v1/me/wanted", get(list_wanted_entries))
        .route("/api/v1/me/wanted/candidates", get(list_wanted_candidates))
        .route("/api/v1/me/wanted/bulk", post(add_wanted_entries))
        .route("/api/v1/me/wanted/{entry_id}", delete(delete_wanted_entry))
        .route("/api/v1/me/matches", get(list_matches))
        .route("/api/v1/me/matches/{match_id}", get(get_match))
        .route(
            "/api/v1/me/matches/{match_id}/proposals",
            post(create_trade_proposal),
        )
        .route("/api/v1/me/trades", get(list_open_trades))
        .route("/api/v1/me/trades/{trade_id}", get(get_trade))
        .route("/api/v1/me/trades/{trade_id}/accept", post(accept_trade))
        .route("/api/v1/me/trades/{trade_id}/cancel", post(cancel_trade))
        .route(
            "/api/v1/me/trades/{trade_id}/complete",
            post(complete_trade),
        )
}

async fn list_matches(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<PageParams>,
) -> Result<Json<PaginatedMatchesResponse>, AppError> {
    Ok(Json(
        matching_service::list_matches(&state.inner.pool, auth.user_id, &params).await?,
    ))
}

async fn get_match(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(match_id): Path<u32>,
) -> Result<Json<TradeMatchResponse>, AppError> {
    Ok(Json(
        matching_service::get_match(&state.inner.pool, auth.user_id, match_id).await?,
    ))
}

async fn create_trade_proposal(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(match_id): Path<u32>,
    Json(body): Json<CreateTradeProposalRequest>,
) -> Result<(StatusCode, Json<TradeResponse>), AppError> {
    let trade =
        trade_service::create_proposal(&state.inner.pool, auth.user_id, match_id, &body).await?;
    Ok((StatusCode::CREATED, Json(trade)))
}

async fn list_open_trades(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<TradePageParams>,
) -> Result<Json<PaginatedTradesResponse>, AppError> {
    Ok(Json(
        trade_service::list_trades(&state.inner.pool, auth.user_id, &params).await?,
    ))
}

async fn get_trade(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(trade_id): Path<u32>,
) -> Result<Json<TradeResponse>, AppError> {
    Ok(Json(
        trade_service::get_trade(&state.inner.pool, auth.user_id, trade_id).await?,
    ))
}

async fn accept_trade(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(trade_id): Path<u32>,
) -> Result<Json<TradeResponse>, AppError> {
    Ok(Json(
        trade_service::accept_trade(&state.inner.pool, auth.user_id, trade_id).await?,
    ))
}

async fn cancel_trade(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(trade_id): Path<u32>,
) -> Result<StatusCode, AppError> {
    trade_service::cancel_trade(&state.inner.pool, auth.user_id, trade_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn complete_trade(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(trade_id): Path<u32>,
) -> Result<Json<TradeResponse>, AppError> {
    let result = trade_service::complete_trade(&state.inner.pool, auth.user_id, trade_id).await?;
    for storage_key in result.photo_storage_keys {
        if let Err(error) = crate::services::media::process_deletion_key(
            &state.inner.pool,
            &state.inner.media_storage,
            &storage_key,
        )
        .await
        {
            tracing::warn!(
                trade_id,
                error = %error,
                "Transferred collection photo deletion queued for retry"
            );
        }
    }
    Ok(Json(result.trade))
}

async fn list_trade_offers(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<TradeListQueryParams>,
) -> Result<Json<PaginatedTradeOffersResponse>, AppError> {
    params.validate().map_err(AppError::BadRequest)?;

    let total =
        trades::count_trade_list_entries(&state.inner.pool, auth.user_id, "duplicate", &params)
            .await?;
    let rows =
        trades::find_trade_list_entries(&state.inner.pool, auth.user_id, "duplicate", &params)
            .await?;
    let data = rows
        .iter()
        .map(TradeOfferResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|message| AppError::InternalError(anyhow::anyhow!(message)))?;

    Ok(Json(PaginatedTradeOffersResponse {
        data,
        page: params.page(),
        per_page: params.per_page(),
        total,
    }))
}

async fn list_wanted_entries(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<TradeListQueryParams>,
) -> Result<Json<PaginatedWantedResponse>, AppError> {
    params.validate().map_err(AppError::BadRequest)?;

    let total =
        trades::count_trade_list_entries(&state.inner.pool, auth.user_id, "wanted", &params)
            .await?;
    let rows =
        trades::find_trade_list_entries(&state.inner.pool, auth.user_id, "wanted", &params).await?;

    Ok(Json(PaginatedWantedResponse {
        data: rows.iter().map(WantedEntryResponse::from).collect(),
        page: params.page(),
        per_page: params.per_page(),
        total,
    }))
}

async fn list_wanted_candidates(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<TradeListQueryParams>,
) -> Result<Json<PaginatedWantedCandidatesResponse>, AppError> {
    params.validate().map_err(AppError::BadRequest)?;
    if params.series_slug().is_none() {
        return Err(AppError::BadRequest(
            "series_slug is required for wanted candidates".to_string(),
        ));
    }

    let total = trades::count_wanted_candidates(&state.inner.pool, auth.user_id, &params).await?;
    let rows = trades::find_wanted_candidates(&state.inner.pool, auth.user_id, &params).await?;

    Ok(Json(PaginatedWantedCandidatesResponse {
        data: rows.iter().map(WantedCandidateResponse::from).collect(),
        page: params.page(),
        per_page: params.per_page(),
        total,
    }))
}

async fn add_wanted_entries(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<BulkWantedRequest>,
) -> Result<Json<BulkWantedResponse>, AppError> {
    let issue_ids = normalize_bulk_issue_ids(&body.issue_ids).map_err(AppError::BadRequest)?;
    let mut transaction = state.inner.pool.begin().await?;
    crate::db::trade_matching::lock_reconciliation_users_for_issues(
        &mut transaction,
        auth.user_id,
        &issue_ids,
    )
    .await?;
    let result =
        trades::add_wanted_entries_in_transaction(&mut transaction, auth.user_id, &issue_ids)
            .await?;
    if !result.created.is_empty() {
        crate::db::trade_matching::reconcile_user_matches(&mut transaction, auth.user_id).await?;
    }
    transaction.commit().await?;
    Ok(Json(result))
}

async fn delete_wanted_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(entry_id): Path<u32>,
) -> Result<StatusCode, AppError> {
    if entry_id == 0 {
        return Err(wanted_entry_not_found());
    }

    let wanted =
        crate::db::collection::find_entry_by_id_and_user(&state.inner.pool, entry_id, auth.user_id)
            .await?;
    if wanted.as_ref().is_none_or(|entry| entry.status != "wanted") {
        return Err(wanted_entry_not_found());
    }
    let mut transaction = state.inner.pool.begin().await?;
    matching_service::prepare_entry_mutation_in_transaction(
        &mut transaction,
        auth.user_id,
        entry_id,
    )
    .await?;
    let photo_storage_keys =
        crate::db::media::enqueue_entry_photo_deletions(&mut transaction, entry_id, auth.user_id)
            .await?;
    let deleted =
        trades::delete_wanted_entry_on_connection(&mut transaction, auth.user_id, entry_id).await?;
    if !deleted {
        return Err(wanted_entry_not_found());
    }

    crate::db::trade_matching::reconcile_user_matches(&mut transaction, auth.user_id).await?;
    transaction.commit().await?;

    for storage_key in photo_storage_keys {
        if let Err(error) = crate::services::media::process_deletion_key(
            &state.inner.pool,
            &state.inner.media_storage,
            &storage_key,
        )
        .await
        {
            tracing::warn!(entry_id, error = %error, "Wanted photo deletion queued for retry");
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

fn wanted_entry_not_found() -> AppError {
    AppError::NotFound("Wanted entry not found".to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, header};
    use axum::response::IntoResponse;
    use lilly_importer_core::adapter::AdapterRegistry;
    use serde_json::{Value, json};
    use sqlx::MySqlPool;
    use sqlx::mysql::MySqlPoolOptions;
    use tower::ServiceExt;

    use super::*;
    use crate::auth::jwt;
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;

    const TEST_JWT_SECRET: &str = "trade-route-test-secret";

    #[test]
    fn wanted_not_found_response_is_generic() {
        let response = wanted_entry_not_found().into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn trade_routes_reject_unauthenticated_requests() {
        let app = test_router(lazy_pool(), PathBuf::from("/tmp/lilly-trade-route-tests"));
        let requests = [
            request(Method::GET, "/api/v1/me/trade-offers", "", None),
            request(Method::GET, "/api/v1/me/wanted", "", None),
            request(
                Method::GET,
                "/api/v1/me/wanted/candidates?series_slug=test",
                "",
                None,
            ),
            request(
                Method::POST,
                "/api/v1/me/wanted/bulk",
                r#"{"issue_ids":[1]}"#,
                None,
            ),
            request(Method::DELETE, "/api/v1/me/wanted/1", "", None),
            request(Method::GET, "/api/v1/me/matches", "", None),
            request(Method::GET, "/api/v1/me/matches/1", "", None),
            request(
                Method::POST,
                "/api/v1/me/matches/1/proposals",
                r#"{"offered_entry_ids":[1],"requested_entry_ids":[2]}"#,
                None,
            ),
            request(Method::GET, "/api/v1/me/trades", "", None),
            request(Method::GET, "/api/v1/me/trades/1", "", None),
            request(Method::POST, "/api/v1/me/trades/1/accept", "", None),
            request(Method::POST, "/api/v1/me/trades/1/cancel", "", None),
            request(Method::POST, "/api/v1/me/trades/1/complete", "", None),
        ];

        for request in requests {
            let response = app
                .clone()
                .oneshot(request)
                .await
                .expect("trade route request must complete");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn trade_routes_validate_query_path_and_json_extractors() {
        let app = test_router(lazy_pool(), PathBuf::from("/tmp/lilly-trade-route-tests"));
        let long_search = "a".repeat(201);
        let long_slug = "s".repeat(101);

        let invalid_requests = [
            request(
                Method::GET,
                &format!("/api/v1/me/trade-offers?q={long_search}"),
                "",
                Some(1),
            ),
            request(
                Method::GET,
                &format!("/api/v1/me/wanted?series_slug={long_slug}"),
                "",
                Some(1),
            ),
            request(Method::GET, "/api/v1/me/wanted/candidates", "", Some(1)),
            request(
                Method::GET,
                "/api/v1/me/wanted/candidates?series_slug=%20",
                "",
                Some(1),
            ),
            request(
                Method::POST,
                "/api/v1/me/wanted/bulk",
                r#"{"issue_ids":[]}"#,
                Some(1),
            ),
            request(
                Method::POST,
                "/api/v1/me/wanted/bulk",
                r#"{"issue_ids":[0]}"#,
                Some(1),
            ),
            request(
                Method::POST,
                "/api/v1/me/wanted/bulk",
                &json!({ "issue_ids": (1..=101).collect::<Vec<u32>>() }).to_string(),
                Some(1),
            ),
            request(Method::POST, "/api/v1/me/wanted/bulk", "{", Some(1)),
            request(
                Method::DELETE,
                "/api/v1/me/wanted/not-a-number",
                "",
                Some(1),
            ),
            request(Method::GET, "/api/v1/me/matches/not-a-number", "", Some(1)),
            request(
                Method::POST,
                "/api/v1/me/matches/1/proposals",
                r#"{"offered_entry_ids":[],"requested_entry_ids":[2]}"#,
                Some(1),
            ),
            request(
                Method::POST,
                "/api/v1/me/matches/1/proposals",
                r#"{"offered_entry_ids":[1],"requested_entry_ids":[0]}"#,
                Some(1),
            ),
            request(Method::GET, "/api/v1/me/trades/not-a-number", "", Some(1)),
            request(Method::GET, "/api/v1/me/trades?scope=all", "", Some(1)),
            request(
                Method::POST,
                "/api/v1/me/trades/not-a-number/accept",
                "",
                Some(1),
            ),
            request(
                Method::POST,
                "/api/v1/me/trades/not-a-number/complete",
                "",
                Some(1),
            ),
        ];

        for request in invalid_requests {
            let response = app
                .clone()
                .oneshot(request)
                .await
                .expect("invalid trade route request must complete");
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }

        let zero_delete = app
            .oneshot(request(Method::DELETE, "/api/v1/me/wanted/0", "", Some(1)))
            .await
            .expect("zero ID delete request must complete");
        assert_eq!(zero_delete.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response_json(zero_delete).await,
            json!({ "error": "Wanted entry not found" })
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn trade_route_contracts_work_against_mariadb() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
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
        let owner_id = insert_user(&pool, &format!("route-owner-{suffix}@example.test")).await;
        let other_id = insert_user(&pool, &format!("route-other-{suffix}@example.test")).await;
        let series_id = insert_series(&pool, &format!("route-test-{suffix}")).await;
        let offer_issue_id = insert_issue(&pool, series_id, 1, "Route Offer").await;
        let wanted_issue_id = insert_issue(&pool, series_id, 2, "Route Wanted").await;
        let missing_issue_id = insert_issue(&pool, series_id, 3, "Route Missing").await;
        let owned_issue_id = insert_issue(&pool, series_id, 4, "Route Owned").await;
        let foreign_issue_id = insert_issue(&pool, series_id, 5, "Foreign Wanted").await;
        let offer_entry_id =
            insert_entry(&pool, owner_id, offer_issue_id, "duplicate", Some("Z2")).await;
        let wanted_entry_id = insert_entry(&pool, owner_id, wanted_issue_id, "wanted", None).await;
        let owned_entry_id =
            insert_entry(&pool, owner_id, owned_issue_id, "owned", Some("Z1")).await;
        let foreign_wanted_id =
            insert_entry(&pool, other_id, foreign_issue_id, "wanted", None).await;
        let media_root = std::env::temp_dir().join(format!("lilly-trade-route-{suffix}"));
        let wanted_storage_key = format!("{suffix:032x}.jpg");
        let wanted_photo_path = media_root.join("user-photos").join(&wanted_storage_key);
        tokio::fs::create_dir_all(wanted_photo_path.parent().unwrap())
            .await
            .expect("photo fixture directory must be created");
        tokio::fs::write(&wanted_photo_path, b"legacy wanted photo")
            .await
            .expect("photo fixture must be written");
        sqlx::query(
            "INSERT INTO collection_photos \
             (entry_id, storage_key, media_type, byte_size, width, height, sort_order) \
             VALUES (?, ?, 'image/jpeg', 19, 1, 1, 0)",
        )
        .bind(wanted_entry_id)
        .bind(&wanted_storage_key)
        .execute(&pool)
        .await
        .expect("wanted photo fixture must be inserted");
        let app = test_router(pool.clone(), media_root.clone());

        let offers = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/v1/me/trade-offers",
                "",
                Some(owner_id),
            ))
            .await
            .expect("offers request must complete");
        assert_eq!(offers.status(), StatusCode::OK);
        let offers = response_json(offers).await;
        assert_eq!(offers["total"], 1);
        assert_eq!(offers["data"][0]["entry_id"], offer_entry_id);

        let wanted = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/v1/me/wanted",
                "",
                Some(owner_id),
            ))
            .await
            .expect("wanted request must complete");
        assert_eq!(wanted.status(), StatusCode::OK);
        let wanted = response_json(wanted).await;
        assert_eq!(wanted["total"], 1);
        assert_eq!(wanted["data"][0]["condition_grade"], Value::Null);

        let candidates = app
            .clone()
            .oneshot(request(
                Method::GET,
                &format!("/api/v1/me/wanted/candidates?series_slug=route-test-{suffix}"),
                "",
                Some(owner_id),
            ))
            .await
            .expect("candidate request must complete");
        assert_eq!(candidates.status(), StatusCode::OK);
        let candidates = response_json(candidates).await;
        assert_eq!(candidates["total"], 3);
        assert!(candidates["data"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["issue_id"] == wanted_issue_id && item["wanted_entry_id"] == wanted_entry_id
            })
        }));

        let bulk = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/v1/me/wanted/bulk",
                &json!({ "issue_ids": [missing_issue_id, wanted_issue_id, owned_issue_id] })
                    .to_string(),
                Some(owner_id),
            ))
            .await
            .expect("bulk request must complete");
        assert_eq!(bulk.status(), StatusCode::OK);
        let bulk = response_json(bulk).await;
        assert_eq!(bulk["created"][0]["issue_id"], missing_issue_id);
        assert_eq!(bulk["unchanged"][0]["issue_id"], wanted_issue_id);
        assert_eq!(bulk["rejected"][0]["issue_id"], owned_issue_id);
        assert_eq!(bulk["rejected"][0]["reason"], "already_owned");

        for protected_entry_id in [foreign_wanted_id, owned_entry_id] {
            let response = app
                .clone()
                .oneshot(request(
                    Method::DELETE,
                    &format!("/api/v1/me/wanted/{protected_entry_id}"),
                    "",
                    Some(owner_id),
                ))
                .await
                .expect("protected delete request must complete");
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        assert_eq!(entry_count(&pool, foreign_wanted_id).await, 1);
        assert_eq!(entry_count(&pool, owned_entry_id).await, 1);

        let deleted = app
            .oneshot(request(
                Method::DELETE,
                &format!("/api/v1/me/wanted/{wanted_entry_id}"),
                "",
                Some(owner_id),
            ))
            .await
            .expect("own wanted delete request must complete");
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        assert_eq!(entry_count(&pool, wanted_entry_id).await, 0);
        assert!(!wanted_photo_path.exists());
        let deletion_processed = sqlx::query_scalar::<_, bool>(
            "SELECT processed_at IS NOT NULL FROM media_deletion_jobs WHERE storage_key = ?",
        )
        .bind(&wanted_storage_key)
        .fetch_one(&pool)
        .await
        .expect("wanted photo deletion job must exist");
        assert!(deletion_processed);

        sqlx::query("DELETE FROM users WHERE id IN (?, ?)")
            .bind(owner_id)
            .bind(other_id)
            .execute(&pool)
            .await
            .expect("user fixtures must be deleted");
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series fixture must be deleted");
        let _ = tokio::fs::remove_dir_all(media_root).await;
    }

    fn lazy_pool() -> MySqlPool {
        MySqlPoolOptions::new()
            .connect_lazy("mysql://test:test@localhost/test")
            .expect("lazy test pool URL must be valid")
    }

    fn test_router(pool: MySqlPool, media_root: PathBuf) -> Router {
        let media_storage = crate::services::media::MediaStorage::new(&media_root);
        router().with_state(AppState {
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
                media_path: media_root,
                media_url_prefix: "/media".to_string(),
                photo_upload_config: crate::config::PhotoUploadConfig::default(),
                media_storage,
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

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body must be readable");
        serde_json::from_slice(&bytes).expect("response body must contain JSON")
    }

    async fn insert_user(pool: &MySqlPool, email: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO users (email, display_name) VALUES (?, 'Route Tester')")
                .bind(email)
                .execute(pool)
                .await
                .expect("user fixture must be inserted"),
        )
    }

    async fn insert_series(pool: &MySqlPool, slug: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO series (name, slug, active) VALUES (?, ?, TRUE)")
                .bind(format!("Route Test {slug}"))
                .bind(slug)
                .execute(pool)
                .await
                .expect("series fixture must be inserted"),
        )
    }

    async fn insert_issue(pool: &MySqlPool, series_id: u32, number: u32, title: &str) -> u32 {
        inserted_id(
            &sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)")
                .bind(series_id)
                .bind(number)
                .bind(title)
                .execute(pool)
                .await
                .expect("issue fixture must be inserted"),
        )
    }

    async fn insert_entry(
        pool: &MySqlPool,
        user_id: u32,
        issue_id: u32,
        status: &str,
        condition: Option<&str>,
    ) -> u32 {
        inserted_id(
            &sqlx::query(
                "INSERT INTO collection_entries
                    (user_id, issue_id, copy_number, condition_grade, status)
                 VALUES (?, ?, 1, ?, ?)",
            )
            .bind(user_id)
            .bind(issue_id)
            .bind(condition)
            .bind(status)
            .execute(pool)
            .await
            .expect("collection fixture must be inserted"),
        )
    }

    fn inserted_id(result: &sqlx::mysql::MySqlQueryResult) -> u32 {
        result
            .last_insert_id()
            .try_into()
            .expect("fixture ID must fit into u32")
    }

    async fn entry_count(pool: &MySqlPool, entry_id: u32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM collection_entries WHERE id = ?")
            .bind(entry_id)
            .fetch_one(pool)
            .await
            .expect("collection fixture must count")
    }
}
