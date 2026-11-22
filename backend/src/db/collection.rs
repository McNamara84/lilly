use sqlx::{MySqlConnection, MySqlPool};

use crate::models::collection::{
    CollectionEntry, CollectionEntryRow, CollectionQueryParams, CollectionSyncResult,
};

/// Bind filter parameters to any sqlx query type.
macro_rules! bind_filters {
    ($query:expr, $params:expr) => {{
        let mut q = $query;
        if let Some(ref slug) = $params.series_slug {
            q = q.bind(slug.as_str());
        }
        if let Some(issue_id) = $params.issue_id {
            q = q.bind(issue_id);
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
        if let Some(issue_id) = $params.issue_id {
            q = q.bind(issue_id);
        }
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

pub struct NewCollectionEntry<'a> {
    pub user_id: u32,
    pub issue_id: u32,
    pub copy_number: u8,
    pub condition_grade: Option<&'a str>,
    pub status: &'a str,
    pub notes: Option<&'a str>,
    pub edition_label: Option<&'a str>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CollectionMutationReceipt {
    pub operation: String,
    pub request_fingerprint: String,
    pub result_json: sqlx::types::Json<CollectionSyncResult>,
}

#[allow(dead_code)]
pub async fn add_entry(
    pool: &MySqlPool,
    entry: NewCollectionEntry<'_>,
) -> Result<u32, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    add_entry_on_connection(&mut connection, entry).await
}

pub async fn add_entry_on_connection(
    connection: &mut MySqlConnection,
    entry: NewCollectionEntry<'_>,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO collection_entries
            (user_id, issue_id, copy_number, edition_label, condition_grade, status, notes)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.user_id)
    .bind(entry.issue_id)
    .bind(entry.copy_number)
    .bind(entry.edition_label)
    .bind(entry.condition_grade)
    .bind(entry.status)
    .bind(entry.notes)
    .execute(connection)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

/// Allocate the smallest free physical copy number for one user's issue.
/// Locking the user row serializes concurrent allocations even when no entry
/// exists yet and therefore no collection row can be locked.
pub async fn next_copy_number_on_connection(
    connection: &mut MySqlConnection,
    user_id: u32,
    issue_id: u32,
) -> Result<Option<u8>, sqlx::Error> {
    sqlx::query_scalar::<_, u32>("SELECT id FROM users WHERE id = ? FOR UPDATE")
        .bind(user_id)
        .fetch_optional(&mut *connection)
        .await?;
    let occupied = sqlx::query_scalar::<_, u8>(
        "SELECT copy_number FROM collection_entries
         WHERE user_id = ? AND issue_id = ? ORDER BY copy_number FOR UPDATE",
    )
    .bind(user_id)
    .bind(issue_id)
    .fetch_all(connection)
    .await?;
    Ok(smallest_free_copy_number(&occupied))
}

fn smallest_free_copy_number(occupied: &[u8]) -> Option<u8> {
    (1..=u8::MAX).find(|copy_number| !occupied.contains(copy_number))
}

#[allow(dead_code)]
pub async fn find_entry_by_id(
    pool: &MySqlPool,
    entry_id: u32,
) -> Result<Option<CollectionEntry>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntry>(
        "SELECT id, user_id, issue_id, copy_number, edition_label, condition_grade,
                status, notes, revision, created_at, updated_at
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
        "SELECT id, user_id, issue_id, copy_number, edition_label, condition_grade,
                status, notes, revision, created_at, updated_at
         FROM collection_entries WHERE id = ? AND user_id = ?",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_entry_by_id_and_user_on_connection(
    connection: &mut MySqlConnection,
    entry_id: u32,
    user_id: u32,
) -> Result<Option<CollectionEntry>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntry>(
        "SELECT id, user_id, issue_id, copy_number, edition_label, condition_grade,
                status, notes, revision, created_at, updated_at
         FROM collection_entries WHERE id = ? AND user_id = ? FOR UPDATE",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(connection)
    .await
}

#[allow(dead_code)]
pub async fn find_entry_row_by_id_and_user(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
) -> Result<Option<CollectionEntryRow>, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    find_entry_row_by_id_and_user_on_connection(&mut connection, entry_id, user_id).await
}

pub async fn find_entry_row_by_id_and_user_on_connection(
    connection: &mut MySqlConnection,
    entry_id: u32,
    user_id: u32,
) -> Result<Option<CollectionEntryRow>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntryRow>(
        "SELECT ce.id, ce.user_id, ce.issue_id, ce.copy_number, ce.edition_label,
                ce.condition_grade,
                ce.status, ce.notes, ce.revision, ce.created_at, ce.updated_at,
                i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.id = ? AND ce.user_id = ?",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_optional(connection)
    .await
}

pub async fn find_entry_row_by_issue_and_user(
    pool: &MySqlPool,
    issue_id: u32,
    user_id: u32,
) -> Result<Option<CollectionEntryRow>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntryRow>(
        "SELECT ce.id, ce.user_id, ce.issue_id, ce.copy_number, ce.edition_label,
                ce.condition_grade,
                ce.status, ce.notes, ce.revision, ce.created_at, ce.updated_at,
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
#[allow(dead_code)]
pub async fn update_entry(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
    condition_grade: Option<&str>,
    status: Option<&str>,
    notes: Option<Option<&str>>,
    edition_label: Option<Option<&str>>,
) -> Result<bool, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    update_entry_on_connection(
        &mut connection,
        entry_id,
        user_id,
        condition_grade,
        status,
        notes,
        edition_label,
    )
    .await
}

#[allow(clippy::option_option)]
pub async fn update_entry_on_connection(
    connection: &mut MySqlConnection,
    entry_id: u32,
    user_id: u32,
    condition_grade: Option<&str>,
    status: Option<&str>,
    notes: Option<Option<&str>>,
    edition_label: Option<Option<&str>>,
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
    if edition_label.is_some() {
        set_clauses.push("edition_label = ?");
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
    if let Some(label) = edition_label {
        query = query.bind(label);
    }

    query = query.bind(entry_id).bind(user_id);

    let result = query.execute(connection).await?;
    Ok(result.rows_affected() > 0)
}

#[allow(dead_code)]
pub async fn delete_entry(
    pool: &MySqlPool,
    entry_id: u32,
    user_id: u32,
) -> Result<bool, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    delete_entry_on_connection(&mut connection, entry_id, user_id).await
}

pub async fn delete_entry_on_connection(
    connection: &mut MySqlConnection,
    entry_id: u32,
    user_id: u32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM collection_entries WHERE id = ? AND user_id = ?")
        .bind(entry_id)
        .bind(user_id)
        .execute(connection)
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

    let sql = build_collection_page_query(params);
    let query = sqlx::query_as::<_, CollectionEntryRow>(sqlx::AssertSqlSafe(sql)).bind(user_id);
    let query = if params.sort.as_deref() == Some("author") {
        query.bind(user_id)
    } else {
        query
    };
    let query = bind_filters!(query, params);
    query.bind(per_page).bind(offset).fetch_all(pool).await
}

fn build_collection_page_query(params: &CollectionQueryParams) -> String {
    let (where_clause, order_clause) = build_filter_clauses(params);
    let original_sort_field = collection_sort_field(params);
    let candidate_order = order_clause
        .replacen(original_sort_field, "sort_value", 1)
        .replace("s.name", "s.name_rank");
    let result_order = order_clause.replacen(original_sort_field, "page_entries.sort_value", 1);
    // Anchor author-sorted candidates to the user's covering collection index.
    let join_order = if params.sort.as_deref() == Some("author") {
        "STRAIGHT_JOIN "
    } else {
        ""
    };
    // Compute author names once per logical issue instead of once per physical
    // copy. The grouped relation keeps multi-author issues from duplicating rows.
    let (sort_field, author_join) = if params.sort.as_deref() == Some("author") {
        (
            "COALESCE(issue_authors.sort_rank, 0)",
            "LEFT JOIN (
                 SELECT issue_id,
                        DENSE_RANK() OVER (ORDER BY first_author COLLATE utf8mb4_unicode_ci) - 1 AS sort_rank
                 FROM (
                     SELECT collected_issues.issue_id,
                            MIN(p.name COLLATE utf8mb4_unicode_ci) AS first_author
                     FROM (
                         SELECT DISTINCT ce_authors.issue_id
                         FROM collection_entries ce_authors WHERE ce_authors.user_id = ?
                     ) collected_issues
                     JOIN issue_persons ip ON ip.issue_id = collected_issues.issue_id AND ip.role = 'author'
                     JOIN persons p ON p.id = ip.person_id
                     GROUP BY collected_issues.issue_id
                     UNION ALL SELECT NULL, ''
                 ) issue_author_names
             ) issue_authors ON issue_authors.issue_id = i.id",
        )
    } else if params.sort.as_deref() == Some("series") {
        ("s.name_rank", "")
    } else {
        (original_sort_field, "")
    };
    // Collation-equivalent names share an integer rank. Including the empty-name
    // sentinel preserves COALESCE(name, '') ordering for missing/empty authors,
    // while sorting/grouping thousands of copies no longer compares wide strings.

    // Sort only candidate IDs and their sort key. Materializing complete entries
    // (especially notes and cover URLs) before LIMIT makes large collections costly.
    // LIMIT prevents derived-table merging; the outer joins fetch at most one page.
    // Keep that page first: deep offsets can otherwise make MariaDB scan every
    // complete entry before joining back to the selected IDs.
    format!(
        "SELECT STRAIGHT_JOIN ce.id, ce.user_id, ce.issue_id, ce.copy_number, ce.edition_label,
                ce.condition_grade,
                ce.status, ce.notes, ce.revision, ce.created_at, ce.updated_at,
                i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM (
             SELECT {join_order}ce.id AS entry_id, {sort_field} AS sort_value
             FROM collection_entries ce
             JOIN issues i ON ce.issue_id = i.id
             JOIN (
                 SELECT id, slug, active,
                        DENSE_RANK() OVER (ORDER BY name) AS name_rank
                 FROM series
             ) s ON i.series_id = s.id
             {author_join}
             WHERE ce.user_id = ? AND s.active = TRUE {where_clause}
             ORDER BY {candidate_order}
             LIMIT ? OFFSET ?
         ) page_entries
         JOIN collection_entries ce ON ce.id = page_entries.entry_id
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         ORDER BY {result_order}"
    )
}

