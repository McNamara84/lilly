pub mod collection;
pub mod demo;
pub mod import_jobs;
pub mod import_review;
pub mod issues;
pub mod media;
pub mod messaging;
pub mod notifications;
pub mod oauth;
pub mod password_reset_tokens;
pub mod privacy_consents;
pub mod profiles;
mod pwa_fixtures;
pub mod refresh_tokens;
pub mod series;
pub mod trade_matching;
pub mod trade_workflow;
pub mod trades;
pub mod users;

#[cfg(test)]
pub(crate) mod test_cleanup;

#[cfg(test)]
static TEST_MIGRATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Import and matching tests exercise global reconciliation queries and shared catalogs.
// Serialize their fixtures; intentional concurrency is exercised within individual tests.
#[cfg(test)]
pub static IMPORT_SYNC_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
pub async fn migrate_test_database(
    pool: &sqlx::MySqlPool,
) -> Result<(), sqlx::migrate::MigrateError> {
    let _guard = TEST_MIGRATION_LOCK.lock().await;
    sqlx::migrate!("./migrations").run(pool).await
}
pub mod account_erasure;
