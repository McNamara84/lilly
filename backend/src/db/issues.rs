#![allow(dead_code)]

use crate::models::series::{Issue, IssueResponse};
use sqlx::MySqlPool;

pub async fn find_issues_by_series(
    pool: &MySqlPool,
    series_id: u32,
    page: u32,
    per_page: u32,
) -> Result<Vec<Issue>, sqlx::Error> {
    let offset = (page.saturating_sub(1)) * per_page;

    sqlx::query_as::<_, Issue>(
        "SELECT id, series_id, issue_number, title, published_at, cycle, \
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
        "SELECT id, series_id, issue_number, title, published_at, cycle, \
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

/// Upsert a single issue (without the normalized relations).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_issue(
    pool: &MySqlPool,
    series_id: u32,
    issue_number: u32,
    title: &str,
    published_at: Option<chrono::NaiveDate>,
    cycle: Option<&str>,
    cover_url: Option<&str>,
    cover_local_path: Option<&str>,
    source_wiki_url: Option<&str>,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO issues (series_id, issue_number, title, published_at, cycle, \
         cover_url, cover_local_path, source_wiki_url) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE title = VALUES(title), \
         published_at = VALUES(published_at), cycle = VALUES(cycle), \
         cover_url = VALUES(cover_url), cover_local_path = VALUES(cover_local_path), \
         source_wiki_url = VALUES(source_wiki_url)",
    )
    .bind(series_id)
    .bind(issue_number)
    .bind(title)
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

/// Find the issue id for a given series + `issue_number`.
pub async fn find_issue_id(
    pool: &MySqlPool,
    series_id: u32,
    issue_number: u32,
) -> Result<Option<u32>, sqlx::Error> {
    let row: Option<(u32,)> =
        sqlx::query_as("SELECT id FROM issues WHERE series_id = ? AND issue_number = ?")
            .bind(series_id)
            .bind(issue_number)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(id,)| id))
}

/// Return all issue numbers already stored for a given series.
pub async fn find_existing_issue_numbers(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<std::collections::HashSet<u32>, sqlx::Error> {
    let rows: Vec<(u32,)> = sqlx::query_as("SELECT issue_number FROM issues WHERE series_id = ?")
        .bind(series_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

// ── Normalized relation helpers ───────────────────────────────────────

/// Get or create a person, returning its id.
async fn get_or_create_person(pool: &MySqlPool, name: &str) -> Result<u32, sqlx::Error> {
    sqlx::query("INSERT IGNORE INTO persons (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await?;
    let row: (u32,) = sqlx::query_as("SELECT id FROM persons WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Get or create a keyword, returning its id.
async fn get_or_create_keyword(pool: &MySqlPool, name: &str) -> Result<u32, sqlx::Error> {
    sqlx::query("INSERT IGNORE INTO keywords (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await?;
    let row: (u32,) = sqlx::query_as("SELECT id FROM keywords WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Get or create a note, returning its id.
async fn get_or_create_note(pool: &MySqlPool, text: &str) -> Result<u32, sqlx::Error> {
    sqlx::query("INSERT IGNORE INTO notes (text) VALUES (?)")
        .bind(text)
        .execute(pool)
        .await?;
    let row: (u32,) = sqlx::query_as("SELECT id FROM notes WHERE text = ?")
        .bind(text)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Link persons to an issue with a given role. Clears previous links for that role first.
pub async fn set_issue_persons(
    pool: &MySqlPool,
    issue_id: u32,
    names: &[String],
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM issue_persons WHERE issue_id = ? AND role = ?")
        .bind(issue_id)
        .bind(role)
        .execute(pool)
        .await?;
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        let person_id = get_or_create_person(pool, trimmed).await?;
        sqlx::query(
            "INSERT IGNORE INTO issue_persons (issue_id, person_id, role) VALUES (?, ?, ?)",
        )
        .bind(issue_id)
        .bind(person_id)
        .bind(role)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Link keywords to an issue. Clears previous links first.
pub async fn set_issue_keywords(
    pool: &MySqlPool,
    issue_id: u32,
    keyword_names: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM issue_keywords WHERE issue_id = ?")
        .bind(issue_id)
        .execute(pool)
        .await?;
    for kw in keyword_names {
        let trimmed = kw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kw_id = get_or_create_keyword(pool, trimmed).await?;
        sqlx::query("INSERT IGNORE INTO issue_keywords (issue_id, keyword_id) VALUES (?, ?)")
            .bind(issue_id)
            .bind(kw_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Link notes to an issue. Clears previous links first.
pub async fn set_issue_notes(
    pool: &MySqlPool,
    issue_id: u32,
    note_texts: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM issue_notes WHERE issue_id = ?")
        .bind(issue_id)
        .execute(pool)
        .await?;
    for text in note_texts {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let note_id = get_or_create_note(pool, trimmed).await?;
        sqlx::query("INSERT IGNORE INTO issue_notes (issue_id, note_id) VALUES (?, ?)")
            .bind(issue_id)
            .bind(note_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Fetch person names for a given issue and role.
pub async fn get_issue_persons(
    pool: &MySqlPool,
    issue_id: u32,
    role: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT p.name FROM persons p \
         JOIN issue_persons ip ON ip.person_id = p.id \
         WHERE ip.issue_id = ? AND ip.role = ? ORDER BY p.name",
    )
    .bind(issue_id)
    .bind(role)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Fetch keyword names for a given issue.
pub async fn get_issue_keywords(
    pool: &MySqlPool,
    issue_id: u32,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT k.name FROM keywords k \
         JOIN issue_keywords ik ON ik.keyword_id = k.id \
         WHERE ik.issue_id = ? ORDER BY k.name",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Fetch note texts for a given issue.
pub async fn get_issue_notes(pool: &MySqlPool, issue_id: u32) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT n.text FROM notes n \
         JOIN issue_notes ino ON ino.note_id = n.id \
         WHERE ino.issue_id = ? ORDER BY n.text",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Build a full `IssueResponse` from an `Issue` row by loading all n:m relations.
pub async fn build_issue_response(
    pool: &MySqlPool,
    issue: &Issue,
) -> Result<IssueResponse, sqlx::Error> {
    let authors = get_issue_persons(pool, issue.id, "author").await?;
    let cover_artists = get_issue_persons(pool, issue.id, "cover_artist").await?;
    let keywords = get_issue_keywords(pool, issue.id).await?;
    let notes = get_issue_notes(pool, issue.id).await?;
    Ok(IssueResponse::from_issue_with_relations(
        issue,
        authors,
        cover_artists,
        keywords,
        notes,
    ))
}

/// Build `IssueResponse` items for a list of issues.
pub async fn build_issue_responses(
    pool: &MySqlPool,
    issues: &[Issue],
) -> Result<Vec<IssueResponse>, sqlx::Error> {
    let mut result = Vec::with_capacity(issues.len());
    for issue in issues {
        result.push(build_issue_response(pool, issue).await?);
    }
    Ok(result)
}