pub async fn find_all_collection_entries_for_user(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Vec<CollectionEntryRow>, sqlx::Error> {
    sqlx::query_as::<_, CollectionEntryRow>(
        "SELECT ce.id, ce.user_id, ce.issue_id, ce.copy_number, ce.edition_label,
                ce.condition_grade, ce.status, ce.notes, ce.revision,
                ce.created_at, ce.updated_at,
                i.issue_number, i.title, i.cover_url, i.cover_local_path,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.user_id = ? AND s.active = TRUE
         ORDER BY s.name, i.issue_number, ce.copy_number",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn find_mutation_receipt_on_connection(
    connection: &mut MySqlConnection,
    user_id: u32,
    mutation_id: &str,
) -> Result<Option<CollectionMutationReceipt>, sqlx::Error> {
    sqlx::query_as::<_, CollectionMutationReceipt>(
        "SELECT operation, request_fingerprint, result_json
         FROM collection_mutation_receipts
         WHERE user_id = ? AND mutation_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .bind(mutation_id)
    .fetch_optional(connection)
    .await
}

pub async fn insert_mutation_receipt_on_connection(
    connection: &mut MySqlConnection,
    user_id: u32,
    mutation_id: &str,
    operation: &str,
    request_fingerprint: &str,
    result: &CollectionSyncResult,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO collection_mutation_receipts
            (user_id, mutation_id, operation, request_fingerprint, result_json)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(mutation_id)
    .bind(operation)
    .bind(request_fingerprint)
    .bind(sqlx::types::Json(result))
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn update_mutation_receipt_result_on_connection(
    connection: &mut MySqlConnection,
    user_id: u32,
    mutation_id: &str,
    result: &CollectionSyncResult,
) -> Result<bool, sqlx::Error> {
    let update = sqlx::query(
        "UPDATE collection_mutation_receipts
         SET result_json = ?
         WHERE user_id = ? AND mutation_id = ?",
    )
    .bind(sqlx::types::Json(result))
    .bind(user_id)
    .bind(mutation_id)
    .execute(connection)
    .await?;
    Ok(update.rows_affected() == 1)
}

pub async fn count_collection_entries(
    pool: &MySqlPool,
    user_id: u32,
    params: &CollectionQueryParams,
) -> Result<u32, sqlx::Error> {
    let (where_clause, _) = build_filter_clauses(params);

    let sql = if where_clause.is_empty() {
        // Count copies through one covering user-index range before looking up
        // issue metadata. This avoids thousands of point probes during concurrent
        // unfiltered/author-sorted requests, while inactive series stay excluded.
        "SELECT CAST(COALESCE(SUM(owned.copy_count), 0) AS SIGNED) AS cnt
         FROM (
             SELECT ce.issue_id, COUNT(*) AS copy_count
             FROM collection_entries ce WHERE ce.user_id = ?
             GROUP BY ce.issue_id
         ) owned
         JOIN issues i ON i.id = owned.issue_id
         JOIN series s ON s.id = i.series_id
         WHERE s.active = TRUE"
            .to_owned()
    } else {
        format!(
            "SELECT COUNT(*) as cnt
             FROM collection_entries ce
             JOIN issues i ON ce.issue_id = i.id
             JOIN series s ON i.series_id = s.id
             WHERE ce.user_id = ? AND s.active = TRUE {where_clause}"
        )
    };

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
    pub total_physical_owned: i64,
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
            (SELECT COUNT(*) FROM collection_entries physical_ce
             WHERE physical_ce.user_id = ?
               AND physical_ce.status IN ('owned', 'duplicate')) AS total_physical_owned,
            COUNT(DISTINCT CASE WHEN ce.status IN ('owned', 'duplicate') THEN ce.issue_id END) AS total_owned,
            COUNT(DISTINCT CASE WHEN ce.status = 'duplicate' THEN ce.issue_id END) AS total_duplicate,
            COUNT(DISTINCT CASE WHEN ce.status = 'wanted' THEN ce.issue_id END) AS total_wanted
         FROM collection_entries ce
         JOIN issues i ON ce.issue_id = i.id
         JOIN series s ON i.series_id = s.id
         WHERE ce.user_id = ? AND s.active = TRUE",
    )
    .bind(user_id)
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

pub async fn is_issue_in_active_series_on_connection(
    connection: &mut MySqlConnection,
    issue_id: u32,
) -> Result<bool, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM issues i JOIN series s ON i.series_id = s.id
         WHERE i.id = ? AND s.active = TRUE",
    )
    .bind(issue_id)
    .fetch_one(connection)
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

    if params.issue_id.is_some() {
        where_parts.push("AND ce.issue_id = ?".to_string());
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
                         AND p.name COLLATE utf8mb4_unicode_ci LIKE CONCAT('%', ?, '%'))"
                .to_string(),
        );
    }

    if params.q.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        where_parts.push(
            "AND (i.title LIKE CONCAT('%', ?, '%') \
             OR EXISTS (SELECT 1 FROM issue_persons ip JOIN persons p ON ip.person_id = p.id \
                        WHERE ip.issue_id = i.id AND ip.role = 'author' \
                        AND p.name COLLATE utf8mb4_unicode_ci LIKE CONCAT('%', ?, '%')))"
                .to_string(),
        );
    }

    let where_clause = where_parts.join(" ");

    let sort_field = collection_sort_field(params);

    let sort_dir = match params.sort_dir.as_deref() {
        Some("desc") => "DESC",
        _ => "ASC",
    };

    let order_clause =
        format!("{sort_field} {sort_dir}, s.name ASC, i.issue_number ASC, ce.id ASC");

    (where_clause, order_clause)
}

