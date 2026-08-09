#![allow(dead_code)]

use crate::models::series::{ImportJob, ImportJobError};
use sqlx::MySqlPool;

const IMPORT_JOB_COLUMNS: &str = "id, series_id, adapter_name, source_key, trigger_type, scheduled_for, status, \
     total_issues, imported_issues, created_issues, updated_issues, unchanged_issues, \
     skipped_issues, failed_issues, error_message, started_by, started_at, completed_at, \
     created_at, updated_at, cancel_requested_at, retry_of_job_id";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportProgress {
    pub total: u32,
    pub created: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub skipped: u32,
    pub failed: u32,
}

pub struct NewImportJob<'a> {
    pub series_id: u32,
    pub adapter_name: &'a str,
    pub source_key: &'a str,
    pub started_by: Option<u32>,
    pub trigger_type: &'a str,
    pub scheduled_for: Option<chrono::NaiveDateTime>,
    pub retry_of_job_id: Option<u32>,
}

impl ImportProgress {
    #[must_use]
    pub const fn imported(self) -> u32 {
        self.created
            .saturating_add(self.updated)
            .saturating_add(self.unchanged)
    }

    #[must_use]
    pub const fn processed(self) -> u32 {
        self.imported()
            .saturating_add(self.skipped)
            .saturating_add(self.failed)
    }
}

/// Atomically checks for an active import and creates a queued job if none exists.
pub async fn create_import_job_if_idle(
    pool: &MySqlPool,
    job: &NewImportJob<'_>,
) -> Result<Option<u32>, sqlx::Error> {
    let mut transaction = pool.begin().await?;

    sqlx::query("SELECT id FROM series WHERE id = ? FOR UPDATE")
        .bind(job.series_id)
        .execute(&mut *transaction)
        .await?;

    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM import_jobs WHERE series_id = ? AND status IN ('pending', 'running')",
    )
    .bind(job.series_id)
    .fetch_one(&mut *transaction)
    .await?;

    if row.0 > 0 {
        transaction.rollback().await?;
        return Ok(None);
    }

    let result = sqlx::query(
        "INSERT INTO import_jobs (series_id, adapter_name, source_key, trigger_type, scheduled_for, \
         started_by, retry_of_job_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(job.series_id)
    .bind(job.adapter_name)
    .bind(job.source_key)
    .bind(job.trigger_type)
    .bind(job.scheduled_for)
    .bind(job.started_by)
    .bind(job.retry_of_job_id)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(Some(result.last_insert_id() as u32))
}

