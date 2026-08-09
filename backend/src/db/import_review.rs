#![allow(dead_code)]

use chrono::{NaiveDate, NaiveDateTime};
use sqlx::{MySql, MySqlPool, QueryBuilder};

use crate::models::import_review::{PublicationEvent, ReviewItem, ReviewOutcomeCounts};

const REVIEW_RESULT_COLUMNS: &str = "id, job_id, issue_id, issue_number, outcome, severity, \
    stage, message, source_key, source_record_id, source_url, title, authors_json, \
    cover_artists_json, published_at, part_number, part_total, cycle, cover_status, \
    cover_reason, cover_local_path, processed_at";

#[derive(Debug, sqlx::FromRow)]
struct ReviewItemRow {
    id: u32,
    job_id: u32,
    issue_id: Option<u32>,
    issue_number: u32,
    outcome: String,
    severity: String,
    stage: Option<String>,
    message: Option<String>,
    source_key: String,
    source_record_id: Option<String>,
    source_url: Option<String>,
    title: Option<String>,
    authors_json: String,
    cover_artists_json: String,
    published_at: Option<NaiveDate>,
    part_number: Option<u32>,
    part_total: Option<u32>,
    cycle: Option<String>,
    cover_status: String,
    cover_reason: Option<String>,
    cover_local_path: Option<String>,
    processed_at: Option<NaiveDateTime>,
}

impl From<ReviewItemRow> for ReviewItem {
    fn from(row: ReviewItemRow) -> Self {
        Self {
            id: row.id,
            job_id: row.job_id,
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            outcome: row.outcome,
            severity: row.severity,
            stage: row.stage,
            message: row.message,
            source_key: row.source_key,
            source_record_id: row.source_record_id,
            source_url: row.source_url,
            title: row.title,
            authors: parse_string_list(&row.authors_json),
            cover_artists: parse_string_list(&row.cover_artists_json),
            published_at: row.published_at,
            part_number: row.part_number,
            part_total: row.part_total,
            cycle: row.cycle,
            cover_status: row.cover_status,
            cover_reason: row.cover_reason,
            cover_local_path: row.cover_local_path,
            processed_at: row.processed_at,
        }
    }
}

fn parse_string_list(value: &str) -> Vec<String> {
    serde_json::from_str(value).unwrap_or_default()
}

pub struct ReviewResultUpdate<'a> {
    pub issue_id: Option<u32>,
    pub issue_number: u32,
    pub outcome: &'a str,
    pub severity: &'a str,
    pub stage: &'a str,
    pub message: Option<&'a str>,
    pub source_record_id: Option<&'a str>,
    pub source_url: Option<&'a str>,
    pub title: Option<&'a str>,
    pub authors: &'a [String],
    pub cover_artists: &'a [String],
    pub published_at: Option<NaiveDate>,
    pub part_number: Option<u32>,
    pub part_total: Option<u32>,
    pub cycle: Option<&'a str>,
    pub cover_status: &'a str,
    pub cover_reason: Option<&'a str>,
    pub cover_local_path: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct ReviewItemFilter {
    pub query: Option<String>,
    pub outcome: Option<String>,
    pub severity: Option<String>,
    pub cover_status: Option<String>,
    pub issue_numbers: Vec<u32>,
}

