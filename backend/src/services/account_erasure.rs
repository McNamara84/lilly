use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, NaiveDateTime, Utc};
use tokio::io::AsyncWriteExt;

use crate::auth::oauth::{hash_secret, random_urlsafe_token};
use crate::db::account_erasure;
use crate::error::AppError;
use crate::models::account_erasure::{ACCOUNT_DELETION_GRACE_DAYS, AccountDeletionStatusResponse};
use crate::models::user::User;
use crate::routes::AppStateInner;

pub const RECOVERY_COOKIE: &str = "account_deletion_recovery";
pub const RECOVERY_COOKIE_PATH: &str = "/api/v1/me/account-deletion";
const WORKER_INTERVAL_SECONDS: u64 = 60;

#[derive(Debug, Clone)]
pub struct ErasureLedger {
    path: PathBuf,
}

impl ErasureLedger {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub async fn require_existing(&self) -> Result<(), std::io::Error> {
        let metadata = tokio::fs::metadata(&self.path).await?;
        if !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "account erasure ledger path is not a regular file",
            ));
        }
        tokio::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .open(&self.path)
            .await?;
        Ok(())
    }

    pub async fn subjects(&self) -> Result<BTreeSet<String>, std::io::Error> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let subject = line.trim();
                if valid_subject(subject) {
                    Ok(subject.to_string())
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "account erasure ledger contains an invalid entry",
                    ))
                }
            })
            .collect()
    }

    pub async fn record(&self, subject: &str) -> Result<(), std::io::Error> {
        if !valid_subject(subject) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid account erasure subject",
            ));
        }
        if self.subjects().await?.contains(subject) {
            return Ok(());
        }
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(subject.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        file.sync_all().await?;
        Ok(())
    }
}

fn valid_subject(subject: &str) -> bool {
    subject.len() == 64 && subject.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub struct ScheduledDeletion {
    pub status: AccountDeletionStatusResponse,
    pub recovery_token: String,
}

pub async fn schedule(
    pool: &sqlx::MySqlPool,
    user_id: u32,
    now: NaiveDateTime,
) -> Result<ScheduledDeletion, AppError> {
    let mut transaction = pool.begin().await?;
    let user = account_erasure::lock_account(&mut transaction, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Account not found".to_string()))?;
    let raw_recovery_token = random_urlsafe_token();
    let recovery_hash = hash_secret(&raw_recovery_token);

    if user.account_state == "pending_deletion" {
        let job = account_erasure::find_job_for_user_on_transaction(&mut transaction, user_id)
            .await?
            .ok_or_else(|| {
                AppError::InternalError(anyhow::anyhow!(
                    "pending account has no account erasure job"
                ))
            })?;
        if job.status != "scheduled" || job.scheduled_for <= now {
            return Err(AppError::ConflictWithCode {
                message: "The cancellation window has expired".to_string(),
                code: "ACCOUNT_DELETION_WINDOW_EXPIRED".to_string(),
            });
        }
        account_erasure::insert_recovery_token(
            &mut transaction,
            user_id,
            &recovery_hash,
            now,
            job.scheduled_for,
        )
        .await?;
        transaction.commit().await?;
        return Ok(ScheduledDeletion {
            status: AccountDeletionStatusResponse {
                status: job.status,
                requested_at: job.requested_at,
                scheduled_for: job.scheduled_for,
                can_cancel: true,
            },
            recovery_token: raw_recovery_token,
        });
    }
    if user.account_state != "active" {
        return Err(AppError::ConflictWithCode {
            message: "Account cannot be deleted in its current state".to_string(),
            code: "ACCOUNT_DELETION_STATE_INVALID".to_string(),
        });
    }

    let scheduled_for = now + Duration::days(ACCOUNT_DELETION_GRACE_DAYS);
    account_erasure::insert_job(&mut transaction, &user, now, scheduled_for).await?;
    account_erasure::deactivate_account(&mut transaction, user_id).await?;
    account_erasure::revoke_credentials(&mut transaction, user_id, &user.email).await?;
    account_erasure::cancel_open_trades(&mut transaction, user_id).await?;
    account_erasure::insert_recovery_token(
        &mut transaction,
        user_id,
        &recovery_hash,
        now,
        scheduled_for,
    )
    .await?;
    transaction.commit().await?;

    Ok(ScheduledDeletion {
        status: AccountDeletionStatusResponse {
            status: "scheduled".to_string(),
            requested_at: now,
            scheduled_for,
            can_cancel: true,
        },
        recovery_token: raw_recovery_token,
    })
}

pub async fn issue_recovery_token(
    pool: &sqlx::MySqlPool,
    user_id: u32,
    now: NaiveDateTime,
) -> Result<Option<(String, NaiveDateTime)>, AppError> {
    let raw_token = random_urlsafe_token();
    let scheduled_for =
        account_erasure::replace_recovery_token(pool, user_id, &hash_secret(&raw_token), now)
            .await?;
    Ok(scheduled_for.map(|scheduled_for| (raw_token, scheduled_for)))
}

pub async fn cancel(
    pool: &sqlx::MySqlPool,
    raw_recovery_token: &str,
    now: NaiveDateTime,
) -> Result<User, AppError> {
    let mut transaction = pool.begin().await?;
    let target = account_erasure::find_recovery_target(
        &mut transaction,
        &hash_secret(raw_recovery_token),
        now,
    )
    .await?
    .ok_or_else(|| AppError::Forbidden {
        message: "Account deletion recovery is missing or expired".to_string(),
        code: Some("ACCOUNT_DELETION_RECOVERY_REQUIRED".to_string()),
    })?;
    let user = account_erasure::restore_account(&mut transaction, target.user_id).await?;
    transaction.commit().await?;
    if let Err(error) = crate::services::trade_matching::reconcile_user(pool, target.user_id).await
    {
        tracing::warn!(
            user_id = target.user_id,
            error = %error,
            "Failed to reconcile trade matches after restoring an account"
        );
    }
    Ok(user)
}

#[must_use]
pub fn recovery_cookie(raw_token: String, seconds: i64, secure: bool) -> Cookie<'static> {
    Cookie::build((RECOVERY_COOKIE.to_string(), raw_token))
        .path(RECOVERY_COOKIE_PATH.to_string())
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(secure)
        .max_age(time::Duration::seconds(seconds.max(0)))
        .build()
}

#[must_use]
pub fn clear_recovery_cookie() -> Cookie<'static> {
    Cookie::build((RECOVERY_COOKIE.to_string(), String::new()))
        .path(RECOVERY_COOKIE_PATH.to_string())
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(time::Duration::ZERO)
        .build()
}

