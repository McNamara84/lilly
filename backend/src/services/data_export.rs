use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, MySqlConnection};

use crate::db::collection_transfer as db;
use crate::error::AppError;
use crate::models::collection_transfer::{EXPORT_SECONDS, TransferJob};
use crate::routes::AppStateInner;
use crate::services::collection_transfer::encode_document;
use crate::services::data_transfer::{private_directory, write_private};

// Every projection is an explicit field allowlist. Never serialize SELECT * rows.
const DATASETS: &[(&str, &str)] = &[
    (
        "collection_metadata",
        "SELECT JSON_OBJECT('id', id, 'issue_id', issue_id, 'revision', revision, 'created_at', created_at, 'updated_at', updated_at) FROM collection_entries WHERE user_id = ? ORDER BY id",
    ),
    (
        "publication_activity",
        "SELECT JSON_OBJECT('series_id', series_id, 'import_job_id', import_job_id, 'action', action, 'decision', decision, 'warning_count', warning_count, 'blocking_count', blocking_count, 'created_at', created_at) FROM series_publication_events WHERE actor_user_id = ? ORDER BY id",
    ),
    (
        "account",
        "SELECT JSON_OBJECT('id', id, 'email', email, 'display_name', display_name, 'role', role, 'email_verified', email_verified, 'location', location, 'profile_public', profile_public, 'collection_public', collection_public, 'created_at', created_at, 'privacy_consent_at', privacy_consent_at) FROM users WHERE id = ?",
    ),
    (
        "consents",
        "SELECT JSON_OBJECT('policy_version', policy_version, 'consented_at', consented_at, 'registration_method', registration_method, 'created_at', created_at) FROM privacy_consents WHERE user_id = ? ORDER BY id",
    ),
    (
        "oauth_identities",
        "SELECT JSON_OBJECT('provider', provider, 'provider_subject', provider_subject, 'created_at', created_at, 'last_login_at', last_login_at) FROM oauth_identities WHERE user_id = ? ORDER BY id",
    ),
    (
        "sessions",
        "SELECT JSON_OBJECT('created_at', created_at, 'authenticated_at', authenticated_at, 'expires_at', expires_at, 'revoked', revoked) FROM refresh_tokens WHERE user_id = ? ORDER BY id",
    ),
    (
        "copy_keys",
        "SELECT JSON_OBJECT('copy_key', copy_key, 'entry_id', entry_id, 'created_at', created_at) FROM collection_transfer_keys WHERE user_id = ? ORDER BY id",
    ),
    (
        "offline_receipts",
        "SELECT JSON_OBJECT('mutation_id', mutation_id, 'operation', operation, 'result', JSON_EXTRACT(result_json, '$'), 'created_at', created_at) FROM collection_mutation_receipts WHERE user_id = ? ORDER BY created_at, mutation_id",
    ),
    (
        "photos",
        "SELECT JSON_OBJECT('id', p.id, 'entry_id', p.entry_id, 'media_type', p.media_type, 'byte_size', p.byte_size, 'width', p.width, 'height', p.height, 'sort_order', p.sort_order, 'created_at', p.created_at, 'archive_path', CONCAT('files/photos/', p.id, '.jpg')) FROM collection_photos p JOIN collection_entries ce ON ce.id = p.entry_id WHERE ce.user_id = ? ORDER BY p.id",
    ),
    (
        "matches",
        "SELECT JSON_OBJECT('id', m.id, 'status', m.status, 'revision', m.revision, 'detected_at', m.detected_at, 'changed_at', m.changed_at, 'stale_at', m.stale_at, 'partner_id', CASE WHEN p.account_state = 'active' THEN p.id END, 'partner_display_name', CASE WHEN p.account_state = 'active' THEN p.display_name ELSE 'Gelöschtes Konto' END) FROM trade_matches m LEFT JOIN users p ON p.id = CASE WHEN m.user_low_id = ? THEN m.user_high_id ELSE m.user_low_id END WHERE m.user_low_id = ? OR m.user_high_id = ? ORDER BY m.id",
    ),
    (
        "match_items",
        "SELECT JSON_OBJECT('match_id', mi.match_id, 'issue_id', mi.issue_id, 'is_offering', mi.offered_by_user_id = ?, 'own_entry_id', ce.id, 'condition_grade', ce.condition_grade, 'edition_label', ce.edition_label) \
         FROM trade_match_items mi JOIN trade_matches m ON m.id = mi.match_id \
         JOIN collection_entries ce \
           ON ce.id = CASE WHEN mi.offered_by_user_id = ? THEN mi.offer_entry_id ELSE mi.wanted_entry_id END \
          AND ce.user_id = ? \
         WHERE m.user_low_id = ? OR m.user_high_id = ? ORDER BY mi.id",
    ),
    (
        "trades",
        "SELECT JSON_OBJECT('id', t.id, 'match_id', t.match_id, 'status', t.status, 'is_initiator', t.initiator_id = ?, 'cancellation_reason', t.cancellation_reason, 'proposed_at', t.proposed_at, 'accepted_at', t.accepted_at, 'cancelled_at', t.cancelled_at, 'completed_at', t.completed_at, 'created_at', t.created_at, 'updated_at', t.updated_at, 'partner_id', CASE WHEN p.account_state = 'active' THEN p.id END, 'partner_display_name', CASE WHEN p.account_state = 'active' THEN p.display_name ELSE 'Gelöschtes Konto' END) FROM trades t LEFT JOIN users p ON p.id = CASE WHEN t.initiator_id = ? THEN t.responder_id ELSE t.initiator_id END WHERE t.initiator_id = ? OR t.responder_id = ? ORDER BY t.id",
    ),
    (
        "trade_items",
        "SELECT JSON_OBJECT('trade_id', ti.trade_id, 'issue_id', ti.issue_id, 'is_outgoing', ti.offered_by_user_id = ?, 'copy_number', ti.copy_number_snapshot, 'condition_grade', ti.condition_grade_snapshot, 'edition_label', ti.edition_label_snapshot, 'created_at', ti.created_at) FROM trade_items ti JOIN trades t ON t.id = ti.trade_id WHERE t.initiator_id = ? OR t.responder_id = ? ORDER BY ti.id",
    ),
    (
        "confirmations",
        "SELECT JSON_OBJECT('trade_id', c.trade_id, 'is_mine', c.user_id = ?, 'confirmed_at', c.confirmed_at) FROM trade_completion_confirmations c JOIN trades t ON t.id = c.trade_id WHERE t.initiator_id = ? OR t.responder_id = ? ORDER BY c.trade_id, c.user_id",
    ),
    (
        "threads",
        "SELECT JSON_OBJECT('id', mt.id, 'trade_id', mt.trade_id, 'created_at', mt.created_at, 'updated_at', mt.updated_at) FROM message_threads mt JOIN trades t ON t.id = mt.trade_id WHERE t.initiator_id = ? OR t.responder_id = ? ORDER BY mt.id",
    ),
    (
        "messages",
        "SELECT JSON_OBJECT('id', m.id, 'thread_id', m.thread_id, 'is_mine', m.sender_id = ?, 'content', m.content, 'created_at', m.created_at, 'read_at', m.read_at) FROM messages m JOIN message_threads mt ON mt.id = m.thread_id JOIN trades t ON t.id = mt.trade_id WHERE t.initiator_id = ? OR t.responder_id = ? ORDER BY m.thread_id, m.id",
    ),
    (
        "notifications",
        "SELECT JSON_OBJECT('id', id, 'kind', kind, 'match_id', match_id, 'trade_id', trade_id, 'message_id', message_id, 'payload', JSON_EXTRACT(payload, '$'), 'read_at', read_at, 'created_at', created_at) FROM notifications WHERE user_id = ? ORDER BY id",
    ),
    (
        "role_changes",
        "SELECT JSON_OBJECT('previous_role', previous_role, 'new_role', new_role, 'method', method, 'created_at', created_at) FROM role_change_events WHERE target_user_id = ? ORDER BY id",
    ),
    (
        "wiki_import_activity",
        "SELECT JSON_OBJECT('id', id, 'series_id', series_id, 'adapter_name', adapter_name, 'status', status, 'started_at', started_at, 'completed_at', completed_at, 'created_at', created_at, 'cancel_requested_at', cancel_requested_at) FROM import_jobs WHERE started_by = ? OR cancel_requested_by = ? ORDER BY id",
    ),
    (
        "transfer_jobs",
        "SELECT JSON_OBJECT('id', id, 'kind', kind, 'format', format, 'status', status, 'created_at', created_at, 'completed_at', completed_at, 'snapshot_at', snapshot_at, 'expires_at', expires_at, 'error_code', error_code, 'total_rows', total_rows, 'byte_size', byte_size) FROM transfer_jobs WHERE user_id = ? ORDER BY created_at, id",
    ),
    (
        "import_results",
        "SELECT JSON_OBJECT('job_id', r.job_id, 'row_number', r.record_number, 'copy_key', r.copy_key, 'status', r.status, 'code', r.code, 'entry_id', r.entry_id) FROM collection_import_results r JOIN transfer_jobs j ON j.id = r.job_id WHERE j.user_id = ? ORDER BY r.job_id, r.record_number",
    ),
    (
        "account_deletion",
        "SELECT JSON_OBJECT('status', status, 'requested_at', requested_at, 'scheduled_for', scheduled_for, 'completed_at', completed_at) FROM account_erasure_jobs WHERE user_id = ? ORDER BY id",
    ),
];