pub async fn seed_import_results(
    pool: &MySqlPool,
    job_id: u32,
    source_key: &str,
    issue_numbers: &[u32],
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    for issue_number in issue_numbers {
        sqlx::query(
            "INSERT INTO import_job_results \
             (job_id, issue_number, source_key, authors_json, cover_artists_json) \
             VALUES (?, ?, ?, '[]', '[]') \
             ON DUPLICATE KEY UPDATE source_key = VALUES(source_key)",
        )
        .bind(job_id)
        .bind(issue_number)
        .bind(source_key)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await
}

pub async fn record_import_result(
    pool: &MySqlPool,
    job_id: u32,
    source_key: &str,
    result: &ReviewResultUpdate<'_>,
) -> Result<(), sqlx::Error> {
    let authors = serde_json::to_string(result.authors).unwrap_or_else(|_| "[]".to_string());
    let cover_artists =
        serde_json::to_string(result.cover_artists).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        "INSERT INTO import_job_results (job_id, issue_id, issue_number, outcome, severity, stage, \
         message, source_key, source_record_id, source_url, title, authors_json, cover_artists_json, \
         published_at, part_number, part_total, cycle, cover_status, cover_reason, cover_local_path, \
         processed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
         CURRENT_TIMESTAMP) ON DUPLICATE KEY UPDATE issue_id = VALUES(issue_id), \
         outcome = VALUES(outcome), severity = VALUES(severity), stage = VALUES(stage), \
         message = VALUES(message), source_key = VALUES(source_key), \
         source_record_id = VALUES(source_record_id), source_url = VALUES(source_url), \
         title = VALUES(title), authors_json = VALUES(authors_json), \
         cover_artists_json = VALUES(cover_artists_json), published_at = VALUES(published_at), \
         part_number = VALUES(part_number), part_total = VALUES(part_total), cycle = VALUES(cycle), \
         cover_status = VALUES(cover_status), cover_reason = VALUES(cover_reason), \
         cover_local_path = VALUES(cover_local_path), processed_at = CURRENT_TIMESTAMP",
    )
    .bind(job_id)
    .bind(result.issue_id)
    .bind(result.issue_number)
    .bind(result.outcome)
    .bind(result.severity)
    .bind(result.stage)
    .bind(result.message)
    .bind(source_key)
    .bind(result.source_record_id)
    .bind(result.source_url)
    .bind(result.title)
    .bind(authors)
    .bind(cover_artists)
    .bind(result.published_at)
    .bind(result.part_number)
    .bind(result.part_total)
    .bind(result.cycle)
    .bind(result.cover_status)
    .bind(result.cover_reason)
    .bind(result.cover_local_path)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_review_items(
    pool: &MySqlPool,
    job_id: u32,
    filter: &ReviewItemFilter,
    page: u32,
    per_page: u32,
) -> Result<Vec<ReviewItem>, sqlx::Error> {
    let offset = u64::from(page.saturating_sub(1))
        .saturating_mul(u64::from(per_page))
        .min(1_000_000);
    let mut query = QueryBuilder::<MySql>::new("SELECT ");
    query
        .push(REVIEW_RESULT_COLUMNS)
        .push(" FROM import_job_results WHERE job_id = ");
    query.push_bind(job_id);
    push_filters(&mut query, filter);
    query.push(" ORDER BY issue_number ASC LIMIT ");
    query.push_bind(per_page).push(" OFFSET ").push_bind(offset);
    let rows = query
        .build_query_as::<ReviewItemRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(ReviewItem::from).collect())
}

pub async fn count_review_items(
    pool: &MySqlPool,
    job_id: u32,
    filter: &ReviewItemFilter,
) -> Result<u32, sqlx::Error> {
    let mut query =
        QueryBuilder::<MySql>::new("SELECT COUNT(*) FROM import_job_results WHERE job_id = ");
    query.push_bind(job_id);
    push_filters(&mut query, filter);
    let (count,): (i64,) = query.build_query_as().fetch_one(pool).await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

fn push_filters(query: &mut QueryBuilder<MySql>, filter: &ReviewItemFilter) {
    if let Some(outcome) = &filter.outcome {
        query.push(" AND outcome = ").push_bind(outcome);
    }
    if let Some(severity) = &filter.severity {
        query.push(" AND severity = ").push_bind(severity);
    }
    if let Some(cover_status) = &filter.cover_status {
        query.push(" AND cover_status = ").push_bind(cover_status);
    }
    if let Some(search) = filter
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        let pattern = format!("%{search}%");
        query
            .push(" AND (CAST(issue_number AS CHAR) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR title LIKE ")
            .push_bind(pattern.clone())
            .push(" OR authors_json LIKE ")
            .push_bind(pattern.clone())
            .push(" OR source_record_id LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if !filter.issue_numbers.is_empty() {
        query.push(" AND issue_number IN (");
        let mut separated = query.separated(", ");
        for issue_number in &filter.issue_numbers {
            separated.push_bind(issue_number);
        }
        separated.push_unseparated(")");
    }
}

pub async fn review_outcome_counts(
    pool: &MySqlPool,
    job_id: u32,
) -> Result<ReviewOutcomeCounts, sqlx::Error> {
    let row: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(CASE WHEN outcome = 'not_processed' THEN 1 END), \
         COUNT(CASE WHEN outcome = 'created' THEN 1 END), \
         COUNT(CASE WHEN outcome = 'updated' THEN 1 END), \
         COUNT(CASE WHEN outcome = 'unchanged' THEN 1 END), \
         COUNT(CASE WHEN outcome = 'skipped' THEN 1 END), \
         COUNT(CASE WHEN outcome = 'failed' THEN 1 END) \
         FROM import_job_results WHERE job_id = ?",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(ReviewOutcomeCounts {
        total: row.0 as u32,
        not_processed: row.1 as u32,
        created: row.2 as u32,
        updated: row.3 as u32,
        unchanged: row.4 as u32,
        skipped: row.5 as u32,
        failed: row.6 as u32,
    })
}

