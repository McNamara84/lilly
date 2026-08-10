use sqlx::MySqlPool;

use super::users;

/// Seeds the deterministic user and catalogue fixture used by local demos and E2E tests.
///
/// The operation is idempotent and only runs when `ENABLE_DEMO_SEED=true`.
pub async fn seed_demo_data(pool: &MySqlPool, e2e_worker_count: u16) -> Result<(), anyhow::Error> {
    let user_id = users::seed_demo_user(pool).await?;
    let e2e_user_ids = users::seed_e2e_worker_users(pool, e2e_worker_count).await?;
    let mut transaction = pool.begin().await?;

    sqlx::query(
        "INSERT INTO series \
         (name, slug, publisher, genre, frequency, total_issues, status, active, source_url) \
         VALUES (?, ?, ?, ?, ?, ?, 'running', TRUE, ?) \
         ON DUPLICATE KEY UPDATE active = TRUE",
    )
    .bind("Maddrax – Die dunkle Zukunft der Erde")
    .bind("maddrax")
    .bind("Bastei Lübbe")
    .bind("Science-Fiction")
    .bind("14-tägig")
    .bind(1_u32)
    .bind("https://de.maddraxikon.com/wiki/Hauptseite")
    .execute(&mut *transaction)
    .await?;

    let (series_id,): (u32,) = sqlx::query_as("SELECT id FROM series WHERE slug = ?")
        .bind("maddrax")
        .fetch_one(&mut *transaction)
        .await?;

    sqlx::query(
        "INSERT IGNORE INTO issues \
         (series_id, issue_number, title, published_at, cycle, source_wiki_url, metadata_synced_at) \
         VALUES (?, 1, ?, '2000-02-08', ?, ?, CURRENT_TIMESTAMP)",
    )
    .bind(series_id)
    .bind("Der Gott aus dem Eis")
    .bind("Euree-Zyklus")
    .bind("https://de.maddraxikon.com/wiki/Quelle:MX1")
    .execute(&mut *transaction)
    .await?;

    let (issue_id,): (u32,) =
        sqlx::query_as("SELECT id FROM issues WHERE series_id = ? AND issue_number = 1")
            .bind(series_id)
            .fetch_one(&mut *transaction)
            .await?;

    sqlx::query(
        "INSERT IGNORE INTO collection_entries \
         (user_id, issue_id, copy_number, condition_grade, status, notes) \
         VALUES (?, ?, 1, 'Z1', 'owned', ?)",
    )
    .bind(user_id)
    .bind(issue_id)
    .bind("Demo-Eintrag für lokale Entwicklung und E2E-Tests")
    .execute(&mut *transaction)
    .await?;

    for e2e_user_id in e2e_user_ids {
        sqlx::query("DELETE FROM collection_entries WHERE user_id = ?")
            .bind(e2e_user_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO collection_entries \
             (user_id, issue_id, copy_number, condition_grade, status, notes) \
             VALUES (?, ?, 1, 'Z1', 'owned', ?)",
        )
        .bind(e2e_user_id)
        .bind(issue_id)
        .bind("Deterministic entry for isolated E2E workers")
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    tracing::info!("Demo catalogue seeded");
    Ok(())
}
