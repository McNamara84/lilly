use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{Duration, NaiveDateTime, Utc};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::auth::middleware::AuthUser;
use crate::db::collection_transfer as db;
use crate::error::AppError;
use crate::models::collection::AddCollectionEntryRequest;
use crate::models::collection_transfer::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_ROWS, EXPORT_SECONDS, PREVIEW_SECONDS, ParsedRow, RowResult,
    TransferJob,
};
use crate::routes::AppStateInner;
use crate::services::collection_transfer::parse_document;

#[derive(Debug, Clone)]
pub struct TransferConfig {
    pub path: PathBuf,
    pub max_import_bytes: usize,
    pub max_import_rows: usize,
    pub max_archive_bytes: u64,
    pub max_jobs_per_day: u32,
    active: Arc<Mutex<HashSet<String>>>,
}

impl TransferConfig {
    pub fn from_lookup(get: &impl Fn(&str) -> Option<String>) -> Self {
        fn positive<T: std::str::FromStr + PartialOrd + Default>(
            get: &impl Fn(&str) -> Option<String>,
            key: &str,
            default: T,
        ) -> T {
            let result = get(key).map_or(default, |value| {
                value
                    .parse()
                    .unwrap_or_else(|_| panic!("{key} must be a positive number"))
            });
            assert!(result > T::default(), "{key} must be positive");
            result
        }
        Self {
            active: Arc::default(),
            path: get("DATA_TRANSFER_PATH")
                .unwrap_or_else(|| "/data-transfer".to_string())
                .into(),
            max_import_bytes: positive(get, "DATA_IMPORT_MAX_BYTES", DEFAULT_MAX_BYTES),
            max_import_rows: positive(get, "DATA_IMPORT_MAX_ROWS", DEFAULT_MAX_ROWS),
            max_archive_bytes: positive(get, "DATA_EXPORT_MAX_BYTES", 2_u64 * 1024 * 1024 * 1024),
            max_jobs_per_day: positive(get, "DATA_TRANSFER_MAX_JOBS_PER_DAY", 24),
        }
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            active: Arc::default(),
            path: std::env::temp_dir().join(format!("lilly-transfer-{}", random_id())),
            max_import_bytes: DEFAULT_MAX_BYTES,
            max_import_rows: DEFAULT_MAX_ROWS,
            max_archive_bytes: 2 * 1024 * 1024 * 1024,
            max_jobs_per_day: 100,
        }
    }

    pub fn job_path(&self, id: &str) -> Result<PathBuf, AppError> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AppError::NotFound("Transfer not found".to_string()));
        }
        Ok(self.path.join(id))
    }

    pub async fn prepare(&self) -> std::io::Result<()> {
        private_directory(&self.path).await
    }

    pub(super) fn lease(&self, id: &str) -> FileLease {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id.to_string());
        FileLease {
            active: self.active.clone(),
            id: id.to_string(),
        }
    }

    fn active(&self, id: &str) -> bool {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(id)
    }
}

// Cleanup must not race a detached upload or a ZIP writer. The task owns this
// guard until all file operations finish, even if its HTTP caller disconnects.
pub(super) struct FileLease {
    active: Arc<Mutex<HashSet<String>>>,
    id: String,
}
impl Drop for FileLease {
    fn drop(&mut self) {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.id);
    }
}

pub fn random_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

pub async fn private_directory(path: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(path).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}

pub async fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(path).await?;
    file.write_all(bytes).await?;
    file.sync_all().await
}

pub fn new_job(user_id: u32, kind: &str, format: &str, now: NaiveDateTime) -> TransferJob {
    TransferJob {
        id: random_id(),
        user_id: Some(user_id),
        kind: kind.to_string(),
        format: format.to_string(),
        status: if kind == "import" {
            "prepared"
        } else {
            "queued"
        }
        .to_string(),
        created_at: now,
        completed_at: None,
        snapshot_at: None,
        expires_at: now
            + Duration::seconds(if kind == "import" {
                PREVIEW_SECONDS
            } else {
                EXPORT_SECONDS
            }),
        invalidated_at: None,
        files_deleted: false,
        error_code: None,
        total_rows: 0,
        byte_size: 0,
        source_sha256: None,
    }
}

