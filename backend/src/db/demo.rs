use sqlx::MySqlPool;

use super::users;

/// Seeds the deterministic user and catalogue fixture used by local demos and E2E tests.
///
/// The operation is idempotent and only runs when `ENABLE_DEMO_SEED=true`.
#[allow(clippy::too_many_lines)]
pub async fn seed_demo_data(pool: &MySqlPool, e2e_worker_count: u16) -> Result<(), anyhow::Error> {
    let user_id = users::seed_demo_user(pool).await?;
    let e2e_user_ids = users::seed_e2e_worker_users(pool, e2e_worker_count).await?;
    let mut transaction = pool.begin().await?;

    let total_issues = 3_u32 + u32::from(e2e_worker_count) * 2;
    sqlx::query(
        "INSERT INTO series \
         (name, slug, publisher, genre, frequency, total_issues, status, active, source_url) \
         VALUES (?, ?, ?, ?, ?, ?, 'running', TRUE, ?) \
         ON DUPLICATE KEY UPDATE active = TRUE, total_issues = VALUES(total_issues)",
    )
    .bind("Maddrax – Die dunkle Zukunft der Erde")
    .bind("maddrax")
    .bind("Bastei Lübbe")
    .bind("Science-Fiction")
    .bind("14-tägig")
    .bind(total_issues)
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
         VALUES (?, 2, ?, '2000-02-22', ?, ?, CURRENT_TIMESTAMP), \
                (?, 3, ?, '2000-03-07', ?, ?, CURRENT_TIMESTAMP)",
    )
    .bind(series_id)
    .bind("Stadt der Verdammten")
    .bind("Euree-Zyklus")
    .bind("https://de.maddraxikon.com/wiki/Quelle:MX2")
    .bind(series_id)
    .bind("Das Grabmal")
    .bind("Euree-Zyklus")
    .bind("https://de.maddraxikon.com/wiki/Quelle:MX3")
    .execute(&mut *transaction)
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

    let partner_password_hash = crate::auth::password::hash_password("e2e-partner-password")
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    for (index, e2e_user_id) in (0_u16..).zip(e2e_user_ids) {
        let worker_offer_issue_number = 4_u32 + (u32::from(index) * 2);
        let partner_offer_issue_number = worker_offer_issue_number + 1;
        sqlx::query(
            "INSERT INTO issues \
             (series_id, issue_number, title, published_at, cycle, source_wiki_url, metadata_synced_at) \
             VALUES (?, ?, ?, '2000-04-01', 'E2E-Tausch', ?, CURRENT_TIMESTAMP), \
                    (?, ?, ?, '2000-04-02', 'E2E-Tausch', ?, CURRENT_TIMESTAMP) \
             ON DUPLICATE KEY UPDATE title = VALUES(title), metadata_synced_at = CURRENT_TIMESTAMP",
        )
        .bind(series_id)
        .bind(worker_offer_issue_number)
        .bind(format!("E2E Tauschangebot {index}"))
        .bind(format!("https://example.invalid/e2e-offer-{index}"))
        .bind(series_id)
        .bind(partner_offer_issue_number)
        .bind(format!("E2E Partnerangebot {index}"))
        .bind(format!("https://example.invalid/e2e-partner-offer-{index}"))
        .execute(&mut *transaction)
        .await?;
        let (worker_offer_issue_id,): (u32,) =
            sqlx::query_as("SELECT id FROM issues WHERE series_id = ? AND issue_number = ?")
                .bind(series_id)
                .bind(worker_offer_issue_number)
                .fetch_one(&mut *transaction)
                .await?;
        let (partner_offer_issue_id,): (u32,) =
            sqlx::query_as("SELECT id FROM issues WHERE series_id = ? AND issue_number = ?")
                .bind(series_id)
                .bind(partner_offer_issue_number)
                .fetch_one(&mut *transaction)
                .await?;

        let partner_email = format!("e2e-partner-{index}@lilly.app");
        let partner_display_name = format!("E2E Partner {index}");
        sqlx::query(
            "INSERT INTO users (email, password_hash, display_name, role, email_verified) \
             VALUES (?, ?, ?, 'user', TRUE) \
             ON DUPLICATE KEY UPDATE password_hash = VALUES(password_hash), \
             display_name = VALUES(display_name), role = 'user', email_verified = TRUE, \
             profile_public = FALSE, collection_public = FALSE",
        )
        .bind(&partner_email)
        .bind(&partner_password_hash)
        .bind(&partner_display_name)
        .execute(&mut *transaction)
        .await?;
        let (partner_user_id,): (u32,) = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(&partner_email)
            .fetch_one(&mut *transaction)
            .await?;

        sqlx::query(
            "DELETE FROM trade_matches
             WHERE user_low_id IN (?, ?) OR user_high_id IN (?, ?)",
        )
        .bind(e2e_user_id)
        .bind(partner_user_id)
        .bind(e2e_user_id)
        .bind(partner_user_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM collection_entries WHERE user_id IN (?, ?)")
            .bind(e2e_user_id)
            .bind(partner_user_id)
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

        sqlx::query(
            "INSERT INTO collection_entries \
             (user_id, issue_id, copy_number, condition_grade, status, notes) \
             VALUES (?, ?, 1, 'Z1', 'duplicate', ?), \
                    (?, ?, 1, NULL, 'wanted', ?), \
                    (?, ?, 1, NULL, 'wanted', ?), \
                    (?, ?, 1, 'Z2', 'duplicate', ?)",
        )
        .bind(e2e_user_id)
        .bind(worker_offer_issue_id)
        .bind("Deterministic worker offer")
        .bind(e2e_user_id)
        .bind(partner_offer_issue_id)
        .bind("Deterministic worker wish")
        .bind(partner_user_id)
        .bind(worker_offer_issue_id)
        .bind("Deterministic partner wish")
        .bind(partner_user_id)
        .bind(partner_offer_issue_id)
        .bind("Deterministic partner offer")
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    tracing::info!("Demo catalogue seeded");
    Ok(())
}
