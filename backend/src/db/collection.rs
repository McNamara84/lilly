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
        if let Some(issue_number) = $params.issue_number {
            q = q.bind(issue_number);
        }
        if let Some(ref condition) = $params.condition {
            q = q.bind(condition.as_str());
        }
        if let Some(ref cmin) = $params.condition_min {
            if let Some(ref cmax) = $params.condition_max {
                q = q.bind(cmin.as_str());
                q = q.bind(cmax.as_str());
            }
        }
        if let Some(ref title) = $params.title {
            if !title.trim().is_empty() {
                q = q.bind(title.trim());
            }
        }
        if let Some(ref author) = $params.author {
            if !author.trim().is_empty() {
                q = q.bind(author.trim());
            }
        }
        if let Some(ref search) = $params.q {
            if !search.trim().is_empty() {
                q = q.bind(search.trim());
                q = q.bind(search.trim());
            }
        }
        q
    }};
}

macro_rules! bind_missing_filters {
    ($query:expr, $params:expr) => {{
        let mut q = $query;
        if let Some(issue_number) = $params.issue_number {
            q = q.bind(issue_number);
        }
        if let Some(ref title) = $params.title {
            if !title.trim().is_empty() {
                q = q.bind(title.trim());
            }
        }
        if let Some(ref author) = $params.author {
            if !author.trim().is_empty() {
                q = q.bind(author.trim());
            }
        }
        if let Some(ref search) = $params.q {
            if !search.trim().is_empty() {
                q = q.bind(search.trim());
                q = q.bind(search.trim());
            }
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
    condition_grade: Option<&str>,
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

pub async fn find_entry_row_by_issue_and_user(
    pool: &MySqlPool,
    issue_id: u32,
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
         WHERE ce.issue_id = ? AND ce.user_id = ?
         LIMIT 1",
    )
    .bind(issue_id)
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

    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));

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
    let offset = u64::from(page.saturating_sub(1))
        .saturating_mul(u64::from(per_page))
        .min(1_000_000);

    let (where_clause, order_clause) = build_filter_clauses(params);

    let sql = format!(
        "SELECT ce.id, ce.user_id, ce.issue_id, ce.copy_number, ce.condition_grade,
                ce.status, ce.notes, ce.created_at, ce.updated_at,
                i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.user_id = ? AND s.active = TRUE {where_clause}
         ORDER BY {order_clause}
         LIMIT ? OFFSET ?"
    );

    let query = sqlx::query_as::<_, CollectionEntryRow>(sqlx::AssertSqlSafe(sql)).bind(user_id);
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
         WHERE ce.user_id = ? AND s.active = TRUE {where_clause}"
    );

    let query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql)).bind(user_id);
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
    params: &CollectionQueryParams,
) -> Result<Vec<MissingIssueRow>, sqlx::Error> {
    let per_page = params.per_page.clamp(1, 100);
    let page = params.page.max(1);
    let offset = u64::from(page.saturating_sub(1))
        .saturating_mul(u64::from(per_page))
        .min(1_000_000);

    let (where_clause, order_clause) = build_missing_filter_clauses(params);
    let sql = format!(
        "SELECT i.id AS issue_id, i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM issues i
         JOIN series s ON i.series_id = s.id
         LEFT JOIN collection_entries ce ON ce.issue_id = i.id AND ce.user_id = ?
         WHERE s.slug = ? AND s.active = TRUE AND ce.id IS NULL {where_clause}
         ORDER BY {order_clause}
         LIMIT ? OFFSET ?"
    );

    let query = sqlx::query_as::<_, MissingIssueRow>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .bind(params.series_slug.as_deref().unwrap_or_default());
    let query = bind_missing_filters!(query, params);
    query.bind(per_page).bind(offset).fetch_all(pool).await
}

