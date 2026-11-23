use chrono::{Duration, NaiveDateTime};
use sqlx::{MySqlConnection, MySqlPool};

use crate::error::AppError;
use crate::models::collection_transfer::{
    JobResponse, ResultCounts, RowResult, TransferEntry, TransferJob,
};
use crate::services::collection_transfer::same_values;

macro_rules! entry_select {
    ($suffix:literal) => {
        concat!(
            "SELECT k.copy_key, s.slug AS series_slug, i.issue_number,
    ce.copy_number, ce.status,
    CASE WHEN i.source_key IS NOT NULL AND i.source_record_id IS NOT NULL THEN i.source_key END AS source_key,
    CASE WHEN i.source_key IS NOT NULL AND i.source_record_id IS NOT NULL THEN i.source_record_id END AS source_record_id, ce.condition_grade,
    ce.edition_label, ce.notes, s.name AS series_name, i.title
    FROM collection_entries ce JOIN issues i ON i.id = ce.issue_id
    JOIN series s ON s.id = i.series_id JOIN collection_transfer_keys k ON k.entry_id = ce.id",
            $suffix
        )
    };
}

pub async fn export_entries(
    connection: &mut MySqlConnection,
    user_id: u32,
) -> Result<Vec<TransferEntry>, sqlx::Error> {
    sqlx::query_as::<_, TransferEntry>(entry_select!(
        " WHERE ce.user_id = ? ORDER BY s.slug, i.issue_number, ce.copy_number"
    ))
    .bind(user_id)
    .fetch_all(connection)
    .await
}

pub async fn find_job(pool: &MySqlPool, id: &str) -> Result<Option<TransferJob>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM transfer_jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn owned_job(
    pool: &MySqlPool,
    id: &str,
    user_id: u32,
    kind: &str,
) -> Result<TransferJob, AppError> {
    find_job(pool, id)
        .await?
        .filter(|job| job.user_id == Some(user_id) && job.kind == kind)
        .ok_or_else(|| AppError::NotFound("Transfer not found".to_string()))
}

pub async fn create_job(
    pool: &MySqlPool,
    job: &TransferJob,
    max_per_day: u32,
) -> Result<(), AppError> {
    crate::services::transaction_retry::retry_transaction("create transfer job", || {
        create_job_once(pool, job, max_per_day)
    })
    .await
}

