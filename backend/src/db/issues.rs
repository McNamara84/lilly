#![allow(dead_code)]

use crate::models::series::Issue;
use sqlx::MySqlPool;

pub async fn find_issues_by_series(
    pool: &MySqlPool,
    series_id: u32,
    page: u32,
    per_page: u32,
) -> Result<Vec<Issue>, sqlx::Error> {
    let offset = (page.saturating_sub(1)) * per_page;

    sqlx::query_as::<_, Issue>(
        "SELECT id, series_id, issue_number, title, author, published_at, cycle, \
         cover_url, cover_local_path, source_wiki_url, created_at \
         FROM issues WHERE series_id = ? ORDER BY issue_number LIMIT ? OFFSET ?",
    )
    .bind(series_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn find_issue_by_id(
    pool: &MySqlPool,
    issue_id: u32,
) -> Result<Option<Issue>, sqlx::Error> {
    sqlx::query_as::<_, Issue>(
        "SELECT id, series_id, issue_number, title, author, published_at, cycle, \
         cover_url, cover_local_path, source_wiki_url, created_at \
         FROM issues WHERE id = ?",
    )
    .bind(issue_id)
    .fetch_optional(pool)
    .await
}

pub async fn count_issues_by_series(pool: &MySqlPool, series_id: u32) -> Result<u32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues WHERE series_id = ?")
        .bind(series_id)
        .fetch_one(pool)
        .await?;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(row.0 as u32)
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_issue(
    pool: &MySqlPool,
    series_id: u32,
    issue_number: u32,
    title: &str,
    author: Option<&str>,
    published_at: Option<chrono::NaiveDate>,
    cycle: Option<&str>,
    cover_url: Option<&str>,
    cover_local_path: Option<&str>,
    source_wiki_url: Option<&str>,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO issues (series_id, issue_number, title, author, published_at, cycle, \
         cover_url, cover_local_path, source_wiki_url) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE title = VALUES(title), author = VALUES(author), \
         published_at = VALUES(published_at), cycle = VALUES(cycle), \
         cover_url = VALUES(cover_url), cover_local_path = VALUES(cover_local_path), \
         source_wiki_url = VALUES(source_wiki_url)",
    )
    .bind(series_id)
    .bind(issue_number)
    .bind(title)
    .bind(author)
    .bind(published_at)
    .bind(cycle)
    .bind(cover_url)
    .bind(cover_local_path)
    .bind(source_wiki_url)
    .execute(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}
