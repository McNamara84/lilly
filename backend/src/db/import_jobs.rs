#![allow(dead_code)]

use crate::models::series::ImportJob;
use sqlx::MySqlPool;

pub async fn create_import_job(
    pool: &MySqlPool,
    series_id: u32,
    adapter_name: &str,
    started_by: u32,
) -> Result<u32, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO import_jobs (series_id, adapter_name, started_by) VALUES (?, ?, ?)",
    )
    .bind(series_id)
    .bind(adapter_name)
    .bind(started_by)
    .execute(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(result.last_insert_id() as u32)
}

/// Atomically checks for active imports and creates a new job if none exist.
/// Uses a transaction with `SELECT ... FOR UPDATE` on the series row to prevent
/// race conditions between concurrent import requests.
/// Returns `Ok(Some(job_id))` if the job was created, `Ok(None)` if an active import exists.
pub async fn create_import_job_if_idle(
    pool: &MySqlPool,
    series_id: u32,
    adapter_name: &str,
    started_by: u32,
) -> Result<Option<u32>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Lock the series row to serialize concurrent import attempts
    sqlx::query("SELECT id FROM series WHERE id = ? FOR UPDATE")
        .bind(series_id)
        .execute(&mut *tx)
        .await?;

    // Check for active imports within the transaction
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM import_jobs WHERE series_id = ? AND status IN ('pending', 'running')",
    )
    .bind(series_id)
    .fetch_one(&mut *tx)
    .await?;

    if row.0 > 0 {
        tx.rollback().await?;
        return Ok(None);
    }

    let result = sqlx::query(
        "INSERT INTO import_jobs (series_id, adapter_name, started_by) VALUES (?, ?, ?)",
    )
    .bind(series_id)
    .bind(adapter_name)
    .bind(started_by)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(Some(result.last_insert_id() as u32))
}

pub async fn update_import_progress(
    pool: &MySqlPool,
    job_id: u32,
    imported_issues: u32,
    total_issues: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE import_jobs SET imported_issues = ?, total_issues = ?, status = 'running', \
         started_at = COALESCE(started_at, CURRENT_TIMESTAMP) WHERE id = ?",
    )
    .bind(imported_issues)
    .bind(total_issues)
    .bind(job_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn complete_import_job(pool: &MySqlPool, job_id: u32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE import_jobs SET status = 'completed', completed_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(job_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn fail_import_job(
    pool: &MySqlPool,
    job_id: u32,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE import_jobs SET status = 'failed', error_message = ?, \
         completed_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(error)
    .bind(job_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn find_import_jobs_by_series(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<Vec<ImportJob>, sqlx::Error> {
    sqlx::query_as::<_, ImportJob>(
        "SELECT id, series_id, adapter_name, status, total_issues, imported_issues, \
         error_message, started_by, started_at, completed_at, created_at \
         FROM import_jobs WHERE series_id = ? ORDER BY created_at DESC",
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
}

pub async fn has_active_import_for_series(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<bool, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM import_jobs WHERE series_id = ? AND status IN ('pending', 'running')",
    )
    .bind(series_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 > 0)
}

pub async fn find_import_job_by_id(
    pool: &MySqlPool,
    job_id: u32,
) -> Result<Option<ImportJob>, sqlx::Error> {
    sqlx::query_as::<_, ImportJob>(
        "SELECT id, series_id, adapter_name, status, total_issues, imported_issues, \
         error_message, started_by, started_at, completed_at, created_at \
         FROM import_jobs WHERE id = ?",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
}

/// Marks any import jobs left in 'pending' or 'running' status as 'failed'.
/// Should be called at server startup to reconcile orphaned jobs from previous runs.
pub async fn reconcile_orphaned_jobs(pool: &MySqlPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE import_jobs SET status = 'failed', \
         error_message = 'Server restarted during import', \
         completed_at = CURRENT_TIMESTAMP \
         WHERE status IN ('pending', 'running')",
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
