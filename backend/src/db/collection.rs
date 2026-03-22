use sqlx::MySqlPool;

use crate::models::collection::{CollectionEntry, CollectionEntryRow, CollectionQueryParams};

/// Bind filter parameters to any sqlx query type.
macro_rules! bind_filters {
    ($query:expr, $params:expr) => {{
        let mut q = $query;
        if let Some(ref slug) = $params.series_slug {
            q = q.bind(slug.as_str());
        }
        if let Some(ref status) = $params.status {
            if status != "missing" {
                q = q.bind(status.as_str());
            }
        }
        if let Some(ref cmin) = $params.condition_min {
            if let Some(ref cmax) = $params.condition_max {
                q = q.bind(cmin.as_str());
                q = q.bind(cmax.as_str());
            }
        }
        if let Some(ref search) = $params.q {
            q = q.bind(search.as_str());
            q = q.bind(search.as_str());
        }
        q
    }};
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

pub async fn add_entry(
    pool: &MySqlPool,
    user_id: u32,
    issue_id: u32,
    copy_number: u8,
    condition_grade: &str,
    status: &str,
    notes: Option<&str>,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO collection_entries (user_id, issue_id, copy_number, condition_grade, status, notes)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(issue_id)
    .bind(copy_number)
    .bind(condition_grade)
    .bind(status)
    .bind(notes)
    .execute(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

#[allow(dead_code)]
pub async fn find_entry_by_id(
    pool: &MySqlPool,
    entry_id: u32,
) -> Result<Option<CollectionEntry>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntry>(
        "SELECT id, user_id, issue_id, copy_number, condition_grade, status, notes, created_at, updated_at
         FROM collection_entries WHERE id = ?",
    )
    .bind(entry_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_entry_by_id_and_user(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
) -> Result<Option<CollectionEntry>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntry>(
        "SELECT id, user_id, issue_id, copy_number, condition_grade, status, notes, created_at, updated_at
         FROM collection_entries WHERE id = ? AND user_id = ?",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_entry_row_by_id_and_user(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
) -> Result<Option<CollectionEntryRow>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntryRow>(
        "SELECT ce.id, ce.user_id, ce.issue_id, ce.copy_number, ce.condition_grade,
                ce.status, ce.notes, ce.created_at, ce.updated_at,
                i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.id = ? AND ce.user_id = ?",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::option_option)]
pub async fn update_entry(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
    condition_grade: Option<&str>,
    status: Option<&str>,
    notes: Option<Option<&str>>,
) -> Result<bool, sqlx::Error> {
    let mut set_clauses = Vec::new();

    if condition_grade.is_some() {
        set_clauses.push("condition_grade = ?");
    }
    if status.is_some() {
        set_clauses.push("status = ?");
    }
    if notes.is_some() {
        set_clauses.push("notes = ?");
    }

    if set_clauses.is_empty() {
        return Ok(false);
    }

    let sql = format!(
        "UPDATE collection_entries SET {} WHERE id = ? AND user_id = ?",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query(&sql);

    if let Some(grade) = condition_grade {
        query = query.bind(grade);
    }
    if let Some(s) = status {
        query = query.bind(s);
    }
    if let Some(n) = notes {
        query = query.bind(n);
    }

    query = query.bind(entry_id).bind(user_id);

    let result = query.execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_entry(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM collection_entries WHERE id = ? AND user_id = ?")
        .bind(entry_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ---------------------------------------------------------------------------
// List with filters, sorting, pagination
// ---------------------------------------------------------------------------

pub async fn find_collection_entries(
    pool: &MySqlPool,
    user_id: u32,
    params: &CollectionQueryParams,
) -> Result<Vec<CollectionEntryRow>, sqlx::Error> {
    let per_page = params.per_page.clamp(1, 100);
    let page = params.page.max(1);
    let offset = (page - 1) * per_page;

    let (where_clause, order_clause) = build_filter_clauses(params);

    let sql = format!(
        "SELECT ce.id, ce.user_id, ce.issue_id, ce.copy_number, ce.condition_grade,
                ce.status, ce.notes, ce.created_at, ce.updated_at,
                i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.user_id = ? {where_clause}
         ORDER BY {order_clause}
         LIMIT ? OFFSET ?"
    );

    let query = sqlx::query_as::<_, CollectionEntryRow>(&sql).bind(user_id);
    let query = bind_filters!(query, params);
    query.bind(per_page).bind(offset).fetch_all(pool).await
}

pub async fn count_collection_entries(
    pool: &MySqlPool,
    user_id: u32,
    params: &CollectionQueryParams,
) -> Result<u32, sqlx::Error> {
    let (where_clause, _) = build_filter_clauses(params);

    let sql = format!(
        "SELECT COUNT(*) as cnt
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.user_id = ? {where_clause}"
    );

    let query = sqlx::query_scalar::<_, i64>(&sql).bind(user_id);
    let query = bind_filters!(query, params);
    let count = query.fetch_one(pool).await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

// ---------------------------------------------------------------------------
// Missing issues (virtual "missing" status)
// ---------------------------------------------------------------------------

pub async fn find_missing_issues(
    pool: &MySqlPool,
    user_id: u32,
    series_slug: &str,
    page: u32,
    per_page: u32,
) -> Result<Vec<MissingIssueRow>, sqlx::Error> {
    let per_page = per_page.clamp(1, 100);
    let page = page.max(1);
    let offset = (page - 1) * per_page;

    sqlx::query_as::<_, MissingIssueRow>(
        "SELECT i.id AS issue_id, i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM issues i
         JOIN series s ON i.series_id = s.id
         LEFT JOIN collection_entries ce ON ce.issue_id = i.id AND ce.user_id = ?
         WHERE s.slug = ? AND s.active = TRUE AND ce.id IS NULL
         ORDER BY i.issue_number ASC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(series_slug)
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn count_missing_issues(
    pool: &MySqlPool,
    user_id: u32,
    series_slug: &str,
) -> Result<u32, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM issues i
         JOIN series s ON i.series_id = s.id
         LEFT JOIN collection_entries ce ON ce.issue_id = i.id AND ce.user_id = ?
         WHERE s.slug = ? AND s.active = TRUE AND ce.id IS NULL",
    )
    .bind(user_id)
    .bind(series_slug)
    .fetch_one(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

#[derive(Debug, sqlx::FromRow)]
pub struct MissingIssueRow {
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code, clippy::struct_field_names)]
pub struct CollectionStatsRow {
    pub total_entries: i64,
    pub total_owned: i64,
    pub total_duplicate: i64,
    pub total_wanted: i64,
}

pub async fn get_collection_stats(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<CollectionStatsRow, sqlx::Error> {
    sqlx::query_as::<_, CollectionStatsRow>(
        "SELECT
            COUNT(*) AS total_entries,
            SUM(CASE WHEN status IN ('owned', 'duplicate') THEN 1 ELSE 0 END) AS total_owned,
            SUM(CASE WHEN status = 'duplicate' THEN 1 ELSE 0 END) AS total_duplicate,
            SUM(CASE WHEN status = 'wanted' THEN 1 ELSE 0 END) AS total_wanted
         FROM collection_entries
         WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

#[derive(Debug, sqlx::FromRow)]
pub struct SeriesStatsRow {
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub total_in_series: i64,
    pub owned_count: i64,
    pub duplicate_count: i64,
    pub wanted_count: i64,
}

pub async fn get_series_stats(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Vec<SeriesStatsRow>, sqlx::Error> {
    sqlx::query_as::<_, SeriesStatsRow>(
        "SELECT
            s.id AS series_id, s.name AS series_name, s.slug AS series_slug,
            (SELECT COUNT(*) FROM issues WHERE series_id = s.id) AS total_in_series,
            COUNT(DISTINCT CASE WHEN ce.status IN ('owned', 'duplicate') THEN ce.issue_id END) AS owned_count,
            COUNT(DISTINCT CASE WHEN ce.status = 'duplicate' THEN ce.issue_id END) AS duplicate_count,
            COUNT(DISTINCT CASE WHEN ce.status = 'wanted' THEN ce.issue_id END) AS wanted_count
         FROM series s
         JOIN collection_entries ce ON ce.issue_id IN (SELECT id FROM issues WHERE series_id = s.id)
         WHERE ce.user_id = ? AND s.active = TRUE
         GROUP BY s.id, s.name, s.slug
         ORDER BY owned_count DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------------------
// Check if issue belongs to an active series
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub async fn is_issue_in_active_series(
    pool: &MySqlPool,
    issue_id: u32,
) -> Result<bool, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM issues i JOIN series s ON i.series_id = s.id
         WHERE i.id = ? AND s.active = TRUE",
    )
    .bind(issue_id)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

pub async fn issue_exists(pool: &MySqlPool, issue_id: u32) -> Result<bool, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM issues WHERE id = ?")
        .bind(issue_id)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

// ---------------------------------------------------------------------------
// Internal helpers: dynamic filter/sort clause builder
// ---------------------------------------------------------------------------

fn build_filter_clauses(params: &CollectionQueryParams) -> (String, String) {
    let mut where_parts = Vec::new();

    if params.series_slug.is_some() {
        where_parts.push("AND s.slug = ?".to_string());
    }

    if let Some(ref status) = params.status {
        if status != "missing" {
            where_parts.push("AND ce.status = ?".to_string());
        }
    }

    if params.condition_min.is_some() && params.condition_max.is_some() {
        where_parts.push(
            "AND FIELD(ce.condition_grade, 'Z0','Z1','Z2','Z3','Z4','Z5') \
             BETWEEN FIELD(?, 'Z0','Z1','Z2','Z3','Z4','Z5') \
             AND FIELD(?, 'Z0','Z1','Z2','Z3','Z4','Z5')"
                .to_string(),
        );
    }

    if params.q.is_some() {
        where_parts.push(
            "AND (i.title LIKE CONCAT('%', ?, '%') \
             OR EXISTS (SELECT 1 FROM issue_persons ip JOIN persons p ON ip.person_id = p.id \
                        WHERE ip.issue_id = i.id AND p.name LIKE CONCAT('%', ?, '%')))"
                .to_string(),
        );
    }

    let where_clause = where_parts.join(" ");

    let sort_field = match params.sort.as_deref() {
        Some("title") => "i.title",
        Some("condition") => "FIELD(ce.condition_grade, 'Z0','Z1','Z2','Z3','Z4','Z5')",
        Some("added") => "ce.created_at",
        _ => "i.issue_number",
    };

    let sort_dir = match params.sort_dir.as_deref() {
        Some("desc") => "DESC",
        _ => "ASC",
    };

    let order_clause = format!("{sort_field} {sort_dir}");

    (where_clause, order_clause)
}