pub async fn mark_import_running(pool: &MySqlPool, job_id: u32) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE import_jobs SET status = 'running', started_at = COALESCE(started_at, CURRENT_TIMESTAMP) \
         WHERE id = ? AND status = 'pending' AND cancel_requested_at IS NULL",
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn update_import_progress(
    pool: &MySqlPool,
    job_id: u32,
    progress: ImportProgress,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE import_jobs SET total_issues = ?, imported_issues = ?, created_issues = ?, \
         updated_issues = ?, unchanged_issues = ?, skipped_issues = ?, failed_issues = ? \
         WHERE id = ? AND status IN ('pending', 'running')",
    )
    .bind(progress.total)
    .bind(progress.imported())
    .bind(progress.created)
    .bind(progress.updated)
    .bind(progress.unchanged)
    .bind(progress.skipped)
    .bind(progress.failed)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn complete_import_job(
    pool: &MySqlPool,
    job_id: u32,
    progress: ImportProgress,
    error_message: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE import_jobs SET status = IF(? > 0, 'completed_with_errors', 'completed'), \
         total_issues = ?, imported_issues = ?, created_issues = ?, updated_issues = ?, \
         unchanged_issues = ?, skipped_issues = ?, failed_issues = ?, error_message = ?, \
         completed_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND status IN ('pending', 'running') AND cancel_requested_at IS NULL",
    )
    .bind(progress.failed)
    .bind(progress.total)
    .bind(progress.imported())
    .bind(progress.created)
    .bind(progress.updated)
    .bind(progress.unchanged)
    .bind(progress.skipped)
    .bind(progress.failed)
    .bind(error_message)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn fail_import_job(
    pool: &MySqlPool,
    job_id: u32,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE import_jobs SET status = 'failed', error_message = ?, completed_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND status IN ('pending', 'running') AND cancel_requested_at IS NULL",
    )
    .bind(error)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn request_import_cancellation(
    pool: &MySqlPool,
    job_id: u32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE import_jobs SET cancel_requested_at = CURRENT_TIMESTAMP, \
         completed_at = IF(status = 'pending', CURRENT_TIMESTAMP, completed_at), \
         status = IF(status = 'pending', 'cancelled', status) \
         WHERE id = ? AND status IN ('pending', 'running') AND cancel_requested_at IS NULL",
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn is_cancel_requested(pool: &MySqlPool, job_id: u32) -> Result<bool, sqlx::Error> {
    let row: Option<(Option<chrono::NaiveDateTime>, String)> =
        sqlx::query_as("SELECT cancel_requested_at, status FROM import_jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_none_or(|(requested_at, status)| requested_at.is_some() || status == "cancelled"))
}

pub async fn cancel_import_job(pool: &MySqlPool, job_id: u32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE import_jobs SET status = 'cancelled', \
         cancel_requested_at = COALESCE(cancel_requested_at, CURRENT_TIMESTAMP), \
         completed_at = CURRENT_TIMESTAMP WHERE id = ? AND status IN ('pending', 'running')",
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_import_error(
    pool: &MySqlPool,
    job_id: u32,
    source_key: &str,
    issue_number: Option<u32>,
    source_record_id: Option<&str>,
    stage: &str,
    message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO import_job_errors \
         (job_id, source_key, issue_number, source_record_id, stage, message) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(job_id)
    .bind(source_key)
    .bind(issue_number)
    .bind(source_record_id)
    .bind(stage)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_import_errors(
    pool: &MySqlPool,
    job_id: u32,
    page: u32,
    per_page: u32,
) -> Result<Vec<ImportJobError>, sqlx::Error> {
    let offset = u64::from(page.saturating_sub(1))
        .saturating_mul(u64::from(per_page))
        .min(1_000_000);
    sqlx::query_as::<_, ImportJobError>(
        "SELECT id, job_id, source_key, issue_number, source_record_id, stage, message, created_at \
         FROM import_job_errors WHERE job_id = ? ORDER BY id LIMIT ? OFFSET ?",
    )
    .bind(job_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn count_import_errors(pool: &MySqlPool, job_id: u32) -> Result<u32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM import_job_errors WHERE job_id = ?")
        .bind(job_id)
        .fetch_one(pool)
        .await?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(row.0 as u32)
}

pub async fn find_import_jobs_by_series(
    pool: &MySqlPool,
    series_id: u32,
) -> Result<Vec<ImportJob>, sqlx::Error> {
    let query = format!(
        "SELECT {IMPORT_JOB_COLUMNS} FROM import_jobs WHERE series_id = ? ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, ImportJob>(sqlx::AssertSqlSafe(query))
        .bind(series_id)
        .fetch_all(pool)
        .await
}

pub async fn find_import_job_by_id(
    pool: &MySqlPool,
    job_id: u32,
) -> Result<Option<ImportJob>, sqlx::Error> {
    let query = format!("SELECT {IMPORT_JOB_COLUMNS} FROM import_jobs WHERE id = ?");
    sqlx::query_as::<_, ImportJob>(sqlx::AssertSqlSafe(query))
        .bind(job_id)
        .fetch_optional(pool)
        .await
}

pub async fn has_scheduled_job(
    pool: &MySqlPool,
    adapter_name: &str,
    scheduled_for: chrono::NaiveDateTime,
) -> Result<bool, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM import_jobs \
         WHERE adapter_name = ? AND scheduled_for = ? AND trigger_type = 'scheduled'",
    )
    .bind(adapter_name)
    .bind(scheduled_for)
    .fetch_one(pool)
    .await?;
    Ok(row.0 > 0)
}

/// Marks jobs orphaned by a server restart as interrupted and therefore retryable.
pub async fn reconcile_orphaned_jobs(pool: &MySqlPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE import_jobs SET status = 'interrupted', \
         error_message = 'Server restarted during import', completed_at = CURRENT_TIMESTAMP \
         WHERE status IN ('pending', 'running')",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use sqlx::mysql::MySqlPoolOptions;

    use super::*;

    #[test]
    fn progress_counts_are_saturating_and_consistent() {
        let progress = ImportProgress {
            total: 12,
            created: 2,
            updated: 3,
            unchanged: 4,
            skipped: 1,
            failed: 2,
        };
        assert_eq!(progress.imported(), 9);
        assert_eq!(progress.processed(), 12);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn import_job_lifecycle_is_persistent_and_race_safe_against_mariadb() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _database_guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(8)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        crate::db::migrate_test_database(&pool)
            .await
            .expect("test migrations must succeed");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role) VALUES (?, 'Import Tester', 'admin')",
        )
        .bind(format!("import-job-{suffix}@example.test"))
        .execute(&pool)
        .await
        .expect("user fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("user fixture ID must fit u32");
        let series_id: u32 = sqlx::query(
            "INSERT INTO series (name, slug, source_key, source_record_id, source_url) \
             VALUES (?, ?, 'test-wiki', ?, 'https://example.test/series')",
        )
        .bind(format!("Import Test {suffix}"))
        .bind(format!("import-test-{suffix}"))
        .bind(format!("Series:{suffix}"))
        .execute(&pool)
        .await
        .expect("series fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("series fixture ID must fit u32");

        let first_job = create_import_job_if_idle(
            &pool,
            &NewImportJob {
                series_id,
                adapter_name: "test-adapter",
                source_key: "test-wiki",
                started_by: Some(user_id),
                trigger_type: "manual",
                scheduled_for: None,
                retry_of_job_id: None,
            },
        )
        .await
        .expect("first job creation must succeed")
        .expect("first job must be created");
        assert!(mark_import_running(&pool, first_job).await.unwrap());
        assert!(!mark_import_running(&pool, first_job).await.unwrap());

        let progress = ImportProgress {
            total: 5,
            created: 1,
            updated: 1,
            unchanged: 1,
            skipped: 1,
            failed: 1,
        };
        update_import_progress(&pool, first_job, progress)
            .await
            .unwrap();
        record_import_error(
            &pool,
            first_job,
            "test-wiki",
            Some(5),
            Some("Issue:5"),
            "validate",
            "missing author",
        )
        .await
        .unwrap();
        assert_eq!(count_import_errors(&pool, first_job).await.unwrap(), 1);
        let errors = find_import_errors(&pool, first_job, 1, 50).await.unwrap();
        assert_eq!(errors[0].source_record_id.as_deref(), Some("Issue:5"));
        assert!(
            complete_import_job(&pool, first_job, progress, Some("one issue failed"))
                .await
                .unwrap()
        );
        assert_eq!(
            find_import_job_by_id(&pool, first_job)
                .await
                .unwrap()
                .unwrap()
                .status,
            "completed_with_errors"
        );

        let pending_job = create_import_job_if_idle(
            &pool,
            &NewImportJob {
                series_id,
                adapter_name: "test-adapter",
                source_key: "test-wiki",
                started_by: Some(user_id),
                trigger_type: "manual",
                scheduled_for: None,
                retry_of_job_id: Some(first_job),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            request_import_cancellation(&pool, pending_job)
                .await
                .unwrap()
        );
        let pending_job = find_import_job_by_id(&pool, pending_job)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pending_job.status, "cancelled");
        assert!(pending_job.completed_at.is_some());

        let running_job = create_import_job_if_idle(
            &pool,
            &NewImportJob {
                series_id,
                adapter_name: "test-adapter",
                source_key: "test-wiki",
                started_by: Some(user_id),
                trigger_type: "manual",
                scheduled_for: None,
                retry_of_job_id: Some(first_job),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert!(mark_import_running(&pool, running_job).await.unwrap());
        assert!(
            request_import_cancellation(&pool, running_job)
                .await
                .unwrap()
        );
        assert!(is_cancel_requested(&pool, running_job).await.unwrap());
        cancel_import_job(&pool, running_job).await.unwrap();
        assert_eq!(
            find_import_job_by_id(&pool, running_job)
                .await
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );

        let concurrent_job = NewImportJob {
            series_id,
            adapter_name: "test-adapter",
            source_key: "test-wiki",
            started_by: Some(user_id),
            trigger_type: "manual",
            scheduled_for: None,
            retry_of_job_id: Some(first_job),
        };
        let first_creation = create_import_job_if_idle(&pool, &concurrent_job);
        let second_creation = create_import_job_if_idle(&pool, &concurrent_job);
        let (first_creation, second_creation) = tokio::join!(first_creation, second_creation);
        let concurrent_jobs = [first_creation.unwrap(), second_creation.unwrap()];
        assert_eq!(
            concurrent_jobs.iter().filter(|job| job.is_some()).count(),
            1
        );
        let concurrent_job = concurrent_jobs.into_iter().flatten().next().unwrap();
        assert!(mark_import_running(&pool, concurrent_job).await.unwrap());
        let partial_progress = ImportProgress {
            total: 2,
            created: 1,
            ..ImportProgress::default()
        };
        update_import_progress(&pool, concurrent_job, partial_progress)
            .await
            .unwrap();
        assert!(reconcile_orphaned_jobs(&pool).await.unwrap() >= 1);
        let interrupted_job = find_import_job_by_id(&pool, concurrent_job)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(interrupted_job.status, "interrupted");
        assert_eq!(interrupted_job.created_issues, 1);
        assert_eq!(interrupted_job.total_issues, 2);

        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series fixture must be deleted");
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("user fixture must be deleted");
    }
}
