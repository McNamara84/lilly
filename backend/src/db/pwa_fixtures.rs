//! Deterministic, private PWA/performance accounts, enabled only by the E2E demo seed.
use sqlx::MySqlPool;

pub async fn seed(
    pool: &MySqlPool,
    workers: u16,
    media_path: &std::path::Path,
) -> Result<(), anyhow::Error> {
    let covers = media_path.join("covers/e2e-pwa");
    tokio::fs::create_dir_all(&covers).await?;
    let hash = crate::auth::password::hash_password("e2e-pwa-password")
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let mut tx = pool.begin().await?;
    let mut accounts = vec!["e2e-performance@lilly.app".to_string()];
    for browser in ["chromium", "firefox", "webkit", "mobile-chrome"] {
        for index in 0..workers {
            accounts.push(format!("e2e-pwa-{browser}-{index}@lilly.app"));
        }
    }
    for email in &accounts {
        sqlx::query("INSERT INTO users (email, password_hash, display_name, role, email_verified) VALUES (?, ?, 'PWA Test', 'user', TRUE) ON DUPLICATE KEY UPDATE password_hash = VALUES(password_hash), email_verified = TRUE")
            .bind(email).bind(&hash).execute(&mut *tx).await?;
    }
    let performance_user: u32 = sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(&accounts[0])
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM collection_entries WHERE user_id = ?")
        .bind(performance_user)
        .execute(&mut *tx)
        .await?;
    for (slug, name) in [
        ("maddrax", "Maddrax – Die dunkle Zukunft der Erde"),
        ("john-sinclair", "Geisterjäger John Sinclair"),
    ] {
        sqlx::query("INSERT INTO series (name, slug, total_issues, status, active) VALUES (?, ?, 100, 'running', TRUE) ON DUPLICATE KEY UPDATE active = TRUE")
            .bind(name).bind(slug).execute(&mut *tx).await?;
        let series_id: u32 = sqlx::query_scalar("SELECT id FROM series WHERE slug = ?")
            .bind(slug)
            .fetch_one(&mut *tx)
            .await?;
        for number in 1..=100 {
            // Distinct, deterministic local JPEGs exercise image loading without wiki traffic.
            let image = image::RgbImage::from_fn(320, 480, |x, y| {
                image::Rgb([
                    (x + number * 13).to_le_bytes()[0],
                    (y + number * 7).to_le_bytes()[0],
                    (x ^ y ^ number).to_le_bytes()[0],
                ])
            });
            let filename = format!("{slug}-{number}.jpg");
            image.save(covers.join(&filename))?;
            sqlx::query(
                "INSERT IGNORE INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)",
            )
            .bind(series_id)
            .bind(number)
            .bind(format!("{name} – Testheft {number}"))
            .execute(&mut *tx)
            .await?;
            let issue_id: u32 = sqlx::query_scalar(
                "SELECT id FROM issues WHERE series_id = ? AND issue_number = ?",
            )
            .bind(series_id)
            .bind(number)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("UPDATE issues SET cover_url = NULL, cover_local_path = ? WHERE id = ?")
                .bind(format!("/media/covers/e2e-pwa/{filename}"))
                .bind(issue_id)
                .execute(&mut *tx)
                .await?;
            let status = match number % 5 {
                0 => "wanted",
                1 => "duplicate",
                _ => "owned",
            };
            sqlx::query("INSERT INTO collection_entries (user_id, issue_id, copy_number, condition_grade, status, notes) VALUES (?, ?, 1, ?, ?, 'Deterministic performance fixture')")
                .bind(performance_user).bind(issue_id).bind(format!("Z{}", number % 5)).bind(status).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE series SET total_issues = (SELECT COUNT(*) FROM issues WHERE series_id = ?) WHERE id = ?")
            .bind(series_id).bind(series_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn performance_fixture_is_repeatable_and_contains_both_series() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::MySqlPool::connect(&url).await.unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let media = std::env::temp_dir().join(format!("lilly-pwa-test-{unique}"));
        super::seed(&pool, 1, &media).await.unwrap();
        super::seed(&pool, 1, &media).await.unwrap();
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT COUNT(*), COUNT(DISTINCT i.series_id), COUNT(DISTINCT ce.status) FROM collection_entries ce JOIN issues i ON i.id = ce.issue_id JOIN users u ON u.id = ce.user_id WHERE u.email = 'e2e-performance@lilly.app'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (200, 2, 3));
        assert_eq!(
            std::fs::read_dir(media.join("covers/e2e-pwa"))
                .unwrap()
                .count(),
            200
        );
        std::fs::remove_dir_all(&media).unwrap();
        sqlx::query("DELETE FROM users WHERE email = 'e2e-performance@lilly.app' OR email LIKE 'e2e-pwa-%@lilly.app'").execute(&pool).await.unwrap();
    }
}
