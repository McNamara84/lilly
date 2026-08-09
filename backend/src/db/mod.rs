pub mod collection;
pub mod demo;
pub mod import_jobs;
pub mod issues;
pub mod profiles;
pub mod refresh_tokens;
pub mod series;
pub mod trades;
pub mod users;

#[cfg(test)]
static TEST_MIGRATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
pub async fn migrate_test_database(
    pool: &sqlx::MySqlPool,
) -> Result<(), sqlx::migrate::MigrateError> {
    let _guard = TEST_MIGRATION_LOCK.lock().await;
    sqlx::migrate!("./migrations").run(pool).await
}
