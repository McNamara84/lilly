#![allow(dead_code)]

use crate::models::series::Series;
use sqlx::MySqlPool;

pub async fn find_all_series(
    pool: &MySqlPool,
    active_only: bool,
) -> Result<Vec<Series>, sqlx::Error> {
    if active_only {
        sqlx::query_as::<_, Series>(
            "SELECT id, name, slug, publisher, genre, frequency, total_issues, status, active, \
             source_url, created_at, updated_at \
             FROM series WHERE active = TRUE ORDER BY name",
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, Series>(
            "SELECT id, name, slug, publisher, genre, frequency, total_issues, status, active, \
             source_url, created_at, updated_at \
             FROM series ORDER BY name",
        )
        .fetch_all(pool)
        .await
    }
}

pub async fn find_series_by_slug(
    pool: &MySqlPool,
    slug: &str,
) -> Result<Option<Series>, sqlx::Error> {
    sqlx::query_as::<_, Series>(
        "SELECT id, name, slug, publisher, genre, frequency, total_issues, status, active, \
         source_url, created_at, updated_at \
         FROM series WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
}

pub async fn find_series_by_id(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<Option<Series>, sqlx::Error> {
    sqlx::query_as::<_, Series>(
        "SELECT id, name, slug, publisher, genre, frequency, total_issues, status, active, \
         source_url, created_at, updated_at \
         FROM series WHERE id = ?",
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_series(
    pool: &MySqlPool,
    name: &str,
    slug: &str,
    publisher: Option<&str>,
    genre: Option<&str>,
    frequency: Option<&str>,
    total_issues: Option<u32>,
    status: &str,
    source_url: Option<&str>,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO series (name, slug, publisher, genre, frequency, total_issues, status, source_url) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(name)
    .bind(slug)
    .bind(publisher)
    .bind(genre)
    .bind(frequency)
    .bind(total_issues)
    .bind(status)
    .bind(source_url)
    .execute(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

pub async fn update_series_import_status(
    pool: &MySqlPool,
    series_id: u32,
    total_issues: u32,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE series SET total_issues = ?, status = ? WHERE id = ?")
        .bind(total_issues)
        .bind(status)
        .bind(series_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn set_series_active(
    pool: &MySqlPool,
    series_id: u32,
    active: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE series SET active = ? WHERE id = ?")
        .bind(active)
        .bind(series_id)
        .execute(pool)
        .await?;

    Ok(())
}