fn preclassified(row: &ParsedRow) -> Option<RowResult> {
    row.code.as_ref().map(|code| RowResult {
        row_number: row.row_number,
        copy_key: row.entry.as_ref().map(|entry| entry.copy_key.clone()),
        status: match code.as_str() {
            "duplicate_file_row" => "unchanged",
            "duplicate_key_conflict" => "conflict",
            _ => "invalid",
        }
        .to_string(),
        code: Some(code.clone()),
        entry_id: None,
    })
}

pub async fn preview(
    state: &Arc<AppStateInner>,
    user_id: u32,
    bytes: &[u8],
    format: &str,
    now: NaiveDateTime,
) -> Result<TransferJob, AppError> {
    let rows = parse_document(
        bytes,
        format,
        state.transfer_config.max_import_bytes,
        state.transfer_config.max_import_rows,
    )?;
    let mut job = new_job(user_id, "import", format, now);
    job.total_rows = u32::try_from(rows.len()).map_err(|e| AppError::InternalError(e.into()))?;
    job.source_sha256 = Some(hex::encode(Sha256::digest(bytes)));
    let lease = state.transfer_config.lease(&job.id);
    state
        .transfer_config
        .prepare()
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    db::create_job(&state.pool, &job, state.transfer_config.max_jobs_per_day).await?;
    let state = state.clone();
    let bytes = bytes.to_vec();
    let response_job = job.clone();
    tokio::spawn(async move {
        let _lease = lease;
        let result = async {
            let directory = state.transfer_config.job_path(&job.id)?;
            private_directory(&directory)
                .await
                .map_err(|e| AppError::InternalError(e.into()))?;
            write_private(&directory.join("source"), &bytes)
                .await
                .map_err(|e| AppError::InternalError(e.into()))?;
            let mut connection = state.pool.acquire().await?;
            for row in &rows {
                let mut result = if let Some(result) = preclassified(row) {
                    result
                } else if let Some(entry) = &row.entry {
                    db::classify_entry(&mut connection, user_id, entry).await?.0
                } else {
                    unreachable!("invalid parsed rows carry an error code")
                };
                result.row_number = row.row_number;
                db::store_result(&mut connection, &job.id, &result).await?;
            }
            Ok::<_, AppError>(())
        }
        .await;
        if result.is_err() {
            mark_failed(&state.pool, &job.id, "preview_failed").await?;
        }
        result
    })
    .await
    .map_err(|error| AppError::InternalError(error.into()))??;
    Ok(response_job)
}

pub async fn commit_import(
    pool: &sqlx::MySqlPool,
    id: &str,
    user_id: u32,
    now: NaiveDateTime,
) -> Result<TransferJob, AppError> {
    let job = db::owned_job(pool, id, user_id, "import").await?;
    if job.invalidated_at.is_some() || job.expires_at <= now {
        return Err(AppError::ConflictWithCode {
            message: "Import preview has expired; upload the file again".to_string(),
            code: "preview_expired".to_string(),
        });
    }
    sqlx::query("UPDATE transfer_jobs j JOIN users u ON u.id = j.user_id SET j.status = 'queued', j.expires_at = ? WHERE j.id = ? AND j.user_id = ? AND j.status = 'prepared' AND j.invalidated_at IS NULL AND u.account_state = 'active'")
        .bind(now + Duration::seconds(EXPORT_SECONDS)).bind(id).bind(user_id).execute(pool).await?;
    let job = db::owned_job(pool, id, user_id, "import").await?;
    if !matches!(job.status.as_str(), "queued" | "running" | "completed") {
        return Err(AppError::Conflict(
            "This import cannot be started".to_string(),
        ));
    }
    Ok(job)
}

