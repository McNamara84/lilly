//! Shared application router and `OpenAPI` registration.

use axum::{Router, middleware};
use tower_http::trace::TraceLayer;
use utoipa_axum::router::OpenApiRouter;

use crate::routes::{self, AppState};
use crate::services::{browser_security, rate_limit};

pub fn business_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(routes::auth::openapi_router())
        .merge(routes::account_erasure::openapi_router())
        .merge(routes::oauth::openapi_router())
        .merge(routes::series::openapi_router())
        .merge(routes::collection::openapi_router())
        .merge(routes::profiles::openapi_router())
        .merge(routes::trades::openapi_router())
        .merge(routes::messages::openapi_router())
        .merge(routes::media::openapi_router())
        .merge(routes::notifications::openapi_router())
        .merge(routes::admin::openapi_router())
}

pub fn specification() -> utoipa::openapi::OpenApi {
    let mut spec = business_routes()
        .merge(routes::health::openapi_router())
        .into_openapi();
    crate::openapi::configure(&mut spec);
    spec
}

pub fn application(state: AppState, docs_enabled: bool) -> Router {
    let (business, _) = business_routes().split_for_parts();
    let (health, _) = routes::health::openapi_router().split_for_parts();
    let business = business
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::enforce_general_api_rate_limit,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            browser_security::enforce_same_origin,
        ));
    Router::new()
        .merge(health)
        .merge(business)
        .with_state(state)
        .merge(crate::openapi::documentation(docs_enabled))
        .layer(middleware::from_fn(crate::openapi::normalize_errors))
        .layer(middleware::from_fn(browser_security::add_security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(crate::cors_layer())
}
