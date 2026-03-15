use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use lilly_importer_core::adapter::AdapterRegistry;
use lilly_importer_core::adapters::maddrax::MaddraxAdapter;
use sqlx::mysql::MySqlPoolOptions;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod config;
mod db;
mod error;
mod models;
mod routes;
mod services;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::AppConfig::from_env();

    let pool = MySqlPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    tracing::info!("Connected to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    tracing::info!("Migrations applied");

    // Promote admin user if ADMIN_EMAIL is configured
    if let Some(ref admin_email) = config.admin_email {
        db::users::ensure_admin_role(&pool, admin_email).await;
    }

    // Seed demo user only if explicitly enabled (dev/test only)
    if std::env::var("ENABLE_DEMO_SEED")
        .unwrap_or_default()
        .eq_ignore_ascii_case("true")
    {
        if let Err(e) = db::users::seed_demo_user(&pool).await {
            tracing::error!("Failed to seed demo user: {e}");
        }
    }

    let email_service = services::email::EmailService::from_config(&config);

    let mut adapter_registry = AdapterRegistry::new();
    adapter_registry.register(Box::new(MaddraxAdapter::new()));

    let app_state = routes::AppState {
        inner: std::sync::Arc::new(routes::AppStateInner {
            pool,
            jwt_secret: config.jwt_secret,
            jwt_access_expiry: config.jwt_access_token_expiry,
            jwt_refresh_expiry: config.jwt_refresh_token_expiry,
            email_service,
            app_base_url: config.app_base_url,
            cookie_secure: config.cookie_secure,
            adapter_registry,
            media_path: PathBuf::from(config.media_path),
        }),
    };

    let app = Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::series::router())
        .merge(routes::admin::router())
        .with_state(app_state)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list([
                    "http://localhost".parse().unwrap(),
                    "http://localhost:5173".parse().unwrap(),
                    "http://localhost:80".parse().unwrap(),
                ]))
                .allow_methods(AllowMethods::list([
                    http::Method::GET,
                    http::Method::POST,
                    http::Method::PUT,
                    http::Method::DELETE,
                    http::Method::OPTIONS,
                ]))
                .allow_headers(AllowHeaders::list([
                    http::header::CONTENT_TYPE,
                    http::header::AUTHORIZATION,
                ]))
                .allow_credentials(true),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], config.backend_port));
    tracing::info!("Backend listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind listener");

    axum::serve(listener, app).await.expect("Server error");
}