pub async fn count_missing_issues(
    pool: &MySqlPool,
    user_id: u32,
    params: &CollectionQueryParams,
) -> Result<u32, sqlx::Error> {
    let (where_clause, _) = build_missing_filter_clauses(params);
    let sql = format!(
        "SELECT COUNT(*)
         FROM issues i
         JOIN series s ON i.series_id = s.id
         LEFT JOIN collection_entries ce ON ce.issue_id = i.id AND ce.user_id = ?
         WHERE s.slug = ? AND s.active = TRUE AND ce.id IS NULL {where_clause}"
    );

    let query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .bind(params.series_slug.as_deref().unwrap_or_default());
    let query = bind_missing_filters!(query, params);
    let count = query.fetch_one(pool).await?;

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
            COUNT(ce.id) AS total_entries,
            COUNT(DISTINCT CASE WHEN ce.status IN ('owned', 'duplicate') THEN ce.issue_id END) AS total_owned,
            COUNT(DISTINCT CASE WHEN ce.status = 'duplicate' THEN ce.issue_id END) AS total_duplicate,
            COUNT(DISTINCT CASE WHEN ce.status = 'wanted' THEN ce.issue_id END) AS total_wanted
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.user_id = ? AND s.active = TRUE",
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
    pub declared_total: Option<u32>,
    pub imported_total: i64,
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
            s.total_issues AS declared_total,
            COUNT(DISTINCT i.id) AS imported_total,
            COUNT(DISTINCT CASE WHEN ce.status IN ('owned', 'duplicate') THEN ce.issue_id END) AS owned_count,
            COUNT(DISTINCT CASE WHEN ce.status = 'duplicate' THEN ce.issue_id END) AS duplicate_count,
            COUNT(DISTINCT CASE WHEN ce.status = 'wanted' THEN ce.issue_id END) AS wanted_count
         FROM series s
         LEFT JOIN issues i ON i.series_id = s.id
         LEFT JOIN collection_entries ce ON ce.issue_id = i.id AND ce.user_id = ?
         WHERE s.active = TRUE
         GROUP BY s.id, s.name, s.slug, s.total_issues
         HAVING COUNT(ce.id) > 0
         ORDER BY owned_count DESC, s.name ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------------------
// Check if issue belongs to an active series
// ---------------------------------------------------------------------------

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

#[allow(dead_code)]
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

    if let Some(ref status) = params.status
        && status != "missing"
    {
        where_parts.push("AND ce.status = ?".to_string());
    }

    if params.issue_number.is_some() {
        where_parts.push("AND i.issue_number = ?".to_string());
    }

    if params.condition.is_some() {
        where_parts.push("AND ce.condition_grade = ?".to_string());
    }

    if params.condition_min.is_some() && params.condition_max.is_some() {
        where_parts.push(
            "AND FIELD(ce.condition_grade, 'Z0','Z1','Z2','Z3','Z4') \
             BETWEEN FIELD(?, 'Z0','Z1','Z2','Z3','Z4') \
             AND FIELD(?, 'Z0','Z1','Z2','Z3','Z4')"
                .to_string(),
        );
    }

    if params
        .title
        .as_ref()
        .is_some_and(|title| !title.trim().is_empty())
    {
        where_parts.push("AND i.title LIKE CONCAT('%', ?, '%')".to_string());
    }

    if params
        .author
        .as_ref()
        .is_some_and(|author| !author.trim().is_empty())
    {
        where_parts.push(
            "AND EXISTS (SELECT 1 FROM issue_persons ip JOIN persons p ON ip.person_id = p.id \
                         WHERE ip.issue_id = i.id AND ip.role = 'author' \
                         AND p.name LIKE CONCAT('%', ?, '%'))"
                .to_string(),
        );
    }

    if params.q.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        where_parts.push(
            "AND (i.title LIKE CONCAT('%', ?, '%') \
             OR EXISTS (SELECT 1 FROM issue_persons ip JOIN persons p ON ip.person_id = p.id \
                        WHERE ip.issue_id = i.id AND ip.role = 'author' \
                        AND p.name LIKE CONCAT('%', ?, '%')))"
                .to_string(),
        );
    }

    let where_clause = where_parts.join(" ");

    let sort_field = match params.sort.as_deref() {
        Some("series") => "s.name",
        Some("title") => "i.title",
        Some("condition") => "FIELD(ce.condition_grade, 'Z0','Z1','Z2','Z3','Z4')",
        Some("author") => {
            "COALESCE((SELECT MIN(p.name) FROM issue_persons ip \
             JOIN persons p ON ip.person_id = p.id \
             WHERE ip.issue_id = i.id AND ip.role = 'author'), '')"
        }
        Some("added") => "ce.created_at",
        _ => "i.issue_number",
    };

    let sort_dir = match params.sort_dir.as_deref() {
        Some("desc") => "DESC",
        _ => "ASC",
    };

    let order_clause =
        format!("{sort_field} {sort_dir}, s.name ASC, i.issue_number ASC, ce.id ASC");

    (where_clause, order_clause)
}