async fn import_row(
    state: &AppStateInner,
    job: &TransferJob,
    row: &ParsedRow,
) -> Result<(), AppError> {
    let user_id = job
        .user_id
        .ok_or_else(|| AppError::Unauthorized("Account is unavailable".to_string()))?;
    let mut tx = state.pool.begin().await?;
    // Keep account/job lock order identical to erasure and export publication.
    let active =
        sqlx::query_scalar::<_, String>("SELECT account_state FROM users WHERE id = ? FOR UPDATE")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM transfer_jobs WHERE id = ? AND invalidated_at IS NULL FOR UPDATE",
    )
    .bind(&job.id)
    .fetch_optional(&mut *tx)
    .await?;
    if active.as_deref() != Some("active") || status.as_deref() != Some("running") {
        return Err(AppError::Conflict("Transfer was cancelled".to_string()));
    }
    let mut result = if let Some(result) = preclassified(row) {
        result
    } else if let Some(entry) = &row.entry {
        let (mut result, _) = db::classify_entry(&mut tx, user_id, entry).await?;
        if result.status == "new" {
            let issue_id: u32 = sqlx::query_scalar("SELECT i.id FROM issues i JOIN series s ON s.id = i.series_id WHERE s.slug = ? AND i.issue_number = ? AND s.active = TRUE")
                    .bind(&entry.series_slug).bind(entry.issue_number).fetch_one(&mut *tx).await?;
            let auth = AuthUser {
                user_id,
                display_name: String::new(),
                role: "user".to_string(),
                auth_time: 0,
                session_version: 0,
            };
            let request = AddCollectionEntryRequest {
                issue_id,
                condition_grade: entry.condition_grade.clone(),
                status: Some(entry.status.clone()),
                notes: entry.notes.clone(),
                copy_number: Some(entry.copy_number),
                edition_label: entry.edition_label.clone(),
            };
            let created = crate::routes::collection::add_to_collection_in_transaction(
                &mut tx, &auth, &request,
            )
            .await?;
            sqlx::query("UPDATE collection_transfer_keys SET copy_key = ? WHERE user_id = ? AND entry_id = ?")
                    .bind(&entry.copy_key).bind(user_id).bind(created.id).execute(&mut *tx).await?;
            result.status = "created".to_string();
            result.entry_id = Some(created.id);
        }
        result
    } else {
        unreachable!("invalid parsed rows carry an error code")
    };
    result.row_number = row.row_number;
    db::store_result(&mut tx, &job.id, &result).await?;
    tx.commit().await?;
    Ok(())
}