async fn create_job_once(
    pool: &MySqlPool,
    job: &TransferJob,
    max_per_day: u32,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let active =
        sqlx::query_scalar::<_, String>("SELECT account_state FROM users WHERE id = ? FOR UPDATE")
            .bind(job.user_id)
            .fetch_optional(&mut *tx)
            .await?;
    if active.as_deref() != Some("active") {
        return Err(AppError::Unauthorized("Account is unavailable".to_string()));
    }
    let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer_jobs WHERE user_id = ? AND kind = ? AND status IN ('queued', 'running')")
        .bind(job.user_id).bind(&job.kind).fetch_one(&mut *tx).await?;
    if running > 0 {
        return Err(AppError::ConflictWithCode {
            message: "A transfer is already running".to_string(),
            code: "transfer_active".to_string(),
        });
    }
    let recent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM transfer_jobs WHERE user_id = ? AND created_at > ?",
    )
    .bind(job.user_id)
    .bind(job.created_at - Duration::days(1))
    .fetch_one(&mut *tx)
    .await?;
    if recent >= i64::from(max_per_day) {
        return Err(AppError::TooManyRequests {
            message: "Daily transfer limit reached".to_string(),
            code: "transfer_limit".to_string(),
            retry_after_seconds: 86_400,
        });
    }
    if job.kind == "import" {
        sqlx::query("UPDATE transfer_jobs SET status = 'cancelled', invalidated_at = ? WHERE user_id = ? AND kind = 'import' AND status = 'prepared'")
            .bind(job.created_at).bind(job.user_id).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO transfer_jobs (id, user_id, kind, format, status, created_at, expires_at, total_rows, source_sha256) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&job.id).bind(job.user_id).bind(&job.kind).bind(&job.format).bind(&job.status)
        .bind(job.created_at).bind(job.expires_at).bind(job.total_rows).bind(&job.source_sha256)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn classify_entry(
    connection: &mut MySqlConnection,
    user_id: u32,
    entry: &TransferEntry,
) -> Result<(RowResult, Option<u32>), AppError> {
    let mut result = RowResult {
        row_number: 0,
        copy_key: Some(entry.copy_key.clone()),
        status: "new".to_string(),
        code: None,
        entry_id: None,
    };
    let issue: Option<(u32, bool, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT i.id, s.active, i.source_key, i.source_record_id FROM issues i JOIN series s ON s.id = i.series_id WHERE s.slug = ? AND i.issue_number = ?")
        .bind(&entry.series_slug).bind(entry.issue_number).fetch_optional(&mut *connection).await?;
    let Some((issue_id, active, source_key, source_record_id)) = issue else {
        result.status = "invalid".to_string();
        result.code = Some("issue_not_found".to_string());
        return Ok((result, None));
    };
    if !active
        || entry.source_key.is_some()
            && (entry.source_key != source_key || entry.source_record_id != source_record_id)
    {
        result.status = "invalid".to_string();
        result.code = Some(
            if active {
                "source_identity_mismatch"
            } else {
                "series_inactive"
            }
            .to_string(),
        );
        return Ok((result, None));
    }
    let key: Option<Option<u32>> = sqlx::query_scalar(
        "SELECT entry_id FROM collection_transfer_keys WHERE user_id = ? AND copy_key = ?",
    )
    .bind(user_id)
    .bind(&entry.copy_key)
    .fetch_optional(&mut *connection)
    .await?;
    if let Some(reference) = key {
        result.entry_id = reference;
        let existing = if let Some(id) = reference {
            sqlx::query_as::<_, TransferEntry>(entry_select!(" WHERE ce.user_id = ? AND ce.id = ?"))
                .bind(user_id)
                .bind(id)
                .fetch_optional(&mut *connection)
                .await?
        } else {
            None
        };
        if existing
            .as_ref()
            .is_some_and(|existing| same_values(existing, entry))
        {
            result.status = "unchanged".to_string();
        } else {
            result.status = "conflict".to_string();
            result.code = Some(
                if existing.is_some() {
                    "entry_changed"
                } else {
                    "entry_removed"
                }
                .to_string(),
            );
        }
    } else {
        let occupied: Option<u32> = sqlx::query_scalar("SELECT id FROM collection_entries WHERE user_id = ? AND issue_id = ? AND copy_number = ?")
            .bind(user_id).bind(issue_id).bind(entry.copy_number).fetch_optional(connection).await?;
        if occupied.is_some() {
            result.status = "conflict".to_string();
            result.code = Some("copy_number_occupied".to_string());
        }
    }
    Ok((result, Some(issue_id)))
}

pub async fn store_result(
    connection: &mut MySqlConnection,
    job_id: &str,
    row: &RowResult,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO collection_import_results (job_id, record_number, copy_key, status, code, entry_id) VALUES (?, ?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE copy_key = VALUES(copy_key), status = VALUES(status), code = VALUES(code), entry_id = VALUES(entry_id)")
        .bind(job_id).bind(row.row_number).bind(&row.copy_key).bind(&row.status).bind(&row.code).bind(row.entry_id)
        .execute(connection).await?;
    Ok(())
}

pub async fn response(
    pool: &MySqlPool,
    job: TransferJob,
    page: u32,
    now: NaiveDateTime,
) -> Result<JobResponse, AppError> {
    let counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM collection_import_results WHERE job_id = ? GROUP BY status",
    )
    .bind(&job.id)
    .fetch_all(pool)
    .await?;
    let mut totals = ResultCounts::default();
    for (status, count) in counts {
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        match status.as_str() {
            "new" => totals.new = count,
            "created" => totals.created = count,
            "unchanged" => totals.unchanged = count,
            "conflict" => totals.conflict = count,
            "invalid" => totals.invalid = count,
            "failed" => totals.failed = count,
            _ => {}
        }
    }
    let page = page.clamp(1, 100_000);
    let results = sqlx::query_as("SELECT record_number, copy_key, status, code, entry_id FROM collection_import_results WHERE job_id = ? ORDER BY record_number LIMIT 100 OFFSET ?")
        .bind(&job.id).bind(u64::from(page - 1) * 100).fetch_all(pool).await?;
    let status = if job.invalidated_at.is_some() {
        "cancelled".to_string()
    } else if job.expires_at <= now
        && matches!(job.status.as_str(), "prepared" | "ready" | "queued")
    {
        "expired".to_string()
    } else {
        job.status
    };
    let date = |value: NaiveDateTime| value.and_utc().to_rfc3339();
    Ok(JobResponse {
        id: job.id,
        kind: job.kind,
        format: job.format,
        status,
        created_at: date(job.created_at),
        completed_at: job.completed_at.map(date),
        snapshot_at: job.snapshot_at.map(date),
        expires_at: date(job.expires_at),
        error_code: job.error_code,
        total_rows: job.total_rows,
        counts: totals,
        results,
        page,
        per_page: 100,
        byte_size: job.byte_size,
    })
}

pub async fn invalidate_user(
    connection: &mut MySqlConnection,
    user_id: u32,
    now: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE transfer_jobs j LEFT JOIN data_export_subjects s ON s.job_id = j.id SET j.status = 'cancelled', j.invalidated_at = ?, j.error_code = 'account_data_changed' WHERE (j.user_id = ? OR s.subject_user_id = ?) AND j.status IN ('prepared', 'queued', 'running', 'ready')")
        .bind(now).bind(user_id).bind(user_id).execute(connection).await?;
    Ok(())
}
