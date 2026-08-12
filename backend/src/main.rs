// SQLx transaction/connection futures exceed Clippy's generic size heuristic.
// Boxing every database call would add indirection throughout the persistence layer.
#![allow(clippy::large_futures)]

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use sqlx::mysql::MySqlPoolOptions;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::models::user::normalize_email;
use crate::services::admin_roles::{PromotionResult, RoleChangeMethod};

mod auth;
mod config;
mod db;
mod error;
mod models;
mod routes;
mod services;

#[derive(Debug, PartialEq, Eq)]
enum StartupCommand {
    Serve,
    PromoteAdmin { email: String },
}

const EXIT_SUCCESS: i32 = 0;
const EXIT_DATABASE_ERROR: i32 = 1;
const EXIT_INVALID_INPUT: i32 = 2;
const EXIT_USER_NOT_FOUND: i32 = 3;
const EXIT_ALREADY_ADMIN: i32 = 4;

fn parse_startup_command(args: impl IntoIterator<Item = String>) -> Result<StartupCommand, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [] => Ok(StartupCommand::Serve),
        [admin, promote, email_flag, email]
            if admin == "admin" && promote == "promote" && email_flag == "--email" =>
        {
            Ok(StartupCommand::PromoteAdmin {
                email: email.clone(),
            })
        }
        _ => Err("Usage: lilly-backend [admin promote --email user@example.org]".to_string()),
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let command = parse_startup_command(std::env::args().skip(1)).unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(EXIT_INVALID_INPUT);
    });
    if let StartupCommand::PromoteAdmin { email } = command {
        let exit_code = run_admin_promotion(&email).await;
        std::process::exit(exit_code);
    }

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

    // Reconcile any import jobs orphaned by a previous server shutdown
    match db::import_jobs::reconcile_orphaned_jobs(&pool).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!(count = n, "Marked orphaned import jobs as interrupted"),
        Err(e) => tracing::error!(error = %e, "Failed to reconcile orphaned import jobs"),
    }

    // Seed deterministic demo data only if explicitly enabled (dev/test only)
    if config.e2e.demo_seed_enabled
        && let Err(e) = db::demo::seed_demo_data(&pool, config.e2e.worker_count).await
    {
        tracing::error!("Failed to seed demo data: {e}");
    }

    let media_path = PathBuf::from(&config.media_path);
    let media_storage = prepare_media_storage(&pool, &media_path).await;

    let match_stats = db::trade_matching::reconcile_all_matches(&pool)
        .await
        .expect("Failed to reconcile trade matches");
    tracing::info!(
        created = match_stats.created,
        updated = match_stats.updated,
        reactivated = match_stats.reactivated,
        staled = match_stats.staled,
        "Trade matches reconciled"
    );

    // Promote the configured bootstrap account before serving requests.
    if let Some(ref admin_email) = config.admin_email {
        bootstrap_admin(&pool, admin_email).await;
    }

    let email_service = services::email::EmailService::from_config(&config);

    let adapter_registry = build_adapter_registry(config.e2e.fixture_adapter_enabled);

    let import_scheduler_config = services::import_scheduler::ImportSchedulerConfig {
        enabled: config.import_scheduler_enabled,
        schedule: config.import_schedule.clone(),
        timezone: config.import_timezone.clone(),
        adapters: config.import_scheduled_adapters.clone(),
    };

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
            media_path,
            media_url_prefix: config.media_url_prefix,
            photo_upload_config: config.photo_upload,
            media_storage,
            import_scheduler_config: import_scheduler_config.clone(),
        }),
    };

    services::import_scheduler::spawn_import_scheduler(
        app_state.inner.clone(),
        import_scheduler_config,
    )
    .expect("Invalid import scheduler configuration");

    let app = Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::series::router())
        .merge(routes::collection::router())
        .merge(routes::profiles::router())
        .merge(routes::trades::router())
        .merge(routes::messages::router())
        .merge(routes::media::router())
        .merge(routes::notifications::router())
        .merge(routes::admin::router())
        .with_state(app_state)
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer());

    let addr = SocketAddr::from(([0, 0, 0, 0], config.backend_port));
    tracing::info!("Backend listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind listener");

    axum::serve(listener, app).await.expect("Server error");
}

async fn prepare_media_storage(
    pool: &sqlx::MySqlPool,
    media_path: &std::path::Path,
) -> services::media::MediaStorage {
    let storage = services::media::MediaStorage::new(media_path);
    match services::media::reconcile_storage(pool, &storage).await {
        Ok(stats) => tracing::info!(
            completed_deletions = stats.completed_jobs,
            failed_deletions = stats.failed_jobs,
            removed_orphans = stats.removed_orphans,
            "User photo storage reconciled"
        ),
        Err(error) => tracing::error!(error = %error, "User photo storage reconciliation failed"),
    }
    storage
}

fn build_adapter_registry(fixture_adapter_enabled: bool) -> lilly_importer_core::AdapterRegistry {
    let mut registry = lilly_importer_adapters::builtin_registry()
        .expect("Failed to create built-in import adapters");
    if fixture_adapter_enabled {
        registry
            .register(Box::new(services::e2e_import::E2eFixtureAdapter))
            .expect("E2E fixture adapter name must be unique");
    }
    registry
}