pub async fn process_due_jobs(state: &AppStateInner) -> Result<u64, anyhow::Error> {
    let mut processed = 0_u64;
    while let Some(job) =
        account_erasure::claim_due_job(&state.pool, Utc::now().naive_utc()).await?
    {
        let job_id = job.id;
        if let Err(error) = process_claimed_job(state, &job).await {
            let exponent = job.attempts.min(10);
            let retry_seconds = 2_i64.pow(exponent).min(3_600);
            account_erasure::mark_job_failed(
                &state.pool,
                job_id,
                error.category(),
                Utc::now().naive_utc() + Duration::seconds(retry_seconds),
            )
            .await?;
            tracing::error!(
                job_id,
                category = error.category(),
                "Account erasure job failed"
            );
            continue;
        }
        processed += 1;
    }
    process_storage_pending(state).await?;
    Ok(processed)
}

async fn process_claimed_job(
    state: &AppStateInner,
    job: &crate::models::account_erasure::AccountErasureJobRow,
) -> Result<(), ErasureProcessingError> {
    let target = account_erasure::find_erasure_target(&state.pool, job.id)
        .await?
        .ok_or(ErasureProcessingError::State)?;
    if target.account_state != "pending_deletion" || Some(target.user_id) != job.user_id {
        return Err(ErasureProcessingError::State);
    }
    state
        .erasure_ledger
        .record(&target.erasure_subject)
        .await
        .map_err(ErasureProcessingError::Ledger)?;
    account_erasure::mark_ledger_recorded(&state.pool, job.id, Utc::now().naive_utc()).await?;
    account_erasure::erase_primary_data(&state.pool, job.id, target.user_id).await?;
    Ok(())
}

async fn process_storage_pending(state: &AppStateInner) -> Result<(), anyhow::Error> {
    let _ = crate::services::media::reconcile_storage(&state.pool, &state.media_storage).await?;
    let now = Utc::now().naive_utc();
    for job_id in account_erasure::storage_pending_job_ids(&state.pool).await? {
        let _ = account_erasure::finish_storage_phase(&state.pool, job_id, now).await?;
    }
    Ok(())
}