fn build_missing_filter_clauses(params: &CollectionQueryParams) -> (String, String) {
    let mut where_parts = Vec::new();

    if params.issue_number.is_some() {
        where_parts.push("AND i.issue_number = ?".to_string());
    }

    if params
        .title
        .as_ref()
        .is_some_and(|title| !title.trim().is_empty())
    {
        where_parts.push("AND i.title LIKE CONCAT('%', ?, '%')".to_string());
    }

    if params
        .author
        .as_ref()
        .is_some_and(|author| !author.trim().is_empty())
    {
        where_parts.push(
            "AND EXISTS (SELECT 1 FROM issue_persons ip JOIN persons p ON ip.person_id = p.id \
                         WHERE ip.issue_id = i.id AND ip.role = 'author' \
                         AND p.name LIKE CONCAT('%', ?, '%'))"
                .to_string(),
        );
    }

    if params.q.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        where_parts.push(
            "AND (i.title LIKE CONCAT('%', ?, '%') \
             OR EXISTS (SELECT 1 FROM issue_persons ip JOIN persons p ON ip.person_id = p.id \
                        WHERE ip.issue_id = i.id AND ip.role = 'author' \
                        AND p.name LIKE CONCAT('%', ?, '%')))"
                .to_string(),
        );
    }

    let sort_field = match params.sort.as_deref() {
        Some("series") => "s.name",
        Some("title") => "i.title",
        Some("author") => {
            "COALESCE((SELECT MIN(p.name) FROM issue_persons ip \
             JOIN persons p ON ip.person_id = p.id \
             WHERE ip.issue_id = i.id AND ip.role = 'author'), '')"
        }
        _ => "i.issue_number",
    };
    let sort_dir = match params.sort_dir.as_deref() {
        Some("desc") => "DESC",
        _ => "ASC",
    };

    (
        where_parts.join(" "),
        format!("{sort_field} {sort_dir}, s.name ASC, i.issue_number ASC, i.id ASC"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_filters_are_combined_and_sorted_stably() {
        let params = CollectionQueryParams {
            series_slug: Some("maddrax".to_string()),
            status: Some("owned".to_string()),
            issue_number: Some(42),
            condition: Some("Z2".to_string()),
            title: Some("Zukunft".to_string()),
            author: Some("Zybell".to_string()),
            sort: Some("author".to_string()),
            sort_dir: Some("desc".to_string()),
            ..CollectionQueryParams::default()
        };

        let (where_clause, order_clause) = build_filter_clauses(&params);

        assert!(where_clause.contains("s.slug = ?"));
        assert!(where_clause.contains("ce.status = ?"));
        assert!(where_clause.contains("i.issue_number = ?"));
        assert!(where_clause.contains("ce.condition_grade = ?"));
        assert!(where_clause.contains("i.title LIKE"));
        assert!(where_clause.contains("ip.role = 'author'"));
        assert!(order_clause.contains("MIN(p.name)"));
        assert!(order_clause.contains("DESC"));
        assert!(order_clause.ends_with("ce.id ASC"));
    }

    #[test]
    fn collection_sort_fields_map_to_expected_sql() {
        let cases = [
            ("series", "s.name"),
            ("issue_number", "i.issue_number"),
            ("condition", "FIELD(ce.condition_grade"),
            ("title", "i.title"),
            ("author", "MIN(p.name)"),
            ("added", "ce.created_at"),
        ];

        for (sort, expected) in cases {
            let params = CollectionQueryParams {
                sort: Some(sort.to_string()),
                ..CollectionQueryParams::default()
            };
            let (_, order_clause) = build_filter_clauses(&params);
            assert!(
                order_clause.contains(expected),
                "sort {sort} should use {expected}: {order_clause}"
            );
        }
    }

    #[test]
    fn empty_text_filters_do_not_change_query() {
        let params = CollectionQueryParams {
            title: Some("   ".to_string()),
            author: Some(String::new()),
            q: Some("  ".to_string()),
            ..CollectionQueryParams::default()
        };

        let (where_clause, _) = build_filter_clauses(&params);
        assert!(where_clause.is_empty());
    }

    #[test]
    fn missing_filters_support_issue_metadata_but_not_collection_fields() {
        let params = CollectionQueryParams {
            issue_number: Some(7),
            title: Some("Nacht".to_string()),
            author: Some("Dark".to_string()),
            condition: Some("Z1".to_string()),
            sort: Some("title".to_string()),
            sort_dir: Some("desc".to_string()),
            ..CollectionQueryParams::default()
        };

        let (where_clause, order_clause) = build_missing_filter_clauses(&params);
        assert!(where_clause.contains("i.issue_number = ?"));
        assert!(where_clause.contains("i.title LIKE"));
        assert!(where_clause.contains("ip.role = 'author'"));
        assert!(!where_clause.contains("condition_grade"));
        assert!(order_clause.starts_with("i.title DESC"));
    }
}