async fn bootstrap_admin(pool: &sqlx::MySqlPool, admin_email: &str) {
    match services::admin_roles::promote_user_to_admin(
        pool,
        admin_email,
        RoleChangeMethod::AdminEmailBootstrap,
    )
    .await
    .expect("ADMIN_EMAIL bootstrap failed")
    {
        PromotionResult::Promoted { user_id } => {
            tracing::info!(
                user_id,
                method = "admin_email_bootstrap",
                "User promoted to admin"
            );
        }
        PromotionResult::AlreadyAdmin { user_id } => {
            tracing::info!(
                user_id,
                method = "admin_email_bootstrap",
                "Bootstrap user is already admin"
            );
        }
        PromotionResult::UserNotFound => {
            tracing::warn!(
                method = "admin_email_bootstrap",
                "ADMIN_EMAIL does not match a registered account; restart after registration or use the admin CLI"
            );
        }
    }
}

async fn run_admin_promotion(email: &str) -> i32 {
    let email = match normalize_email(email) {
        Ok(email) => email,
        Err(message) => {
            eprintln!("{message}");
            return EXIT_INVALID_INPUT;
        }
    };
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL must be set");
        return EXIT_DATABASE_ERROR;
    };
    let pool = match MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Failed to connect to database: {error}");
            return EXIT_DATABASE_ERROR;
        }
    };
    if let Err(error) = sqlx::migrate!("./migrations").run(&pool).await {
        eprintln!("Failed to run migrations: {error}");
        return EXIT_DATABASE_ERROR;
    }

    match services::admin_roles::promote_user_to_admin(&pool, &email, RoleChangeMethod::Cli).await {
        Ok(result) => {
            match result {
                PromotionResult::Promoted { user_id } => {
                    println!("Promoted user {user_id} to admin");
                }
                PromotionResult::AlreadyAdmin { user_id } => {
                    println!("User {user_id} is already an admin");
                }
                PromotionResult::UserNotFound => {
                    eprintln!("No registered user matches the supplied email address");
                }
            }
            promotion_result_exit_code(result)
        }
        Err(services::admin_roles::AdminRoleError::InvalidEmail(message)) => {
            eprintln!("{message}");
            EXIT_INVALID_INPUT
        }
        Err(services::admin_roles::AdminRoleError::Database(error)) => {
            eprintln!("Admin promotion failed: {error}");
            EXIT_DATABASE_ERROR
        }
    }
}

const fn promotion_result_exit_code(result: PromotionResult) -> i32 {
    match result {
        PromotionResult::Promoted { .. } => EXIT_SUCCESS,
        PromotionResult::AlreadyAdmin { .. } => EXIT_ALREADY_ADMIN,
        PromotionResult::UserNotFound => EXIT_USER_NOT_FOUND,
    }
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            "http://localhost".parse().unwrap(),
            "http://localhost:5173".parse().unwrap(),
            "http://localhost:80".parse().unwrap(),
        ]))
        .allow_methods(AllowMethods::list([
            http::Method::GET,
            http::Method::POST,
            http::Method::PATCH,
            http::Method::PUT,
            http::Method::DELETE,
            http::Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
        ]))
        .allow_credentials(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_defaults_to_the_server() {
        assert_eq!(
            parse_startup_command(Vec::new()).unwrap(),
            StartupCommand::Serve
        );
    }

    #[test]
    fn startup_command_parses_admin_promotion_without_a_password() {
        assert_eq!(
            parse_startup_command([
                "admin".to_string(),
                "promote".to_string(),
                "--email".to_string(),
                "user@example.org".to_string(),
            ])
            .unwrap(),
            StartupCommand::PromoteAdmin {
                email: "user@example.org".to_string()
            }
        );
    }

    #[test]
    fn startup_command_rejects_unknown_or_incomplete_arguments() {
        for arguments in [
            vec!["admin".to_string()],
            vec!["admin".to_string(), "promote".to_string()],
            vec![
                "admin".to_string(),
                "promote".to_string(),
                "--password".to_string(),
                "secret".to_string(),
            ],
        ] {
            assert!(parse_startup_command(arguments).is_err());
        }
    }

    #[test]
    fn admin_cli_outcomes_have_distinct_exit_codes() {
        let codes = [
            EXIT_SUCCESS,
            EXIT_DATABASE_ERROR,
            EXIT_INVALID_INPUT,
            EXIT_USER_NOT_FOUND,
            EXIT_ALREADY_ADMIN,
        ];
        assert_eq!(
            codes
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            codes.len()
        );
        assert_eq!(
            promotion_result_exit_code(PromotionResult::Promoted { user_id: 1 }),
            EXIT_SUCCESS
        );
        assert_eq!(
            promotion_result_exit_code(PromotionResult::AlreadyAdmin { user_id: 1 }),
            EXIT_ALREADY_ADMIN
        );
        assert_eq!(
            promotion_result_exit_code(PromotionResult::UserNotFound),
            EXIT_USER_NOT_FOUND
        );
    }

    #[tokio::test]
    async fn admin_cli_rejects_invalid_email_before_database_setup() {
        assert_eq!(run_admin_promotion("invalid").await, EXIT_INVALID_INPUT);
    }
}