pub async fn replay_ledger(state: &AppStateInner) -> Result<u64, anyhow::Error> {
    state.erasure_ledger.require_existing().await?;
    let subjects = state.erasure_ledger.subjects().await?;
    let now = Utc::now().naive_utc();
    let mut restored = 0_u64;
    for subject in &subjects {
        if account_erasure::schedule_restored_subject(&state.pool, subject, now).await? {
            restored += 1;
        }
    }
    process_due_jobs(state).await?;
    for subject in &subjects {
        if account_erasure::subject_exists(&state.pool, subject).await? {
            anyhow::bail!("account erasure ledger replay left a restored account accessible");
        }
    }
    Ok(restored)
}

pub fn spawn_worker(state: Arc<AppStateInner>) {
    tokio::spawn(async move {
        if let Err(error) = account_erasure::recover_running_jobs(&state.pool).await {
            tracing::error!(error = %error, "Failed to recover interrupted account erasure jobs");
        }
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(WORKER_INTERVAL_SECONDS));
        loop {
            interval.tick().await;
            if let Err(error) = process_due_jobs(&state).await {
                tracing::error!(error = %error, "Account erasure worker iteration failed");
            }
        }
    });
}

#[derive(Debug, thiserror::Error)]
enum ErasureProcessingError {
    #[error("ledger failure")]
    Ledger(#[source] std::io::Error),
    #[error("database failure")]
    Database(#[from] sqlx::Error),
    #[error("invalid erasure state")]
    State,
}

impl ErasureProcessingError {
    const fn category(&self) -> &'static str {
        match self {
            Self::Ledger(_) => "ledger_unavailable",
            Self::Database(_) => "database_error",
            Self::State => "invalid_state",
        }
    }
}

pub fn recovery_jar(
    jar: CookieJar,
    raw_token: String,
    scheduled_for: NaiveDateTime,
    secure: bool,
) -> CookieJar {
    let seconds = (scheduled_for - Utc::now().naive_utc()).num_seconds();
    jar.add(recovery_cookie(raw_token, seconds, secure))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    async fn database_pool() -> Option<sqlx::MySqlPool> {
        let database_url = std::env::var("DATABASE_URL").ok()?;
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .ok()?;
        crate::db::migrate_test_database(&pool).await.ok()?;
        Some(pool)
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    async fn insert_user(pool: &sqlx::MySqlPool, suffix: u128, label: &str) -> u32 {
        sqlx::query(
            "INSERT INTO users \
             (email, display_name, email_verified, profile_public, collection_public) \
             VALUES (?, ?, TRUE, TRUE, TRUE)",
        )
        .bind(format!("erasure-{label}-{suffix}@example.test"))
        .bind(format!("Erasure {label}"))
        .execute(pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap()
    }

    async fn initialise_test_ledger(path: &Path) {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, b"").await.unwrap();
    }

    fn erasure_test_state(
        pool: sqlx::MySqlPool,
        root: &Path,
        ledger_path: PathBuf,
    ) -> AppStateInner {
        let media_path = root.join("media");
        AppStateInner {
            pool,
            jwt_secret: "account-erasure-test-secret".to_string(),
            jwt_access_expiry: 900,
            jwt_refresh_expiry: 2_592_000,
            password_reset_ttl_seconds: 3_600,
            email_service: crate::services::email::EmailService::Log {
                from: "test@lilly.app".to_string(),
            },
            app_base_url: "http://localhost".to_string(),
            cookie_secure: false,
            oauth_service: crate::services::oauth::OAuthService::disabled(),
            privacy_policy_version: "test-v1".to_string(),
            adapter_registry: lilly_importer_core::adapter::AdapterRegistry::new(),
            media_path: media_path.clone(),
            media_url_prefix: "/media".to_string(),
            photo_upload_config: crate::config::PhotoUploadConfig::default(),
            media_storage: crate::services::media::MediaStorage::new(&media_path),
            erasure_ledger: ErasureLedger::new(ledger_path),
            import_scheduler_config: crate::services::import_scheduler::ImportSchedulerConfig {
                enabled: false,
                schedule: "0 10 6 * * Sat *".to_string(),
                timezone: "Europe/Berlin".to_string(),
                adapters: Vec::new(),
            },
            request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
        }
    }

    #[test]
    fn subjects_are_strict_hex_identifiers() {
        assert!(valid_subject(&"a".repeat(64)));
        assert!(valid_subject(&"F".repeat(64)));
        assert!(!valid_subject(&"a".repeat(63)));
        assert!(!valid_subject(&format!("{}!", "a".repeat(63))));
    }

    #[tokio::test]
    async fn ledger_is_idempotent_and_rejects_corruption() {
        let root =
            std::env::temp_dir().join(format!("lilly-erasure-ledger-{}", random_urlsafe_token()));
        let path = root.join("ledger");
        let ledger = ErasureLedger::new(&path);
        let subject = "a".repeat(64);
        initialise_test_ledger(&path).await;
        ledger.require_existing().await.unwrap();
        ledger.record(&subject).await.unwrap();
        ledger.record(&subject).await.unwrap();
        assert_eq!(ledger.subjects().await.unwrap().len(), 1);
        assert_eq!(
            tokio::fs::read_to_string(&path).await.unwrap(),
            format!("{subject}\n")
        );

        tokio::fs::write(&path, "invalid\n").await.unwrap();
        assert_eq!(
            ledger.subjects().await.unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn missing_ledger_is_never_created_at_runtime() {
        let root = std::env::temp_dir().join(format!(
            "lilly-missing-erasure-ledger-{}",
            random_urlsafe_token()
        ));
        let path = root.join("ledger");
        let ledger = ErasureLedger::new(&path);
        let subject = "b".repeat(64);

        assert_eq!(
            ledger.require_existing().await.unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            ledger.subjects().await.unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            ledger.record(&subject).await.unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        assert!(!path.exists());
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn removed_ledger_is_not_recreated_by_a_later_write() {
        let root = std::env::temp_dir().join(format!(
            "lilly-removed-erasure-ledger-{}",
            random_urlsafe_token()
        ));
        let path = root.join("ledger");
        let ledger = ErasureLedger::new(&path);
        initialise_test_ledger(&path).await;
        ledger.record(&"c".repeat(64)).await.unwrap();
        tokio::fs::remove_file(&path).await.unwrap();

        assert_eq!(
            ledger.record(&"d".repeat(64)).await.unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        assert!(!path.exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn restore_replay_is_fail_closed_and_idempotent() {
        let Some(pool) = database_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = unique_suffix();
        let user_id = insert_user(&pool, suffix, "restore").await;
        let subject: String = sqlx::query_scalar("SELECT erasure_subject FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let root =
            std::env::temp_dir().join(format!("lilly-erasure-replay-{}", random_urlsafe_token()));
        let ledger_path = root.join("ledger");
        let state = erasure_test_state(pool.clone(), &root, ledger_path.clone());

        assert!(replay_ledger(&state).await.is_err());
        assert!(
            account_erasure::subject_exists(&pool, &subject)
                .await
                .unwrap()
        );

        initialise_test_ledger(&ledger_path).await;
        state.erasure_ledger.record(&subject).await.unwrap();
        assert_eq!(replay_ledger(&state).await.unwrap(), 1);
        assert!(
            !account_erasure::subject_exists(&pool, &subject)
                .await
                .unwrap()
        );
        assert_eq!(replay_ledger(&state).await.unwrap(), 0);

        let replay_job_id: u64 = sqlx::query_scalar(
            "SELECT id FROM account_erasure_jobs WHERE status = 'completed' \
             ORDER BY id DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM account_erasure_jobs WHERE id = ?")
            .bind(replay_job_id)
            .execute(&pool)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[test]
    fn recovery_cookie_is_narrow_and_hardened() {
        let cookie = recovery_cookie("secret".to_string(), 60, true);
        assert_eq!(cookie.path(), Some(RECOVERY_COOKIE_PATH));
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));
        assert!(cookie.http_only().unwrap_or(false));
        assert!(cookie.secure().unwrap_or(false));
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn scheduling_revokes_sessions_and_cancellation_restores_visibility() {
        let Some(pool) = database_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = unique_suffix();
        let user_id = insert_user(&pool, suffix, "cancel").await;
        let counterpart_id = insert_user(&pool, suffix, "counterpart").await;
        let (low, high) =
            crate::db::trade_matching::normalize_user_pair(user_id, counterpart_id).unwrap();
        let match_id: u32 = sqlx::query(
            "INSERT INTO trade_matches \
             (user_low_id, user_high_id, status, fingerprint) VALUES (?, ?, 'active', ?)",
        )
        .bind(low)
        .bind(high)
        .bind(format!("{suffix:064x}"))
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let trade_id: u32 = sqlx::query(
            "INSERT INTO trades \
             (match_id, initiator_id, responder_id, status, open_match_id) \
             VALUES (?, ?, ?, 'proposed', ?)",
        )
        .bind(match_id)
        .bind(user_id)
        .bind(counterpart_id)
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        crate::db::refresh_tokens::store_refresh_token(
            &pool,
            user_id,
            &format!("{suffix:064x}"),
            Utc::now().naive_utc() + Duration::days(30),
            Utc::now().naive_utc(),
        )
        .await
        .unwrap();
        let now = Utc::now().naive_utc();
        sqlx::query(
            "INSERT INTO password_reset_tokens \
             (user_id, token_hash, created_at, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(hash_secret(&format!("reset-{suffix}")))
        .bind(now)
        .bind(now + Duration::hours(1))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO pending_oauth_links \
             (token_hash, provider, provider_subject, verified_email, display_name, \
              created_at, expires_at) VALUES (?, 'google', ?, ?, 'Erasure cancel', ?, ?)",
        )
        .bind(hash_secret(&format!("pending-link-{suffix}")))
        .bind(format!("pending-subject-{suffix}"))
        .bind(format!("erasure-cancel-{suffix}@example.test"))
        .bind(now)
        .bind(now + Duration::minutes(10))
        .execute(&pool)
        .await
        .unwrap();

        let scheduled = schedule(&pool, user_id, now).await.unwrap();
        assert_eq!(scheduled.status.status, "scheduled");
        assert_eq!(scheduled.status.scheduled_for, now + Duration::days(7));
        let state: (String, bool, bool, u32) = sqlx::query_as(
            "SELECT account_state, profile_public, collection_public, session_version \
             FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, ("pending_deletion".to_string(), false, false, 1));
        let cancelled_trade: (String, Option<String>, Option<u32>) =
            sqlx::query_as("SELECT status, cancellation_reason, match_id FROM trades WHERE id = ?")
                .bind(trade_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            cancelled_trade,
            (
                "cancelled".to_string(),
                Some("account_deletion".to_string()),
                None
            )
        );
        let notice: (Option<u32>, Option<u32>, String) = sqlx::query_as(
            "SELECT actor_user_id, match_id, payload FROM notifications \
             WHERE user_id = ? AND trade_id = ? AND kind = 'trade_cancelled'",
        )
        .bind(counterpart_id)
        .bind(trade_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(notice.0, None);
        assert_eq!(notice.1, None);
        assert!(notice.2.contains("account_deletion"));
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT revoked FROM refresh_tokens WHERE user_id = ?",)
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap()
        );
        for table in ["password_reset_tokens", "pending_oauth_links"] {
            let count = if table == "password_reset_tokens" {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM password_reset_tokens WHERE user_id = ?",
                )
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap()
            } else {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM pending_oauth_links WHERE verified_email = ?",
                )
                .bind(format!("erasure-cancel-{suffix}@example.test"))
                .fetch_one(&pool)
                .await
                .unwrap()
            };
            assert_eq!(count, 0, "{table} must be revoked during scheduling");
        }

        let restored = cancel(&pool, &scheduled.recovery_token, now + Duration::seconds(1))
            .await
            .unwrap();
        assert!(restored.is_active());
        let restored_state: (bool, bool, u32) = sqlx::query_as(
            "SELECT profile_public, collection_public, session_version FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(restored_state, (true, true, 2));
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM trades WHERE id = ?")
                .bind(trade_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "cancelled"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM account_erasure_jobs WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(counterpart_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM trades WHERE id = ?")
            .bind(trade_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn final_erasure_anonymises_shared_trade_and_message_history() {
        let Some(pool) = database_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = unique_suffix();
        let erased_user_id = insert_user(&pool, suffix, "erase").await;
        let remaining_user_id = insert_user(&pool, suffix, "remain").await;
        let erased_email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = ?")
            .bind(erased_user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let avatar_storage_key = format!("erasure-avatar-{suffix}.jpg");
        sqlx::query("UPDATE users SET avatar_path = ? WHERE id = ?")
            .bind(&avatar_storage_key)
            .bind(erased_user_id)
            .execute(&pool)
            .await
            .unwrap();
        let series_id: u32 =
            sqlx::query("INSERT INTO series (name, slug, active) VALUES (?, ?, TRUE)")
                .bind(format!("Erasure Series {suffix}"))
                .bind(format!("erasure-series-{suffix}"))
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_id()
                .try_into()
                .unwrap();
        let issue_id: u32 = sqlx::query(
            "INSERT INTO issues (series_id, issue_number, title) VALUES (?, 1, 'Shared issue')",
        )
        .bind(series_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_consents \
             (user_id, policy_version, consented_at, registration_method) \
             VALUES (?, 'erasure-test-v1', CURRENT_TIMESTAMP(6), 'password')",
        )
        .bind(erased_user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO oauth_identities (user_id, provider, provider_subject) \
             VALUES (?, 'github', ?)",
        )
        .bind(erased_user_id)
        .bind(format!("erasure-oauth-{suffix}"))
        .execute(&pool)
        .await
        .unwrap();
        let role_event_id: u32 = sqlx::query(
            "INSERT INTO role_change_events \
             (target_user_id, previous_role, new_role, method) \
             VALUES (?, 'user', 'admin', 'cli')",
        )
        .bind(erased_user_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let import_job_id: u32 = sqlx::query(
            "INSERT INTO import_jobs \
             (series_id, adapter_name, source_key, trigger_type, started_by) \
             VALUES (?, 'erasure-test', 'erasure-test', 'manual', ?)",
        )
        .bind(series_id)
        .bind(erased_user_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let publication_event_id: u32 = sqlx::query(
            "INSERT INTO series_publication_events \
             (series_id, import_job_id, actor_user_id, action, decision) \
             VALUES (?, ?, ?, 'activated', 'clean')",
        )
        .bind(series_id)
        .bind(import_job_id)
        .bind(erased_user_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let (low, high) =
            crate::db::trade_matching::normalize_user_pair(erased_user_id, remaining_user_id)
                .unwrap();
        let match_id: u32 = sqlx::query(
            "INSERT INTO trade_matches \
             (user_low_id, user_high_id, status, fingerprint) VALUES (?, ?, 'stale', ?)",
        )
        .bind(low)
        .bind(high)
        .bind(format!("{:064x}", suffix + 1))
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let trade_id: u32 = sqlx::query(
            "INSERT INTO trades \
             (match_id, initiator_id, responder_id, status, completed_at) \
             VALUES (?, ?, ?, 'completed', CURRENT_TIMESTAMP)",
        )
        .bind(match_id)
        .bind(erased_user_id)
        .bind(remaining_user_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        sqlx::query(
            "INSERT INTO trade_items \
             (trade_id, issue_id, offered_by_user_id, receiving_user_id, \
              copy_number_snapshot, condition_grade_snapshot) \
             VALUES (?, ?, ?, ?, 1, 'Z2')",
        )
        .bind(trade_id)
        .bind(issue_id)
        .bind(erased_user_id)
        .bind(remaining_user_id)
        .execute(&pool)
        .await
        .unwrap();
        let thread_id: u32 = sqlx::query("INSERT INTO message_threads (trade_id) VALUES (?)")
            .bind(trade_id)
            .execute(&pool)
            .await
            .unwrap()
            .last_insert_id()
            .try_into()
            .unwrap();
        let uuid_tail = suffix & 0xffff_ffff_ffff;
        sqlx::query(
            "INSERT INTO messages (thread_id, sender_id, client_message_id, content) \
             VALUES (?, ?, ?, 'private erased text'), (?, ?, ?, 'retained reply')",
        )
        .bind(thread_id)
        .bind(erased_user_id)
        .bind(format!("00000000-0000-4000-8000-{uuid_tail:012x}"))
        .bind(thread_id)
        .bind(remaining_user_id)
        .bind(format!("00000000-0000-4000-9000-{uuid_tail:012x}"))
        .execute(&pool)
        .await
        .unwrap();

        let now = Utc::now().naive_utc();
        schedule(&pool, erased_user_id, now).await.unwrap();
        let late_pending_link_hash = hash_secret(&format!("late-erasure-link-{suffix}"));
        sqlx::query(
            "INSERT INTO pending_oauth_links \
             (token_hash, provider, provider_subject, verified_email, display_name, created_at, expires_at) \
             VALUES (?, 'google', ?, ?, 'Late OAuth callback', ?, ?)",
        )
        .bind(&late_pending_link_hash)
        .bind(format!("late-erasure-subject-{suffix}"))
        .bind(&erased_email)
        .bind(now)
        .bind(now + Duration::minutes(10))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE account_erasure_jobs SET scheduled_for = ? WHERE user_id = ?")
            .bind(now - Duration::seconds(1))
            .bind(erased_user_id)
            .execute(&pool)
            .await
            .unwrap();
        let job = account_erasure::claim_due_job(&pool, now)
            .await
            .unwrap()
            .unwrap();
        let target = account_erasure::find_erasure_target(&pool, job.id)
            .await
            .unwrap()
            .unwrap();
        let root = std::env::temp_dir().join(format!("lilly-final-erasure-{suffix}"));
        let ledger_path = root.join("ledger");
        initialise_test_ledger(&ledger_path).await;
        let ledger = ErasureLedger::new(ledger_path);
        ledger.record(&target.erasure_subject).await.unwrap();
        account_erasure::mark_ledger_recorded(&pool, job.id, now)
            .await
            .unwrap();
        account_erasure::erase_primary_data(&pool, job.id, erased_user_id)
            .await
            .unwrap();
        assert!(
            !account_erasure::finish_storage_phase(&pool, job.id, now)
                .await
                .unwrap(),
            "the job must wait for its linked file deletion"
        );
        let media_job: (Option<u64>, Option<NaiveDateTime>) = sqlx::query_as(
            "SELECT erasure_job_id, processed_at FROM media_deletion_jobs WHERE storage_key = ?",
        )
        .bind(&avatar_storage_key)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(media_job, (Some(job.id), None));
        sqlx::query("UPDATE media_deletion_jobs SET processed_at = ? WHERE storage_key = ?")
            .bind(now)
            .bind(&avatar_storage_key)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            account_erasure::finish_storage_phase(&pool, job.id, now)
                .await
                .unwrap()
        );

        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE id = ?")
                .bind(erased_user_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM privacy_consents WHERE user_id = ?",
            )
            .bind(erased_user_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM oauth_identities WHERE user_id = ?",
            )
            .bind(erased_user_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM pending_oauth_links WHERE token_hash = ?",
            )
            .bind(&late_pending_link_hash)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, Option<u32>>(
                "SELECT target_user_id FROM role_change_events WHERE id = ?",
            )
            .bind(role_event_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            None
        );
        assert_eq!(
            sqlx::query_scalar::<_, Option<u32>>(
                "SELECT started_by FROM import_jobs WHERE id = ?",
            )
            .bind(import_job_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            None
        );
        assert_eq!(
            sqlx::query_scalar::<_, Option<u32>>(
                "SELECT actor_user_id FROM series_publication_events WHERE id = ?",
            )
            .bind(publication_event_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            None
        );
        let participants: (Option<u32>, Option<u32>, Option<u32>) =
            sqlx::query_as("SELECT match_id, initiator_id, responder_id FROM trades WHERE id = ?")
                .bind(trade_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(participants, (None, None, Some(remaining_user_id)));
        let messages: Vec<(Option<u32>, String)> = sqlx::query_as(
            "SELECT sender_id, content FROM messages WHERE thread_id = ? ORDER BY id",
        )
        .bind(thread_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            messages,
            vec![
                (
                    None,
                    "[Nachricht vom gelöschten Konto entfernt]".to_string()
                ),
                (Some(remaining_user_id), "retained reply".to_string()),
            ]
        );
        assert_eq!(ledger.subjects().await.unwrap().len(), 1);

        sqlx::query("DELETE FROM trades WHERE id = ?")
            .bind(trade_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM account_erasure_jobs WHERE id = ?")
            .bind(job.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM media_deletion_jobs WHERE storage_key = ?")
            .bind(&avatar_storage_key)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(remaining_user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM series_publication_events WHERE id = ?")
            .bind(publication_event_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM import_jobs WHERE id = ?")
            .bind(import_job_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM role_change_events WHERE id = ?")
            .bind(role_event_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