async fn json_rows(
    connection: &mut MySqlConnection,
    sql: &'static str,
    user_id: u32,
) -> Result<Vec<Value>, AppError> {
    let mut query = sqlx::query_scalar::<_, String>(sql);
    // Only compile-time allowlisted statements reach this helper; values stay bound.
    for _ in 0..sql.bytes().filter(|byte| *byte == b'?').count() {
        query = query.bind(user_id);
    }
    let rows = query.fetch_all(connection).await?;
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(|e| AppError::InternalError(e.into())))
        .collect()
}

#[derive(sqlx::FromRow)]
struct Subject {
    id: u32,
    session_version: u32,
    account_state: String,
}

pub async fn lock_valid_subjects(
    connection: &mut MySqlConnection,
    job_id: &str,
) -> Result<(), AppError> {
    let subjects: Vec<(Option<u32>, u32, String)> = sqlx::query_as("SELECT subject_user_id, session_version, account_state FROM data_export_subjects WHERE job_id = ? ORDER BY subject_user_id")
        .bind(job_id).fetch_all(&mut *connection).await?;
    if subjects.is_empty() {
        return Err(AppError::Conflict(
            "Export snapshot is unavailable".to_string(),
        ));
    }
    for (user_id, version, state) in subjects {
        let current: Option<(u32, String)> = sqlx::query_as(
            "SELECT session_version, account_state FROM users WHERE id = ? FOR UPDATE",
        )
        .bind(user_id)
        .fetch_optional(&mut *connection)
        .await?;
        if current.as_ref() != Some(&(version, state)) {
            return Err(AppError::ConflictWithCode {
                message: "Account data changed; request a new export".to_string(),
                code: "export_invalidated".to_string(),
            });
        }
    }
    Ok(())
}

