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
             source_key, source_record_id, source_url, created_at, updated_at \
             FROM series WHERE active = TRUE ORDER BY name",
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, Series>(
            "SELECT id, name, slug, publisher, genre, frequency, total_issues, status, active, \
             source_key, source_record_id, source_url, created_at, updated_at \
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
         source_key, source_record_id, source_url, created_at, updated_at \
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
         source_key, source_record_id, source_url, created_at, updated_at \
         FROM series WHERE id = ?",
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_series_by_source_identity(
    pool: &MySqlPool,
    source_key: &str,
    source_record_id: &str,
) -> Result<Option<Series>, sqlx::Error> {
    sqlx::query_as::<_, Series>(
        "SELECT id, name, slug, publisher, genre, frequency, total_issues, status, active, \
         source_key, source_record_id, source_url, created_at, updated_at \
         FROM series WHERE source_key = ? AND source_record_id = ?",
    )
    .bind(source_key)
    .bind(source_record_id)
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
    source_key: &str,
    source_record_id: &str,
    source_url: &str,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO series (name, slug, publisher, genre, frequency, total_issues, status, \
         source_key, source_record_id, source_url) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(name)
    .bind(slug)
    .bind(publisher)
    .bind(genre)
    .bind(frequency)
    .bind(total_issues)
    .bind(status)
    .bind(source_key)
    .bind(source_record_id)
    .bind(source_url)
    .execute(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

pub async fn bind_series_source_identity(
    pool: &MySqlPool,
    series_id: u32,
    source_key: &str,
    source_record_id: &str,
    source_url: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE series SET source_key = ?, source_record_id = ?, source_url = ? \
         WHERE id = ? AND source_key IS NULL AND source_record_id IS NULL",
    )
    .bind(source_key)
    .bind(source_record_id)
    .bind(source_url)
    .bind(series_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_series_metadata(
    pool: &MySqlPool,
    series_id: u32,
    name: &str,
    publisher: Option<&str>,
    genre: Option<&str>,
    frequency: Option<&str>,
    total_issues: Option<u32>,
    status: &str,
    source_url: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE series SET name = ?, publisher = ?, genre = ?, frequency = ?, total_issues = ?, \
         status = ?, source_url = ? WHERE id = ?",
    )
    .bind(name)
    .bind(publisher)
    .bind(genre)
    .bind(frequency)
    .bind(total_issues)
    .bind(status)
    .bind(source_url)
    .bind(series_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_series_total_issues(
    pool: &MySqlPool,
    series_id: u32,
    total_issues: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE series SET total_issues = ? WHERE id = ?")
        .bind(total_issues)
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
