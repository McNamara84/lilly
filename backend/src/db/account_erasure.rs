use chrono::NaiveDateTime;
use serde_json::json;
use sqlx::{MySql, MySqlPool, Transaction};

use crate::models::account_erasure::{
    AccountDeletionStatusResponse, AccountErasureJobRow, AdminAccountErasureJobResponse,
};
use crate::models::user::User;

#[derive(Debug, sqlx::FromRow)]
pub struct ErasureAccountRow {
    pub id: u32,
    pub email: String,
    pub account_state: String,
    pub profile_public: bool,
    pub collection_public: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RecoveryTargetRow {
    pub user_id: u32,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct AuthMethodsRow {
    pub password: bool,
    pub google: bool,
    pub github: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ErasureTargetRow {
    pub user_id: u32,
    pub erasure_subject: String,
    pub account_state: String,
}

pub async fn lock_account(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
) -> Result<Option<ErasureAccountRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, email, account_state, \
                profile_public, collection_public \
         FROM users WHERE id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn find_status(
    pool: &MySqlPool,
    user_id: u32,
    now: NaiveDateTime,
) -> Result<Option<AccountDeletionStatusResponse>, sqlx::Error> {
    sqlx::query_as(
        "SELECT status, requested_at, scheduled_for, \
                (scheduled_for > ? AND status = 'scheduled') AS can_cancel \
         FROM account_erasure_jobs WHERE user_id = ?",
    )
    .bind(now)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_status_by_recovery_token(
    pool: &MySqlPool,
    token_hash: &str,
    now: NaiveDateTime,
) -> Result<Option<AccountDeletionStatusResponse>, sqlx::Error> {
    sqlx::query_as(
        "SELECT j.status, j.requested_at, j.scheduled_for, \
                (j.scheduled_for > ? AND j.status = 'scheduled') AS can_cancel \
         FROM account_erasure_recovery_tokens rt \
         JOIN account_erasure_jobs j ON j.user_id = rt.user_id \
         JOIN users u ON u.id = rt.user_id \
         WHERE rt.token_hash = ? AND rt.consumed_at IS NULL \
           AND rt.expires_at > ? AND u.account_state = 'pending_deletion'",
    )
    .bind(now)
    .bind(token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await
}

pub async fn subject_exists(pool: &MySqlPool, subject: &str) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE erasure_subject = ?")
            .bind(subject)
            .fetch_one(pool)
            .await?
            != 0,
    )
}

pub async fn find_auth_methods(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Option<AuthMethodsRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT (u.password_hash IS NOT NULL) AS password, \
                EXISTS(SELECT 1 FROM oauth_identities oi \
                       WHERE oi.user_id = u.id AND oi.provider = 'google') AS google, \
                EXISTS(SELECT 1 FROM oauth_identities oi \
                       WHERE oi.user_id = u.id AND oi.provider = 'github') AS github \
         FROM users u WHERE u.id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_job_for_user_on_transaction(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
) -> Result<Option<AccountErasureJobRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, status, previous_profile_public, previous_collection_public, \
                requested_at, scheduled_for, started_at, completed_at, ledger_recorded_at, \
                attempts, next_retry_at, last_error_category \
         FROM account_erasure_jobs WHERE user_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn insert_job(
    transaction: &mut Transaction<'_, MySql>,
    user: &ErasureAccountRow,
    requested_at: NaiveDateTime,
    scheduled_for: NaiveDateTime,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO account_erasure_jobs \
         (user_id, previous_profile_public, previous_collection_public, requested_at, scheduled_for) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user.id)
    .bind(user.profile_public)
    .bind(user.collection_public)
    .bind(requested_at)
    .bind(scheduled_for)
    .execute(&mut **transaction)
    .await?;
    Ok(result.last_insert_id())
}

pub async fn deactivate_account(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET account_state = 'pending_deletion', session_version = session_version + 1, \
                profile_public = FALSE, collection_public = FALSE \
         WHERE id = ? AND account_state = 'active'",
    )
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn revoke_credentials(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
    email: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = ? AND revoked = FALSE")
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query("DELETE FROM pending_oauth_links WHERE verified_email = ?")
        .bind(email)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub async fn cancel_open_trades(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
) -> Result<(), sqlx::Error> {
    let trades = sqlx::query_as::<_, crate::db::trade_workflow::ProposalCancellationRow>(
        "SELECT id, match_id, initiator_id, responder_id FROM trades \
         WHERE status IN ('proposed', 'accepted') \
           AND (initiator_id = ? OR responder_id = ?) FOR UPDATE",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&mut **transaction)
    .await?;
    sqlx::query(
        "UPDATE trades SET status = 'cancelled', open_match_id = NULL, \
                cancellation_reason = 'account_deletion', cancelled_at = CURRENT_TIMESTAMP \
         WHERE status IN ('proposed', 'accepted') AND (initiator_id = ? OR responder_id = ?)",
    )
    .bind(user_id)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;

    for trade in trades {
        let recipient_id = if trade.initiator_id == user_id {
            trade.responder_id
        } else {
            trade.initiator_id
        };
        crate::db::notifications::insert_notification(
            transaction,
            recipient_id,
            None,
            "trade_cancelled",
            // The derived match is removed in this transaction. Keeping this
            // FK null prevents its ON DELETE CASCADE from discarding the
            // neutral cancellation notice meant for the remaining account.
            None,
            Some(trade.id),
            None,
            &format!("trade:{}:account-deletion", trade.id),
            &json!({ "trade_id": trade.id, "reason": "account_deletion" }),
        )
        .await?;
    }

    // Match records are derived state. Removing them also clears their item rows;
    // terminal trades retain a nullable historic match reference.
    sqlx::query("DELETE FROM trade_matches WHERE user_low_id = ? OR user_high_id = ?")
        .bind(user_id)
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub async fn insert_recovery_token(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
    token_hash: &str,
    now: NaiveDateTime,
    expires_at: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO account_erasure_recovery_tokens \
         (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(now)
    .bind(expires_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn replace_recovery_token(
    pool: &MySqlPool,
    user_id: u32,
    token_hash: &str,
    now: NaiveDateTime,
) -> Result<Option<NaiveDateTime>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let scheduled_for = sqlx::query_scalar::<_, NaiveDateTime>(
        "SELECT j.scheduled_for FROM account_erasure_jobs j \
         JOIN users u ON u.id = j.user_id \
         WHERE j.user_id = ? AND j.status = 'scheduled' \
           AND u.account_state = 'pending_deletion' AND j.scheduled_for > ? FOR UPDATE",
    )
    .bind(user_id)
    .bind(now)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(scheduled_for) = scheduled_for else {
        transaction.rollback().await?;
        return Ok(None);
    };
    sqlx::query("DELETE FROM account_erasure_recovery_tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    insert_recovery_token(&mut transaction, user_id, token_hash, now, scheduled_for).await?;
    transaction.commit().await?;
    Ok(Some(scheduled_for))
}

pub async fn find_recovery_target(
    transaction: &mut Transaction<'_, MySql>,
    token_hash: &str,
    now: NaiveDateTime,
) -> Result<Option<RecoveryTargetRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT rt.user_id \
         FROM account_erasure_recovery_tokens rt \
         JOIN account_erasure_jobs j ON j.user_id = rt.user_id \
         JOIN users u ON u.id = rt.user_id \
         WHERE rt.token_hash = ? AND rt.consumed_at IS NULL AND rt.expires_at > ? \
           AND j.status = 'scheduled' AND j.scheduled_for > ? \
           AND u.account_state = 'pending_deletion' FOR UPDATE",
    )
    .bind(token_hash)
    .bind(now)
    .bind(now)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn restore_account(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
) -> Result<User, sqlx::Error> {
    let restored = sqlx::query(
        "UPDATE users u JOIN account_erasure_jobs j ON j.user_id = u.id \
         SET u.account_state = 'active', u.session_version = u.session_version + 1, \
             u.profile_public = j.previous_profile_public, \
             u.collection_public = j.previous_collection_public \
         WHERE u.id = ? AND u.account_state = 'pending_deletion' AND j.status = 'scheduled'",
    )
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;
    if restored.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name, role, email_verified, \
                account_state, session_version, erasure_subject \
         FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query("DELETE FROM account_erasure_recovery_tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query("DELETE FROM account_erasure_jobs WHERE user_id = ? AND status = 'scheduled'")
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;
    Ok(user)
}

pub async fn claim_due_job(
    pool: &MySqlPool,
    now: NaiveDateTime,
) -> Result<Option<AccountErasureJobRow>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let job_id = sqlx::query_scalar::<_, u64>(
        "SELECT id FROM account_erasure_jobs \
         WHERE user_id IS NOT NULL AND scheduled_for <= ? \
           AND (status = 'scheduled' OR (status = 'failed' AND (next_retry_at IS NULL OR next_retry_at <= ?))) \
         ORDER BY scheduled_for, id LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .bind(now)
    .bind(now)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(job_id) = job_id else {
        transaction.commit().await?;
        return Ok(None);
    };
    sqlx::query(
        "UPDATE account_erasure_jobs SET status = 'running', started_at = COALESCE(started_at, ?), \
                attempts = attempts + 1, next_retry_at = NULL, last_error_category = NULL \
         WHERE id = ?",
    )
    .bind(now)
    .bind(job_id)
    .execute(&mut *transaction)
    .await?;
    let job = sqlx::query_as::<_, AccountErasureJobRow>(
        "SELECT id, user_id, status, previous_profile_public, previous_collection_public, \
                requested_at, scheduled_for, started_at, completed_at, ledger_recorded_at, \
                attempts, next_retry_at, last_error_category \
         FROM account_erasure_jobs WHERE id = ?",
    )
    .bind(job_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Some(job))
}

pub async fn mark_job_failed(
    pool: &MySqlPool,
    job_id: u64,
    category: &str,
    next_retry_at: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE account_erasure_jobs SET status = 'failed', last_error_category = ?, \
                next_retry_at = ? WHERE id = ? AND status = 'running'",
    )
    .bind(category)
    .bind(next_retry_at)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn recover_running_jobs(pool: &MySqlPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE account_erasure_jobs SET status = 'failed', \
                last_error_category = 'worker_interrupted', next_retry_at = CURRENT_TIMESTAMP \
         WHERE status = 'running'",
    )
    .execute(pool)
    .await?
    .rows_affected())
}

pub async fn mark_ledger_recorded(
    pool: &MySqlPool,
    job_id: u64,
    now: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE account_erasure_jobs SET ledger_recorded_at = COALESCE(ledger_recorded_at, ?) \
         WHERE id = ? AND status = 'running'",
    )
    .bind(now)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_erasure_target(
    pool: &MySqlPool,
    job_id: u64,
) -> Result<Option<ErasureTargetRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT u.id AS user_id, u.erasure_subject, u.account_state \
         FROM account_erasure_jobs j JOIN users u ON u.id = j.user_id \
         WHERE j.id = ? AND j.status = 'running'",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
}

pub async fn erase_primary_data(
    pool: &MySqlPool,
    job_id: u64,
    user_id: u32,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let account = sqlx::query_as::<_, (String, String)>(
        "SELECT account_state, email FROM users WHERE id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((state, email)) = account else {
        transaction.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    };
    if state != "pending_deletion" {
        transaction.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }

    sqlx::query(
        "INSERT INTO media_deletion_jobs (storage_key, erasure_job_id) \
         SELECT avatar_path, ? FROM users WHERE id = ? AND avatar_path IS NOT NULL \
         ON DUPLICATE KEY UPDATE erasure_job_id = VALUES(erasure_job_id), \
             processed_at = NULL, last_error = NULL",
    )
    .bind(job_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO media_deletion_jobs (storage_key, erasure_job_id) \
         SELECT cp.storage_key, ? FROM collection_photos cp \
         JOIN collection_entries ce ON ce.id = cp.entry_id WHERE ce.user_id = ? \
         ON DUPLICATE KEY UPDATE erasure_job_id = VALUES(erasure_job_id), \
             processed_at = NULL, last_error = NULL",
    )
    .bind(job_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;

    // The other participant keeps the chronology, not content authored by the
    // erased account or a reconstructable sender reference.
    sqlx::query(
        "UPDATE messages SET content = '[Nachricht vom gelöschten Konto entfernt]' \
         WHERE sender_id = ?",
    )
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM notifications WHERE actor_user_id = ? AND user_id <> ?")
        .bind(user_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;

    // These should already be gone since scheduling. Repeating the cleanup
    // closes over credentials and OAuth callbacks created during the grace
    // period and keeps restore replay and crash recovery idempotent.
    revoke_credentials(&mut transaction, user_id, &email).await?;
    cancel_open_trades(&mut transaction, user_id).await?;

    let deleted =
        sqlx::query("DELETE FROM users WHERE id = ? AND account_state = 'pending_deletion'")
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
    if deleted.rows_affected() != 1 {
        transaction.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }

    // Once both participants are gone the shared record no longer serves any
    // remaining account and can be removed completely.
    sqlx::query("DELETE FROM trades WHERE initiator_id IS NULL AND responder_id IS NULL")
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "UPDATE account_erasure_jobs SET status = 'storage_pending', user_id = NULL, \
                last_error_category = NULL, next_retry_at = NULL WHERE id = ?",
    )
    .bind(job_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn finish_storage_phase(
    pool: &MySqlPool,
    job_id: u64,
    now: NaiveDateTime,
) -> Result<bool, sqlx::Error> {
    let pending = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM media_deletion_jobs \
         WHERE erasure_job_id = ? AND processed_at IS NULL",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;
    if pending != 0 {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE account_erasure_jobs SET status = 'completed', completed_at = ?, \
                last_error_category = NULL, next_retry_at = NULL \
         WHERE id = ? AND status = 'storage_pending'",
    )
    .bind(now)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(true)
}

pub async fn storage_pending_job_ids(pool: &MySqlPool) -> Result<Vec<u64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM account_erasure_jobs WHERE status = 'storage_pending' ORDER BY id",
    )
    .fetch_all(pool)
    .await
}

pub async fn schedule_restored_subject(
    pool: &MySqlPool,
    subject: &str,
    now: NaiveDateTime,
) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let user = sqlx::query_as::<_, ErasureAccountRow>(
        "SELECT id, email, account_state, \
                profile_public, collection_public \
         FROM users WHERE erasure_subject = ? FOR UPDATE",
    )
    .bind(subject)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(user) = user else {
        transaction.commit().await?;
        return Ok(false);
    };
    if find_job_for_user_on_transaction(&mut transaction, user.id)
        .await?
        .is_none()
    {
        insert_job(&mut transaction, &user, now, now).await?;
    } else {
        sqlx::query(
            "UPDATE account_erasure_jobs SET status = 'scheduled', scheduled_for = ?, \
                    started_at = NULL, completed_at = NULL, next_retry_at = NULL, \
                    last_error_category = NULL WHERE user_id = ?",
        )
        .bind(now)
        .bind(user.id)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query(
        "UPDATE users SET account_state = 'pending_deletion', \
                session_version = session_version + 1, profile_public = FALSE, \
                collection_public = FALSE WHERE id = ?",
    )
    .bind(user.id)
    .execute(&mut *transaction)
    .await?;
    revoke_credentials(&mut transaction, user.id, &user.email).await?;
    cancel_open_trades(&mut transaction, user.id).await?;
    transaction.commit().await?;
    Ok(true)
}

pub async fn list_admin_jobs(
    pool: &MySqlPool,
) -> Result<Vec<AdminAccountErasureJobResponse>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, status, requested_at, scheduled_for, started_at, completed_at, \
                attempts, next_retry_at, last_error_category \
         FROM account_erasure_jobs WHERE status <> 'completed' \
         ORDER BY scheduled_for, id LIMIT 100",
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use sqlx::mysql::MySqlPoolOptions;

    #[test]
    fn job_column_projection_contains_no_identity_fields() {
        let projection =
            "id user_id status requested_at scheduled_for attempts last_error_category";
        assert!(!projection.contains("email"));
        assert!(!projection.contains("display_name"));
        assert!(!projection.contains("erasure_subject"));
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn nullable_trade_participants_still_reject_self_trades() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        crate::db::migrate_test_database(&pool)
            .await
            .expect("test migrations must succeed");
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let first_user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role, email_verified) \
             VALUES (?, 'First trigger user', 'user', TRUE)",
        )
        .bind(format!("trade-trigger-first-{suffix}@example.test"))
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let second_user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role, email_verified) \
             VALUES (?, 'Second trigger user', 'user', TRUE)",
        )
        .bind(format!("trade-trigger-second-{suffix}@example.test"))
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let match_id: u32 = sqlx::query(
            "INSERT INTO trade_matches (user_low_id, user_high_id, status, fingerprint) \
             VALUES (?, ?, 'stale', ?)",
        )
        .bind(first_user_id.min(second_user_id))
        .bind(first_user_id.max(second_user_id))
        .bind(format!("{suffix:064x}"))
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();

        let self_trade = sqlx::query(
            "INSERT INTO trades (match_id, initiator_id, responder_id, status) \
             VALUES (?, ?, ?, 'proposed')",
        )
        .bind(match_id)
        .bind(first_user_id)
        .bind(first_user_id)
        .execute(&pool)
        .await;
        assert!(
            self_trade.is_err(),
            "the trade trigger must reject self-trades"
        );

        let trade_id: u32 = sqlx::query(
            "INSERT INTO trades \
             (match_id, initiator_id, responder_id, status, completed_at) \
             VALUES (?, ?, ?, 'completed', CURRENT_TIMESTAMP)",
        )
        .bind(match_id)
        .bind(first_user_id)
        .bind(second_user_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let series_id: u32 = sqlx::query("INSERT INTO series (name, slug) VALUES (?, ?)")
            .bind(format!("Trade trigger series {suffix}"))
            .bind(format!("trade-trigger-series-{suffix}"))
            .execute(&pool)
            .await
            .unwrap()
            .last_insert_id()
            .try_into()
            .unwrap();
        let issue_id: u32 =
            sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, 1, ?)")
                .bind(series_id)
                .bind(format!("Trade trigger issue {suffix}"))
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_id()
                .try_into()
                .unwrap();
        let self_trade_item = sqlx::query(
            "INSERT INTO trade_items \
             (trade_id, issue_id, offered_by_user_id, receiving_user_id, \
              copy_number_snapshot, condition_grade_snapshot) \
             VALUES (?, ?, ?, ?, 1, 'Z1')",
        )
        .bind(trade_id)
        .bind(issue_id)
        .bind(first_user_id)
        .bind(first_user_id)
        .execute(&pool)
        .await;
        assert!(
            self_trade_item.is_err(),
            "the trade-item trigger must reject identical participants"
        );
        let self_trade_update = sqlx::query("UPDATE trades SET responder_id = ? WHERE id = ?")
            .bind(first_user_id)
            .bind(trade_id)
            .execute(&pool)
            .await;
        assert!(
            self_trade_update.is_err(),
            "the trade update trigger must reject self-trades"
        );
        let trade_item_id: u32 = sqlx::query(
            "INSERT INTO trade_items \
             (trade_id, issue_id, offered_by_user_id, receiving_user_id, \
              copy_number_snapshot, condition_grade_snapshot) \
             VALUES (?, ?, ?, ?, 1, 'Z1')",
        )
        .bind(trade_id)
        .bind(issue_id)
        .bind(first_user_id)
        .bind(second_user_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let self_trade_item_update =
            sqlx::query("UPDATE trade_items SET receiving_user_id = ? WHERE id = ?")
                .bind(first_user_id)
                .bind(trade_item_id)
                .execute(&pool)
                .await;
        assert!(
            self_trade_item_update.is_err(),
            "the trade-item update trigger must reject identical participants"
        );

        sqlx::query("DELETE FROM trades WHERE id = ?")
            .bind(trade_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM trade_matches WHERE id = ?")
            .bind(match_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id IN (?, ?)")
            .bind(first_user_id)
            .bind(second_user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