pub async fn build_export(state: &AppStateInner, job: &TransferJob) -> Result<(), AppError> {
    for attempt in 0..3 {
        match build_export_once(state, job).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                let current = db::find_job(&state.pool, &job.id).await?;
                if current.as_ref().is_some_and(|job| job.status == "ready") {
                    return Ok(());
                }
                let retryable = matches!(&error, AppError::InternalError(inner) if inner.chain().any(|source| source.downcast_ref::<std::io::Error>().is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)))
                    || crate::services::transaction_retry::is_retryable_transaction_error(&error);
                if attempt == 2
                    || !retryable
                    || current
                        .as_ref()
                        .is_none_or(|job| job.status != "running" || job.invalidated_at.is_some())
                {
                    return Err(error);
                }
                let root = state.transfer_config.job_path(&job.id)?;
                match tokio::fs::remove_dir_all(&root).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(AppError::InternalError(error.into())),
                }
                sqlx::query("DELETE FROM data_export_subjects WHERE job_id = ?")
                    .bind(&job.id)
                    .execute(&state.pool)
                    .await?;
            }
        }
    }
    unreachable!("the final snapshot attempt always returns")
}

// Keep snapshot collection and atomic publication visible as one audited lifecycle.
#[allow(clippy::too_many_lines)]
async fn build_export_once(state: &AppStateInner, job: &TransferJob) -> Result<(), AppError> {
    let user_id = job
        .user_id
        .ok_or_else(|| AppError::Unauthorized("Account is unavailable".to_string()))?;
    let root = state.transfer_config.job_path(&job.id)?;
    private_directory(&root)
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    let snapshot_at = Utc::now().naive_utc();
    let mut connection = state.pool.acquire().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *connection)
        .await?;
    let mut tx = connection.begin().await?;
    let entries = db::export_entries(&mut tx, user_id).await?;
    let mut datasets = BTreeMap::new();
    for &(name, sql) in DATASETS {
        datasets.insert(
            name.to_string(),
            Value::Array(json_rows(&mut tx, sql, user_id).await?),
        );
    }
    datasets.insert(
        "collection".to_string(),
        serde_json::to_value(&entries).map_err(|e| AppError::InternalError(e.into()))?,
    );
    datasets.insert("comments".to_string(), json!([]));
    datasets.insert("ratings".to_string(), json!([]));
    let photos: Vec<(u32, String)> = sqlx::query_as("SELECT p.id, p.storage_key FROM collection_photos p JOIN collection_entries ce ON ce.id = p.entry_id WHERE ce.user_id = ? ORDER BY p.id")
        .bind(user_id).fetch_all(&mut *tx).await?;
    let avatar: Option<String> = sqlx::query_scalar(
        "SELECT avatar_path FROM users WHERE id = ? AND account_state = 'active'",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let files_root = root.join("staging");
    private_directory(&files_root)
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    let mut staged_bytes = 0_u64;
    for (photo_id, key) in photos {
        stage_media(
            state,
            &files_root,
            &key,
            &format!("photo-{photo_id}.jpg"),
            &mut staged_bytes,
            &mut files,
            &format!("files/photos/{photo_id}.jpg"),
        )
        .await?;
    }
    if let Some(key) = avatar {
        stage_media(
            state,
            &files_root,
            &key,
            "avatar.jpg",
            &mut staged_bytes,
            &mut files,
            "files/avatar.jpg",
        )
        .await?;
        datasets.insert(
            "avatar".to_string(),
            json!([{ "archive_path": "files/avatar.jpg" }]),
        );
    } else {
        datasets.insert("avatar".to_string(), json!([]));
    }
    let sources: Vec<(String, String, Option<String>)> = sqlx::query_as("SELECT id, format, source_sha256 FROM transfer_jobs WHERE user_id = ? AND kind = 'import' AND files_deleted = FALSE AND status IN ('prepared', 'queued', 'running') ORDER BY id")
        .bind(user_id).fetch_all(&mut *tx).await?;
    for (id, format, checksum) in sources {
        let bytes = tokio::fs::read(state.transfer_config.job_path(&id)?.join("source"))
            .await
            .map_err(|e| AppError::InternalError(e.into()))?;
        // A preview retained from before atomic publication may have been truncated.
        if checksum.as_deref() != Some(&hex::encode(Sha256::digest(&bytes))) {
            return Err(AppError::Conflict("Stored import file changed".to_string()));
        }
        staged_bytes = staged_bytes.saturating_add(bytes.len() as u64);
        enforce_size(staged_bytes, state.transfer_config.max_archive_bytes)?;
        let path = files_root.join(format!("import-{id}.{format}"));
        write_private(&path, &bytes)
            .await
            .map_err(|e| AppError::InternalError(e.into()))?;
        files.push((format!("files/imports/{id}.{format}"), path));
    }
    let subjects: Vec<Subject> = sqlx::query_as("SELECT DISTINCT u.id, u.session_version, u.account_state FROM users u WHERE u.id = ? OR u.id IN (SELECT initiator_id FROM trades WHERE responder_id = ?) OR u.id IN (SELECT responder_id FROM trades WHERE initiator_id = ?) OR u.id IN (SELECT user_low_id FROM trade_matches WHERE user_high_id = ?) OR u.id IN (SELECT user_high_id FROM trade_matches WHERE user_low_id = ?) ORDER BY u.id")
        .bind(user_id).bind(user_id).bind(user_id).bind(user_id).bind(user_id).fetch_all(&mut *tx).await?;
    for subject in subjects {
        sqlx::query("INSERT INTO data_export_subjects (job_id, subject_user_id, session_version, account_state) VALUES (?, ?, ?, ?)")
            .bind(&job.id).bind(subject.id).bind(subject.session_version).bind(subject.account_state).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    let timestamp = snapshot_at.and_utc().to_rfc3339();
    let csv = encode_document(&entries, "csv", &timestamp)?;
    let collection_json = encode_document(&entries, "json", &timestamp)?;
    let partial = root.join("archive.partial");
    let limit = state.transfer_config.max_archive_bytes;
    let archive_timestamp = timestamp.clone();
    let archive_path = partial.clone();
    tokio::task::spawn_blocking(move || {
        write_archive(
            &archive_path,
            &datasets,
            &csv,
            &collection_json,
            &files,
            &archive_timestamp,
            limit,
        )
    })
    .await
    .map_err(|e| AppError::InternalError(e.into()))?
    .map_err(AppError::InternalError)?;
    let byte_size = tokio::fs::metadata(&partial)
        .await
        .map_err(|e| AppError::InternalError(e.into()))?
        .len();
    let mut publication = state.pool.begin().await?;
    lock_valid_subjects(&mut publication, &job.id).await?;
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM transfer_jobs WHERE id = ? AND invalidated_at IS NULL FOR UPDATE",
    )
    .bind(&job.id)
    .fetch_optional(&mut *publication)
    .await?;
    if status.as_deref() != Some("running") {
        return Err(AppError::Conflict("Export was cancelled".to_string()));
    }
    tokio::fs::rename(&partial, root.join("archive.zip"))
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    let now = Utc::now().naive_utc();
    sqlx::query("UPDATE transfer_jobs SET status = 'ready', completed_at = ?, snapshot_at = ?, expires_at = ?, byte_size = ? WHERE id = ?")
        .bind(now).bind(snapshot_at).bind(now + Duration::seconds(EXPORT_SECONDS)).bind(byte_size).bind(&job.id).execute(&mut *publication).await?;
    publication.commit().await?;
    // Archive contents are now durable; remove redundant private staging files.
    let _ = tokio::fs::remove_dir_all(files_root).await;
    Ok(())
}