fn collection_sort_field(params: &CollectionQueryParams) -> &'static str {
    match params.sort.as_deref() {
        Some("series") => "s.name",
        Some("title") => "i.title",
        Some("condition") => "FIELD(ce.condition_grade, 'Z0','Z1','Z2','Z3','Z4')",
        Some("author") => {
            "COALESCE((SELECT MIN(p.name COLLATE utf8mb4_unicode_ci) FROM issue_persons ip \
             JOIN persons p ON ip.person_id = p.id \
             WHERE ip.issue_id = i.id AND ip.role = 'author'), '')"
        }
        Some("added") => "ce.created_at",
        _ => "i.issue_number",
    }
}

fn build_missing_filter_clauses(params: &CollectionQueryParams) -> (String, String) {
    let mut where_parts = Vec::new();

    if params.issue_id.is_some() {
        where_parts.push("AND i.id = ?".to_string());
    }

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
                         AND p.name COLLATE utf8mb4_unicode_ci LIKE CONCAT('%', ?, '%'))"
                .to_string(),
        );
    }

    if params.q.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        where_parts.push(
            "AND (i.title LIKE CONCAT('%', ?, '%') \
             OR EXISTS (SELECT 1 FROM issue_persons ip JOIN persons p ON ip.person_id = p.id \
                        WHERE ip.issue_id = i.id AND ip.role = 'author' \
                        AND p.name COLLATE utf8mb4_unicode_ci LIKE CONCAT('%', ?, '%')))"
                .to_string(),
        );
    }

    let sort_field = match params.sort.as_deref() {
        Some("series") => "s.name",
        Some("title") => "i.title",
        Some("author") => {
            "COALESCE((SELECT MIN(p.name COLLATE utf8mb4_unicode_ci) FROM issue_persons ip \
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

    fn author_window_rows(value: &serde_json::Value, rows: &mut Vec<u64>) {
        match value {
            serde_json::Value::Object(object) => {
                if object
                    .get("sort_key")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|key| key.contains("collate utf8mb4_unicode_ci"))
                {
                    rows.push(object["r_output_rows"].as_u64().unwrap());
                }
                for child in object.values() {
                    author_window_rows(child, rows);
                }
            }
            serde_json::Value::Array(children) => {
                for child in children {
                    author_window_rows(child, rows);
                }
            }
            _ => {}
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[allow(clippy::too_many_lines)]
    async fn paged_candidates_preserve_filters_privacy_and_author_ties() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let mut cleanup = crate::db::test_cleanup::DatabaseTestCleanup::new(pool.clone());
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id =
            |result: sqlx::mysql::MySqlQueryResult| u32::try_from(result.last_insert_id()).unwrap();
        let mut users = Vec::new();
        for name in ["owner", "foreign", "empty"] {
            let user = id(
                sqlx::query("INSERT INTO users (email, display_name) VALUES (?, ?)")
                    .bind(format!("paged-{name}-{suffix}@example.test"))
                    .bind(name)
                    .execute(&pool)
                    .await
                    .unwrap(),
            );
            cleanup.defer("DELETE FROM users WHERE id = ?", &[user]);
            users.push(user);
        }
        let mut series = Vec::new();
        for (name, active) in [("A", true), ("Z", true), ("Hidden", false)] {
            let series_id = id(sqlx::query(
                "INSERT INTO series (name, slug, active) VALUES (?, ?, ?)",
            )
            .bind(format!("Paged {name} {suffix}"))
            .bind(format!("paged-{name}-{suffix}"))
            .bind(active)
            .execute(&pool)
            .await
            .unwrap());
            cleanup.defer("DELETE FROM series WHERE id = ?", &[series_id]);
            series.push(series_id);
        }
        let mut issues = Vec::new();
        for (series_id, number, title) in [
            (series[0], 2, "Target"),
            (series[1], 1, "Other"),
            (series[1], 2, "No author"),
            (series[2], 1, "Hidden"),
            (series[0], 9, "Missing"),
        ] {
            issues.push(id(sqlx::query(
                "INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)",
            )
            .bind(series_id)
            .bind(number)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap()));
        }
        let mut people = Vec::new();
        for name in ["Béla", "Anna", "Zoe", "Aardvark"] {
            let person = id(sqlx::query("INSERT INTO persons (name) VALUES (?)")
                .bind(format!("{name} {suffix}"))
                .execute(&pool)
                .await
                .unwrap());
            cleanup.defer("DELETE FROM persons WHERE id = ?", &[person]);
            people.push(person);
        }
        for (issue, person, role) in [
            (issues[0], people[0], "author"),
            (issues[0], people[3], "cover_artist"),
            (issues[1], people[1], "author"),
            (issues[1], people[2], "author"),
            (issues[4], people[1], "author"),
        ] {
            sqlx::query("INSERT INTO issue_persons (issue_id, person_id, role) VALUES (?, ?, ?)")
                .bind(issue)
                .bind(person)
                .bind(role)
                .execute(&pool)
                .await
                .unwrap();
        }
        let note = "Private long note ".repeat(400);
        let mut entries = Vec::new();
        for (issue, copy, condition, status) in [
            (issues[0], 1, Some("Z1"), "owned"),
            (issues[0], 2, Some("Z2"), "owned"),
            (issues[1], 1, None, "wanted"),
            (issues[2], 1, Some("Z4"), "duplicate"),
            (issues[3], 1, Some("Z0"), "owned"),
        ] {
            entries.push(
                add_entry(
                    &pool,
                    NewCollectionEntry {
                        user_id: users[0],
                        issue_id: issue,
                        copy_number: copy,
                        condition_grade: condition,
                        status,
                        notes: Some(&note),
                        edition_label: Some("Edition"),
                    },
                )
                .await
                .unwrap(),
            );
        }
        add_entry(
            &pool,
            NewCollectionEntry {
                user_id: users[1],
                issue_id: issues[0],
                copy_number: 1,
                condition_grade: Some("Z0"),
                status: "owned",
                notes: Some("Foreign secret"),
                edition_label: None,
            },
        )
        .await
        .unwrap();

        let mut params = CollectionQueryParams {
            sort: Some("author".into()),
            page: 1,
            per_page: 2,
            ..Default::default()
        };
        assert_eq!(
            count_collection_entries(&pool, users[0], &params)
                .await
                .unwrap(),
            4
        );
        assert_eq!(
            count_collection_entries(&pool, users[1], &params)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            count_collection_entries(&pool, users[2], &params)
                .await
                .unwrap(),
            0
        );
        let first = find_collection_entries(&pool, users[0], &params)
            .await
            .unwrap();
        assert_eq!(
            first.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![entries[3], entries[2]]
        );
        params.page = 2;
        let second = find_collection_entries(&pool, users[0], &params)
            .await
            .unwrap();
        assert_eq!(
            second.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![entries[0], entries[1]]
        );
        assert!(
            second
                .iter()
                .all(|e| e.user_id == users[0] && e.notes.as_deref() == Some(note.as_str()))
        );
        params.sort_dir = Some("desc".into());
        params.page = 1;
        assert_eq!(
            find_collection_entries(&pool, users[0], &params)
                .await
                .unwrap()
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![entries[0], entries[1]]
        );
        for sort in [
            "series",
            "issue_number",
            "title",
            "condition",
            "author",
            "added",
        ] {
            params.sort = Some(sort.into());
            params.per_page = 500;
            let rows = find_collection_entries(&pool, users[0], &params)
                .await
                .unwrap();
            assert_eq!(
                rows.len(),
                4,
                "sort {sort} must keep copies distinct and hide inactive/foreign entries"
            );
        }
        params = CollectionQueryParams {
            series_slug: Some(format!("paged-A-{suffix}")),
            title: Some("Target".into()),
            author: Some("bela".into()),
            status: Some("owned".into()),
            condition: Some("Z2".into()),
            sort: Some("author".into()),
            page: 1,
            per_page: 50,
            ..Default::default()
        };
        assert_eq!(
            count_collection_entries(&pool, users[0], &params)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            find_collection_entries(&pool, users[0], &params)
                .await
                .unwrap()[0]
                .id,
            entries[1]
        );
        params.page = u32::MAX;
        assert!(
            find_collection_entries(&pool, users[0], &params)
                .await
                .unwrap()
                .is_empty()
        );
        // Equal collation weights and empty author names must retain the same
        // secondary ordering as the original MIN(name)/COALESCE expression.
        let mut extra_entries = Vec::new();
        for (number, author) in [(1, format!("béla {suffix}")), (3, String::new())] {
            let person = id(sqlx::query("INSERT INTO persons (name) VALUES (?)")
                .bind(author)
                .execute(&pool)
                .await
                .unwrap());
            cleanup.defer("DELETE FROM persons WHERE id = ?", &[person]);
            let issue = id(sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, 'Collation target')")
                .bind(series[0]).bind(number).execute(&pool).await.unwrap());
            sqlx::query(
                "INSERT INTO issue_persons (issue_id, person_id, role) VALUES (?, ?, 'author')",
            )
            .bind(issue)
            .bind(person)
            .execute(&pool)
            .await
            .unwrap();
            extra_entries.push(
                add_entry(
                    &pool,
                    NewCollectionEntry {
                        user_id: users[0],
                        issue_id: issue,
                        copy_number: 1,
                        condition_grade: Some("Z1"),
                        status: "owned",
                        notes: None,
                        edition_label: None,
                    },
                )
                .await
                .unwrap(),
            );
        }
        params = CollectionQueryParams {
            sort: Some("author".into()),
            page: 1,
            per_page: 100,
            ..Default::default()
        };
        assert_eq!(
            find_collection_entries(&pool, users[0], &params)
                .await
                .unwrap()
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![
                extra_entries[1],
                entries[3],
                entries[2],
                extra_entries[0],
                entries[0],
                entries[1]
            ]
        );
        params.sort_dir = Some("desc".into());
        assert_eq!(
            find_collection_entries(&pool, users[0], &params)
                .await
                .unwrap()
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![
                extra_entries[0],
                entries[0],
                entries[1],
                entries[2],
                extra_entries[1],
                entries[3]
            ]
        );
        // Series names are unique under the column's collation. Check case/accent
        // ordering between valid names, plus stable copy ties within each series.
        let mut series_ties = Vec::new();
        for (index, name) in ["Équal A", "equal B"].iter().enumerate() {
            let series_id = id(sqlx::query(
                "INSERT INTO series (name, slug, active) VALUES (?, ?, TRUE)",
            )
            .bind(format!("{name} {suffix}"))
            .bind(format!("series-tie-{index}-{suffix}"))
            .execute(&pool)
            .await
            .unwrap());
            cleanup.defer("DELETE FROM series WHERE id = ?", &[series_id]);
            let issue = id(sqlx::query(
                "INSERT INTO issues (series_id, issue_number, title) VALUES (?, 1, 'Series tie')",
            )
            .bind(series_id)
            .execute(&pool)
            .await
            .unwrap());
            for copy_number in 1..=2 {
                series_ties.push(
                    add_entry(
                        &pool,
                        NewCollectionEntry {
                            user_id: users[0],
                            issue_id: issue,
                            copy_number,
                            condition_grade: Some("Z1"),
                            status: "owned",
                            notes: None,
                            edition_label: None,
                        },
                    )
                    .await
                    .unwrap(),
                );
            }
        }
        for sort in ["series", "author", "issue_number"] {
            for direction in ["asc", "desc"] {
                params = CollectionQueryParams {
                    sort: Some(sort.into()),
                    sort_dir: Some(direction.into()),
                    title: Some("Series tie".into()),
                    page: 1,
                    per_page: 1,
                    ..Default::default()
                };
                let expected = if sort == "series" && direction == "desc" {
                    [
                        series_ties[2],
                        series_ties[3],
                        series_ties[0],
                        series_ties[1],
                    ]
                } else {
                    [
                        series_ties[0],
                        series_ties[1],
                        series_ties[2],
                        series_ties[3],
                    ]
                };
                for (page, expected_id) in expected.iter().enumerate() {
                    params.page = u32::try_from(page + 1).unwrap();
                    assert_eq!(
                        find_collection_entries(&pool, users[0], &params)
                            .await
                            .unwrap()[0]
                            .id,
                        *expected_id
                    );
                }
            }
        }
        // Unrelated catalogue people must not enter a small collection's author
        // ranking. Inspect the production query's executed window cardinality.
        let mut unrelated = sqlx::QueryBuilder::<sqlx::MySql>::new("INSERT INTO persons (name) ");
        unrelated.push_values(0..10_000, |mut row, index| {
            row.push_bind(format!("Unrelated {suffix} {index}"));
        });
        let first_unrelated = id(unrelated.build().execute(&pool).await.unwrap());
        cleanup.defer(
            "DELETE FROM persons WHERE id BETWEEN ? AND ?",
            &[first_unrelated, first_unrelated + 9_999],
        );
        params = CollectionQueryParams {
            sort: Some("author".into()),
            page: 1,
            per_page: 50,
            ..Default::default()
        };
        let analyzed = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(format!(
            "ANALYZE FORMAT=JSON {}",
            build_collection_page_query(&params)
        )))
        .bind(users[1])
        .bind(users[1])
        .bind(50_u32)
        .bind(0_u64)
        .fetch_one(&pool)
        .await
        .unwrap();
        let analyzed: serde_json::Value = serde_json::from_str(&analyzed).unwrap();
        let mut ranked_rows = Vec::new();
        author_window_rows(&analyzed, &mut ranked_rows);
        assert_eq!(
            ranked_rows,
            vec![2],
            "one owned issue plus the empty sentinel"
        );
        assert_eq!(
            find_collection_entries(&pool, users[1], &params)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            find_collection_entries(&pool, users[2], &params)
                .await
                .unwrap()
                .is_empty()
        );
        cleanup.finish().await.unwrap();
    }

    #[test]
    fn collection_filters_are_combined_and_sorted_stably() {
        let params = CollectionQueryParams {
            series_slug: Some("maddrax".to_string()),
            issue_id: Some(123),
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
        assert!(where_clause.contains("ce.issue_id = ?"));
        assert!(where_clause.contains("ce.status = ?"));
        assert!(where_clause.contains("i.issue_number = ?"));
        assert!(where_clause.contains("ce.condition_grade = ?"));
        assert!(where_clause.contains("i.title LIKE"));
        assert!(where_clause.contains("ip.role = 'author'"));
        assert!(order_clause.contains("MIN(p.name COLLATE utf8mb4_unicode_ci)"));
        assert!(order_clause.contains("DESC"));
        assert!(order_clause.ends_with("ce.id ASC"));
    }

    #[test]
    fn hostile_input_is_never_interpolated_into_generated_sql() {
        const PAYLOADS: [&str; 5] = [
            "' OR '1'='1",
            "'; DROP TABLE users; --",
            "1 UNION SELECT password_hash FROM users",
            "\" OR 1=1 /*",
            "x'; SLEEP(5); --",
        ];

        for payload in PAYLOADS {
            let value = Some(payload.to_string());
            let params = CollectionQueryParams {
                series_slug: value.clone(),
                status: value.clone(),
                condition: value.clone(),
                condition_min: value.clone(),
                condition_max: value.clone(),
                title: value.clone(),
                author: value.clone(),
                q: value.clone(),
                sort: value.clone(),
                sort_dir: value,
                ..CollectionQueryParams::default()
            };

            let (where_clause, order_clause) = build_filter_clauses(&params);

            for sql in [&where_clause, &order_clause] {
                assert!(!sql.contains(payload), "{payload} leaked into: {sql}");
                assert!(!sql.contains("DROP TABLE"), "{sql}");
            }
            // Unknown sort keys and directions fall back to the fixed defaults.
            assert!(
                order_clause.starts_with("i.issue_number ASC"),
                "{order_clause}"
            );
        }
    }

    #[test]
    fn collection_sort_fields_map_to_expected_sql() {
        let cases = [
            ("series", "s.name"),
            ("issue_number", "i.issue_number"),
            ("condition", "FIELD(ce.condition_grade"),
            ("title", "i.title"),
            ("author", "MIN(p.name COLLATE utf8mb4_unicode_ci)"),
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
        assert_eq!(where_clause, "");
    }

    #[test]
    fn missing_filters_support_issue_metadata_but_not_collection_fields() {
        let params = CollectionQueryParams {
            issue_id: Some(123),
            issue_number: Some(7),
            title: Some("Nacht".to_string()),
            author: Some("Dark".to_string()),
            condition: Some("Z1".to_string()),
            sort: Some("title".to_string()),
            sort_dir: Some("desc".to_string()),
            ..CollectionQueryParams::default()
        };

        let (where_clause, order_clause) = build_missing_filter_clauses(&params);
        assert!(where_clause.contains("i.id = ?"));
        assert!(where_clause.contains("i.issue_number = ?"));
        assert!(where_clause.contains("i.title LIKE"));
        assert!(where_clause.contains("ip.role = 'author'"));
        assert!(!where_clause.contains("condition_grade"));
        assert!(order_clause.starts_with("i.title DESC"));
    }

    #[test]
    fn copy_number_allocation_fills_holes_and_reports_capacity() {
        assert_eq!(smallest_free_copy_number(&[]), Some(1));
        assert_eq!(smallest_free_copy_number(&[1, 3, 4]), Some(2));
        assert_eq!(
            smallest_free_copy_number(&(1..=u8::MAX).collect::<Vec<_>>()),
            None
        );
    }
}
