use std::net::SocketAddr;

use axum::Router;
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

    // Seed demo user if no users exist
    db::users::seed_demo_user(&pool).await;

    let app_state = routes::AppState {
        pool,
        jwt_secret: config.jwt_secret,
        jwt_access_expiry: config.jwt_access_token_expiry,
        jwt_refresh_expiry: config.jwt_refresh_token_expiry,
    };

    let app = Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
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
                ])),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], config.backend_port));
    tracing::info!("Backend listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind listener");

    axum::serve(listener, app).await.expect("Server error");
}
