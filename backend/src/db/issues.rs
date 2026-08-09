#![allow(dead_code)]

use crate::models::series::{Issue, IssueResponse};
use sqlx::{MySqlConnection, MySqlPool};

/// Complete issue metadata written as one atomic replacement.
pub struct IssueMetadataUpdate<'a> {
    pub series_id: u32,
    pub issue_number: u32,
    pub title: &'a str,
    pub published_at: Option<chrono::NaiveDate>,
    pub part_number: Option<u32>,
    pub part_total: Option<u32>,
    pub cycle: Option<&'a str>,
    pub cover_url: Option<&'a str>,
    pub cover_local_path: Option<&'a str>,
    pub source_key: &'a str,
    pub source_record_id: &'a str,
    pub source_wiki_url: Option<&'a str>,
    pub authors: &'a [String],
    pub cover_artists: &'a [String],
    pub keywords: &'a [String],
    pub notes: &'a [String],
}

pub async fn find_issues_by_series(
    pool: &MySqlPool,
    series_id: u32,
    page: u32,
    per_page: u32,
) -> Result<Vec<Issue>, sqlx::Error> {
    let offset = u64::from(page.saturating_sub(1))
        .saturating_mul(u64::from(per_page))
        .min(1_000_000);

    sqlx::query_as::<_, Issue>(
        "SELECT id, series_id, issue_number, title, published_at, part_number, part_total, cycle, \
         cover_url, cover_local_path, source_key, source_record_id, source_wiki_url, \
         metadata_synced_at, created_at \
         FROM issues WHERE series_id = ? ORDER BY issue_number LIMIT ? OFFSET ?",
    )
    .bind(series_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn find_all_issues_by_series(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<Vec<Issue>, sqlx::Error> {
    sqlx::query_as::<_, Issue>(
        "SELECT id, series_id, issue_number, title, published_at, part_number, part_total, cycle, \
         cover_url, cover_local_path, source_key, source_record_id, source_wiki_url, \
         metadata_synced_at, created_at \
         FROM issues WHERE series_id = ? ORDER BY issue_number",
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
}

pub async fn find_issue_by_id(
    pool: &MySqlPool,
    issue_id: u32,
) -> Result<Option<Issue>, sqlx::Error> {
    sqlx::query_as::<_, Issue>(
        "SELECT id, series_id, issue_number, title, published_at, part_number, part_total, cycle, \
         cover_url, cover_local_path, source_key, source_record_id, source_wiki_url, \
         metadata_synced_at, created_at \
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

/// Replace an issue and all normalized metadata relations atomically.
pub async fn replace_issue_metadata(
    pool: &MySqlPool,
    update: &IssueMetadataUpdate<'_>,
) -> Result<u32, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let issue_id = upsert_issue(&mut transaction, update).await?;
    set_issue_persons(&mut transaction, issue_id, update.authors, "author").await?;
    set_issue_persons(
        &mut transaction,
        issue_id,
        update.cover_artists,
        "cover_artist",
    )
    .await?;
    set_issue_keywords(&mut transaction, issue_id, update.keywords).await?;
    set_issue_notes(&mut transaction, issue_id, update.notes).await?;
    mark_issue_metadata_synced(&mut transaction, issue_id).await?;
    transaction.commit().await?;
    Ok(issue_id)
}

/// Upsert a single issue (without the normalized relations).
async fn upsert_issue(
    connection: &mut MySqlConnection,
    update: &IssueMetadataUpdate<'_>,
) -> Result<u32, sqlx::Error> {
    let source_owner: Option<(u32, u32, u32)> = sqlx::query_as(
        "SELECT id, series_id, issue_number FROM issues \
         WHERE source_key = ? AND source_record_id = ? FOR UPDATE",
    )
    .bind(update.source_key)
    .bind(update.source_record_id)
    .fetch_optional(&mut *connection)
    .await?;
    if let Some((_, owner_series_id, owner_issue_number)) = source_owner
        && (owner_series_id != update.series_id || owner_issue_number != update.issue_number)
    {
        return Err(sqlx::Error::Protocol(format!(
            "Source identity '{}:{}' belongs to series {owner_series_id}, issue {owner_issue_number}",
            update.source_key, update.source_record_id
        )));
    }

    sqlx::query(
        "INSERT INTO issues (series_id, issue_number, title, published_at, part_number, part_total, cycle, \
         cover_url, cover_local_path, source_key, source_record_id, source_wiki_url) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE title = VALUES(title), \
         published_at = VALUES(published_at), part_number = VALUES(part_number), \
         part_total = VALUES(part_total), cycle = VALUES(cycle), \
         cover_url = COALESCE(VALUES(cover_url), cover_url), \
         cover_local_path = COALESCE(VALUES(cover_local_path), cover_local_path), \
         source_key = VALUES(source_key), source_record_id = VALUES(source_record_id), \
         source_wiki_url = VALUES(source_wiki_url)",
    )
    .bind(update.series_id)
    .bind(update.issue_number)
    .bind(update.title)
    .bind(update.published_at)
    .bind(update.part_number)
    .bind(update.part_total)
    .bind(update.cycle)
    .bind(update.cover_url)
    .bind(update.cover_local_path)
    .bind(update.source_key)
    .bind(update.source_record_id)
    .bind(update.source_wiki_url)
    .execute(&mut *connection)
    .await?;

    let row: (u32,) =
        sqlx::query_as("SELECT id FROM issues WHERE series_id = ? AND issue_number = ?")
            .bind(update.series_id)
            .bind(update.issue_number)
            .fetch_one(&mut *connection)
            .await?;
    Ok(row.0)
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

/// Returns all issue numbers that still need a one-time metadata backfill.
pub async fn find_unsynced_issue_numbers(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<Vec<u32>, sqlx::Error> {
    let rows: Vec<(u32,)> = sqlx::query_as(
        "SELECT issue_number FROM issues \
         WHERE series_id = ? AND metadata_synced_at IS NULL \
         ORDER BY issue_number",
    )
    .bind(series_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(number,)| number).collect())
}

/// Marks metadata as fully synchronized after the issue and all relations were persisted.
async fn mark_issue_metadata_synced(
    connection: &mut MySqlConnection,
    issue_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE issues SET metadata_synced_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(issue_id)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

pub async fn mark_issue_checked(pool: &MySqlPool, issue_id: u32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE issues SET metadata_synced_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(issue_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Normalized relation helpers ───────────────────────────────────────

/// Get or create a person, returning its id.
async fn get_or_create_person(
    connection: &mut MySqlConnection,
    name: &str,
) -> Result<u32, sqlx::Error> {
    sqlx::query("INSERT IGNORE INTO persons (name) VALUES (?)")
        .bind(name)
        .execute(&mut *connection)
        .await?;
    let row: (u32,) = sqlx::query_as("SELECT id FROM persons WHERE name = ?")
        .bind(name)
        .fetch_one(&mut *connection)
        .await?;
    Ok(row.0)
}

/// Get or create a keyword, returning its id.
async fn get_or_create_keyword(
    connection: &mut MySqlConnection,
    name: &str,
) -> Result<u32, sqlx::Error> {
    sqlx::query("INSERT IGNORE INTO keywords (name) VALUES (?)")
        .bind(name)
        .execute(&mut *connection)
        .await?;
    let row: (u32,) = sqlx::query_as("SELECT id FROM keywords WHERE name = ?")
        .bind(name)
        .fetch_one(&mut *connection)
        .await?;
    Ok(row.0)
}

/// Get or create a note, returning its id.
async fn get_or_create_note(
    connection: &mut MySqlConnection,
    text: &str,
) -> Result<u32, sqlx::Error> {
    sqlx::query("INSERT IGNORE INTO notes (text) VALUES (?)")
        .bind(text)
        .execute(&mut *connection)
        .await?;
    let row: (u32,) = sqlx::query_as("SELECT id FROM notes WHERE text = ?")
        .bind(text)
        .fetch_one(&mut *connection)
        .await?;
    Ok(row.0)
}

/// Link persons to an issue with a given role. Clears previous links for that role first.
async fn set_issue_persons(
    connection: &mut MySqlConnection,
    issue_id: u32,
    names: &[String],
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM issue_persons WHERE issue_id = ? AND role = ?")
        .bind(issue_id)
        .bind(role)
        .execute(&mut *connection)
        .await?;
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        let person_id = get_or_create_person(connection, trimmed).await?;
        sqlx::query(
            "INSERT IGNORE INTO issue_persons (issue_id, person_id, role) VALUES (?, ?, ?)",
        )
        .bind(issue_id)
        .bind(person_id)
        .bind(role)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

/// Link keywords to an issue. Clears previous links first.
async fn set_issue_keywords(
    connection: &mut MySqlConnection,
    issue_id: u32,
    keyword_names: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM issue_keywords WHERE issue_id = ?")
        .bind(issue_id)
        .execute(&mut *connection)
        .await?;
    for kw in keyword_names {
        let trimmed = kw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kw_id = get_or_create_keyword(connection, trimmed).await?;
        sqlx::query("INSERT IGNORE INTO issue_keywords (issue_id, keyword_id) VALUES (?, ?)")
            .bind(issue_id)
            .bind(kw_id)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

/// Link notes to an issue. Clears previous links first.
async fn set_issue_notes(
    connection: &mut MySqlConnection,
    issue_id: u32,
    note_texts: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM issue_notes WHERE issue_id = ?")
        .bind(issue_id)
        .execute(&mut *connection)
        .await?;
    for text in note_texts {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let note_id = get_or_create_note(connection, trimmed).await?;
        sqlx::query("INSERT IGNORE INTO issue_notes (issue_id, note_id) VALUES (?, ?)")
            .bind(issue_id)
            .bind(note_id)
            .execute(&mut *connection)
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

/// Build `IssueResponse` items for a list of issues using batched queries.
/// Uses 4 queries total (instead of 4 per issue) to load all relations.
pub async fn build_issue_responses(
    pool: &MySqlPool,
    issues: &[Issue],
) -> Result<Vec<IssueResponse>, sqlx::Error> {
    if issues.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<u32> = issues.iter().map(|i| i.id).collect();
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    // Batch-load persons (authors + cover_artists)
    let persons_query = format!(
        "SELECT ip.issue_id, p.name, ip.role FROM persons p \
         JOIN issue_persons ip ON ip.person_id = p.id \
         WHERE ip.issue_id IN ({placeholders}) ORDER BY ip.issue_id, p.name"
    );
    let mut q = sqlx::query_as::<_, (u32, String, String)>(sqlx::AssertSqlSafe(persons_query));
    for id in &ids {
        q = q.bind(id);
    }
    let person_rows = q.fetch_all(pool).await?;

    // Batch-load keywords
    let keywords_query = format!(
        "SELECT ik.issue_id, k.name FROM keywords k \
         JOIN issue_keywords ik ON ik.keyword_id = k.id \
         WHERE ik.issue_id IN ({placeholders}) ORDER BY ik.issue_id, k.name"
    );
    let mut q = sqlx::query_as::<_, (u32, String)>(sqlx::AssertSqlSafe(keywords_query));
    for id in &ids {
        q = q.bind(id);
    }
    let keyword_rows = q.fetch_all(pool).await?;

    // Batch-load notes
    let notes_query = format!(
        "SELECT ino.issue_id, n.text FROM notes n \
         JOIN issue_notes ino ON ino.note_id = n.id \
         WHERE ino.issue_id IN ({placeholders}) ORDER BY ino.issue_id, n.text"
    );
    let mut q = sqlx::query_as::<_, (u32, String)>(sqlx::AssertSqlSafe(notes_query));
    for id in &ids {
        q = q.bind(id);
    }
    let note_rows = q.fetch_all(pool).await?;

    // Group by issue_id
    let mut authors_map: std::collections::HashMap<u32, Vec<String>> =
        std::collections::HashMap::new();
    let mut cover_artists_map: std::collections::HashMap<u32, Vec<String>> =
        std::collections::HashMap::new();
    let mut keywords_map: std::collections::HashMap<u32, Vec<String>> =
        std::collections::HashMap::new();
    let mut notes_map: std::collections::HashMap<u32, Vec<String>> =
        std::collections::HashMap::new();

    for (issue_id, name, role) in &person_rows {
        if role == "author" {
            authors_map.entry(*issue_id).or_default().push(name.clone());
        } else {
            cover_artists_map
                .entry(*issue_id)
                .or_default()
                .push(name.clone());
        }
    }
    for (issue_id, name) in keyword_rows {
        keywords_map.entry(issue_id).or_default().push(name);
    }
    for (issue_id, text) in note_rows {
        notes_map.entry(issue_id).or_default().push(text);
    }

    let result = issues
        .iter()
        .map(|issue| {
            IssueResponse::from_issue_with_relations(
                issue,
                authors_map.remove(&issue.id).unwrap_or_default(),
                cover_artists_map.remove(&issue.id).unwrap_or_default(),
                keywords_map.remove(&issue.id).unwrap_or_default(),
                notes_map.remove(&issue.id).unwrap_or_default(),
            )
        })
        .collect();

    Ok(result)
}