/// Item-level findings are represented by their result row. Job-level findings
/// (for example a source-list discrepancy) live only in `import_job_errors`.
pub async fn review_risk_counts(pool: &MySqlPool, job_id: u32) -> Result<(u32, u32), sqlx::Error> {
    let row: (i64, i64) = sqlx::query_as(
        "SELECT \
         (SELECT COUNT(*) FROM import_job_results WHERE job_id = ? AND severity = 'warning') + \
         (SELECT COUNT(*) FROM import_job_errors WHERE job_id = ? AND severity = 'warning' \
             AND issue_number IS NULL), \
         (SELECT COUNT(*) FROM import_job_results WHERE job_id = ? AND severity = 'blocking') + \
         (SELECT COUNT(*) FROM import_job_errors WHERE job_id = ? AND severity = 'blocking' \
             AND issue_number IS NULL)",
    )
    .bind(job_id)
    .bind(job_id)
    .bind(job_id)
    .bind(job_id)
    .fetch_one(pool)
    .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok((row.0 as u32, row.1 as u32))
}

pub async fn find_review_items_by_numbers(
    pool: &MySqlPool,
    job_id: u32,
    issue_numbers: &[u32],
) -> Result<Vec<ReviewItem>, sqlx::Error> {
    if issue_numbers.is_empty() {
        return Ok(Vec::new());
    }
    find_review_items(
        pool,
        job_id,
        &ReviewItemFilter {
            issue_numbers: issue_numbers.to_vec(),
            ..ReviewItemFilter::default()
        },
        1,
        issue_numbers.len().try_into().unwrap_or(u32::MAX),
    )
    .await
}

pub async fn range_sample_numbers(pool: &MySqlPool, job_id: u32) -> Result<Vec<u32>, sqlx::Error> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM import_job_results WHERE job_id = ?")
        .bind(job_id)
        .fetch_one(pool)
        .await?;
    if count.0 == 0 {
        return Ok(Vec::new());
    }
    let middle_offset = count.0 / 2;
    let rows: Vec<(u32,)> = sqlx::query_as(
        "(SELECT issue_number FROM import_job_results WHERE job_id = ? ORDER BY issue_number ASC LIMIT 1) \
         UNION (SELECT issue_number FROM import_job_results WHERE job_id = ? ORDER BY issue_number ASC LIMIT 1 OFFSET ?) \
         UNION (SELECT issue_number FROM import_job_results WHERE job_id = ? ORDER BY issue_number DESC LIMIT 1) \
         ORDER BY issue_number",
    )
    .bind(job_id)
    .bind(job_id)
    .bind(middle_offset)
    .bind(job_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(number,)| number).collect())
}

pub async fn latest_import_job_id_for_series(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<Option<u32>, sqlx::Error> {
    let row: (Option<u32>,) = sqlx::query_as("SELECT MAX(id) FROM import_jobs WHERE series_id = ?")
        .bind(series_id)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn find_last_publication_event(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<Option<PublicationEvent>, sqlx::Error> {
    sqlx::query_as::<_, PublicationEvent>(
        "SELECT id, series_id, import_job_id, actor_user_id, action, decision, warning_count, \
         blocking_count, created_at FROM series_publication_events WHERE series_id = ? \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_json_metadata_degrades_to_an_empty_list() {
        assert!(parse_string_list("not-json").is_empty());
        assert_eq!(
            parse_string_list(r#"["Jane Doe"]"#),
            vec!["Jane Doe".to_string()]
        );
    }
}