async fn stage_media(
    state: &AppStateInner,
    root: &Path,
    key: &str,
    name: &str,
    size: &mut u64,
    files: &mut Vec<(String, PathBuf)>,
    archive_name: &str,
) -> Result<(), AppError> {
    let bytes = state
        .media_storage
        .read(key)
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    *size = size.saturating_add(bytes.len() as u64);
    enforce_size(*size, state.transfer_config.max_archive_bytes)?;
    let path = root.join(name);
    write_private(&path, &bytes)
        .await
        .map_err(|e| AppError::InternalError(e.into()))?;
    files.push((archive_name.to_string(), path));
    Ok(())
}

fn enforce_size(bytes: u64, limit: u64) -> Result<(), AppError> {
    if bytes > limit {
        return Err(AppError::PayloadTooLarge(
            "Export exceeds the configured storage limit".to_string(),
        ));
    }
    Ok(())
}

struct LimitedFile {
    file: std::fs::File,
    limit: u64,
}
impl Write for LimitedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let position = self.file.stream_position()?;
        if position.saturating_add(buf.len() as u64) > self.limit {
            return Err(std::io::Error::other("export storage limit"));
        }
        self.file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Seek for LimitedFile {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}

fn write_archive(
    path: &Path,
    datasets: &BTreeMap<String, Value>,
    csv: &[u8],
    collection_json: &[u8],
    media: &[(String, PathBuf)],
    timestamp: &str,
    limit: u64,
) -> anyhow::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    let mut archive = zip::ZipWriter::new(LimitedFile { file, limit });
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o600);
    let mut checksums = BTreeMap::new();
    let mut add = |name: &str, data: &[u8]| -> anyhow::Result<()> {
        archive.start_file(name, options)?;
        archive.write_all(data)?;
        checksums.insert(
            name.to_string(),
            json!({"sha256": hex::encode(Sha256::digest(data)), "bytes": data.len()}),
        );
        Ok(())
    };
    add("README.txt", b"LILLY personal data export v1\nJSON files contain explicit personal-data projections. SQL timestamps are UTC; booleans may use 0/1. collection.json and collection.csv use the documented collection transfer format. Photos, avatar and retained import files are included. Messages include received messages visible to you. Hidden partner account data and authentication secrets are excluded. Empty comments/ratings datasets are not yet implemented. This archive is a data disclosure, not an account restore. See manifest.json for counts, snapshot time and SHA-256 checksums.\n")?;
    add("collection.csv", csv)?;
    add("collection.json", collection_json)?;
    for (name, value) in datasets {
        if name != "collection" {
            add(
                &format!("data/{name}.json"),
                &serde_json::to_vec_pretty(value)?,
            )?;
        }
    }
    drop(add);
    for (name, source) in media {
        archive.start_file(name, options)?;
        let mut input = std::fs::File::open(source)?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        let mut bytes = 0_u64;
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            archive.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
            bytes += count as u64;
        }
        checksums.insert(
            name.clone(),
            json!({"sha256": hex::encode(hash.finalize()), "bytes": bytes}),
        );
    }
    let counts: BTreeMap<&String, usize> = datasets
        .iter()
        .map(|(name, value)| (name, value.as_array().map_or(0, Vec::len)))
        .collect();
    let manifest = json!({"format": "lilly.personal-data", "format_version": 1, "snapshot_at": timestamp,
        "created_at": Utc::now().to_rfc3339(), "counts": counts, "files": checksums,
        "not_implemented": ["comments", "ratings"]});
    archive.start_file("manifest.json", options)?;
    archive.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    archive.finish()?.file.sync_all()?;
    let mut check = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    for index in 0..check.len() {
        std::io::copy(&mut check.by_index(index)?, &mut std::io::sink())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::data_transfer::TransferConfig;

    #[tokio::test]
    async fn archive_contains_actual_files_counts_and_independent_checksums() {
        let config = TransferConfig::for_tests();
        config.prepare().await.unwrap();
        let photo = config.path.join("photo.jpg");
        write_private(&photo, b"actual-photo-bytes").await.unwrap();
        let path = config.path.join("archive.zip");
        let data = BTreeMap::from([
            (
                "messages".to_string(),
                json!([{"content": "received message", "is_mine": false}]),
            ),
            ("collection".to_string(), json!([])),
        ]);
        write_archive(
            &path,
            &data,
            b"csv",
            b"{}",
            &[("files/photos/1.jpg".to_string(), photo)],
            "2026-11-22T12:00:00Z",
            100_000,
        )
        .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        let manifest: Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(manifest["counts"]["messages"], 1);
        assert_eq!(
            manifest["files"]["files/photos/1.jpg"]["sha256"],
            hex::encode(Sha256::digest(b"actual-photo-bytes"))
        );
        let mut bytes = Vec::new();
        archive
            .by_name("files/photos/1.jpg")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"actual-photo-bytes");
        assert!(enforce_size(101, 100).is_err());
        assert!(enforce_size(100, 100).is_ok());
        assert!(
            write_archive(
                &config.path.join("small.zip"),
                &data,
                b"csv",
                b"{}",
                &[],
                "now",
                20
            )
            .is_err()
        );
        assert!(config.path.starts_with(std::env::temp_dir()));
        drop(archive);
        tokio::fs::remove_dir_all(config.path).await.unwrap();
    }
}