async fn run_import(state: &AppStateInner, job: &TransferJob) -> Result<(), AppError> {
    let path = state.transfer_config.job_path(&job.id)?.join("source");
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    if metadata.len() > u64::try_from(state.transfer_config.max_import_bytes).unwrap_or(u64::MAX) {
        return Err(AppError::PayloadTooLarge(
            "Import file is too large".to_string(),
        ));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    if job.source_sha256.as_deref() != Some(&hex::encode(Sha256::digest(&bytes))) {
        return Err(AppError::Conflict("Import file changed".to_string()));
    }
    let rows = parse_document(
        &bytes,
        &job.format,
        state.transfer_config.max_import_bytes,
        state.transfer_config.max_import_rows,
    )?;
    for row in &rows {
        crate::services::transaction_retry::retry_transaction("collection import", || {
            import_row(state, job, row)
        })
        .await?;
    }
    sqlx::query("UPDATE transfer_jobs SET status = 'completed', completed_at = ? WHERE id = ? AND status = 'running' AND invalidated_at IS NULL")
        .bind(Utc::now().naive_utc()).bind(&job.id).execute(&state.pool).await?;
    Ok(())
}

pub async fn mark_failed(pool: &sqlx::MySqlPool, id: &str, code: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE collection_import_results SET status = 'failed', code = 'job_interrupted' WHERE job_id = ? AND status = 'new'")
        .bind(id).execute(pool).await?;
    sqlx::query("UPDATE transfer_jobs SET status = 'failed', error_code = ?, completed_at = CURRENT_TIMESTAMP(6) WHERE id = ? AND status IN ('prepared', 'queued', 'running') AND invalidated_at IS NULL")
        .bind(code).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn process_next(state: &AppStateInner) -> Result<bool, AppError> {
    let mut tx = state.pool.begin().await?;
    let job: Option<TransferJob> = sqlx::query_as("SELECT * FROM transfer_jobs WHERE status = 'queued' AND invalidated_at IS NULL ORDER BY created_at, id LIMIT 1 FOR UPDATE SKIP LOCKED")
        .fetch_optional(&mut *tx).await?;
    let Some(mut job) = job else {
        return Ok(false);
    };
    sqlx::query("UPDATE transfer_jobs SET status = 'running' WHERE id = ?")
        .bind(&job.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    job.status = "running".to_string();
    let _lease = state.transfer_config.lease(&job.id);
    if db::find_job(&state.pool, &job.id)
        .await?
        .is_none_or(|job| job.status != "running" || job.invalidated_at.is_some())
    {
        return Ok(true);
    }
    let result = if job.kind == "import" {
        run_import(state, &job).await
    } else {
        crate::services::data_export::build_export(state, &job).await
    };
    if let Err(error) = result {
        tracing::warn!(job_id = %job.id, "Transfer processing failed");
        mark_failed(
            &state.pool,
            &job.id,
            match error {
                AppError::PayloadTooLarge(_) => "storage_limit",
                _ => "transfer_failed",
            },
        )
        .await?;
    }
    Ok(true)
}

pub async fn cleanup(state: &AppStateInner, now: NaiveDateTime) -> Result<(), AppError> {
    sqlx::query("UPDATE transfer_jobs j LEFT JOIN users u ON u.id = j.user_id SET j.status = 'cancelled', j.invalidated_at = COALESCE(j.invalidated_at, ?) WHERE (u.id IS NULL OR u.account_state <> 'active') AND j.status IN ('prepared', 'queued', 'running', 'ready')")
        .bind(now).execute(&state.pool).await?;
    sqlx::query("UPDATE transfer_jobs SET status = 'expired' WHERE expires_at <= ? AND status IN ('prepared', 'queued', 'ready')")
        .bind(now).execute(&state.pool).await?;
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM transfer_jobs WHERE files_deleted = FALSE AND (invalidated_at IS NOT NULL OR status IN ('failed', 'cancelled', 'expired') OR (kind = 'import' AND status = 'completed'))")
        .fetch_all(&state.pool).await?;
    for id in ids {
        if state.transfer_config.active(&id) {
            continue;
        }
        let path = state.transfer_config.job_path(&id)?;
        match tokio::fs::remove_dir_all(&path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                tracing::warn!(job_id = %id, "Transfer cleanup will be retried");
                continue;
            }
        }
        sqlx::query(
            "UPDATE transfer_jobs SET files_deleted = TRUE, source_sha256 = NULL WHERE id = ?",
        )
        .bind(id)
        .execute(&state.pool)
        .await?;
    }
    sqlx::query("DELETE FROM transfer_jobs WHERE files_deleted = TRUE AND created_at < ?")
        .bind(now - Duration::days(7))
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub async fn recover(state: &AppStateInner) -> Result<(), AppError> {
    state
        .transfer_config
        .prepare()
        .await
        .map_err(|error| AppError::InternalError(error.into()))?;
    sqlx::query("UPDATE transfer_jobs SET status = 'failed', error_code = 'worker_interrupted', completed_at = CURRENT_TIMESTAMP(6) WHERE status = 'running'").execute(&state.pool).await?;
    let jobs: Vec<TransferJob> = sqlx::query_as("SELECT * FROM transfer_jobs WHERE status IN ('ready', 'prepared') AND invalidated_at IS NULL AND expires_at > CURRENT_TIMESTAMP(6)")
        .fetch_all(&state.pool).await?;
    for job in jobs {
        let path = state
            .transfer_config
            .job_path(&job.id)?
            .join(if job.kind == "export" {
                "archive.zip"
            } else {
                "source"
            });
        let available = tokio::fs::metadata(path).await.is_ok_and(|metadata| {
            metadata.is_file() && (job.kind == "import" || metadata.len() == job.byte_size)
        });
        if !available {
            sqlx::query("UPDATE transfer_jobs SET status = 'failed', error_code = 'stored_file_missing', completed_at = COALESCE(completed_at, CURRENT_TIMESTAMP(6)) WHERE id = ? AND status IN ('ready', 'prepared')")
                .bind(job.id).execute(&state.pool).await?;
        }
    }
    sqlx::query("UPDATE collection_import_results r JOIN transfer_jobs j ON j.id = r.job_id SET r.status = 'failed', r.code = 'job_interrupted' WHERE j.status = 'failed' AND r.status = 'new'")
        .execute(&state.pool).await?;
    // A database restore may remove a job whose private files survived in the
    // separate volume. No live writer creates a directory before its DB job.
    let mut directories = tokio::fs::read_dir(&state.transfer_config.path)
        .await
        .map_err(|error| AppError::InternalError(error.into()))?;
    while let Some(directory) = directories
        .next_entry()
        .await
        .map_err(|error| AppError::InternalError(error.into()))?
    {
        let id = directory.file_name().to_string_lossy().into_owned();
        if state.transfer_config.job_path(&id).is_ok()
            && directory
                .file_type()
                .await
                .map_err(|error| AppError::InternalError(error.into()))?
                .is_dir()
            && !state.transfer_config.active(&id)
            && db::find_job(&state.pool, &id).await?.is_none()
        {
            tokio::fs::remove_dir_all(directory.path())
                .await
                .map_err(|error| AppError::InternalError(error.into()))?;
        }
    }
    cleanup(state, Utc::now().naive_utc()).await
}

pub fn spawn_worker(state: Arc<AppStateInner>) {
    tokio::spawn(async move {
        if let Err(error) = recover(&state).await {
            tracing::error!(error = %error, "Transfer recovery failed");
            return;
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            interval.tick().await;
            if cleanup(&state, Utc::now().naive_utc()).await.is_err() {
                tracing::warn!("Transfer cleanup failed");
            }
            if process_next(&state).await.is_err() {
                tracing::warn!("Transfer worker iteration failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_rejects_path_escape_and_config_enforces_positive_limits() {
        let config = TransferConfig::for_tests();
        for id in ["../secret", "", "/tmp/file", &"z".repeat(32)] {
            assert!(config.job_path(id).is_err());
        }
        assert!(
            config
                .job_path(&random_id())
                .unwrap()
                .starts_with(&config.path)
        );
        let config = TransferConfig::from_lookup(&|_| None);
        assert_eq!(config.max_import_rows, 10_000);
        assert_eq!(config.max_import_bytes, 10 * 1024 * 1024);
        assert!(
            std::panic::catch_unwind(|| TransferConfig::from_lookup(&|key| (key
                == "DATA_IMPORT_MAX_ROWS")
                .then(|| "0".to_string())))
            .is_err()
        );
    }

    #[tokio::test]
    async fn private_files_are_created_exclusively() {
        let config = TransferConfig::for_tests();
        config.prepare().await.unwrap();
        let path = config.path.join("source");
        write_private(&path, b"private").await.unwrap();
        assert!(write_private(&path, b"replacement").await.is_err());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"private");
        assert!(config.path.starts_with(std::env::temp_dir()));
        tokio::fs::remove_dir_all(&config.path).await.unwrap();
    }
}
