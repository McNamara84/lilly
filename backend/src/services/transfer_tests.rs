use std::path::PathBuf;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use serde_json::Value;
use tower::ServiceExt;

use crate::db::collection_transfer as db;
use crate::models::collection_transfer::TransferEntry;
use crate::routes::{AppState, AppStateInner};
use crate::services::{collection_transfer as format, data_transfer as transfer};

struct Fixture {
    state: AppState,
    owner: u32,
    other: u32,
    admin: u32,
    series: u32,
    issues: Vec<u32>,
    slug: String,
    root: PathBuf,
}

impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("DATABASE_URL")
            .expect("Transfer integration tests require an isolated DATABASE_URL");
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(8)
            .connect(&url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let suffix = transfer::random_id();
        let mut users = Vec::new();
        for role in ["owner", "other", "admin"] {
            let id = sqlx::query("INSERT INTO users (email, display_name, email_verified, role, location) VALUES (?, ?, TRUE, ?, ?)")
                .bind(format!("transfer-{role}-{suffix}@example.test")).bind(format!("Transfer {role}"))
                .bind(if role == "admin" { "admin" } else { "user" }).bind(format!("PRIVATE-{role}-location"))
                .execute(&pool).await.unwrap().last_insert_id();
            users.push(u32::try_from(id).unwrap());
        }
        let slug = format!("transfer-{suffix}");
        let series = u32::try_from(
            sqlx::query("INSERT INTO series (name, slug, active) VALUES (?, ?, TRUE)")
                .bind(&slug)
                .bind(&slug)
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_id(),
        )
        .unwrap();
        let mut issues = Vec::new();
        for number in 1..=4 {
            let id =
                sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)")
                    .bind(series)
                    .bind(number)
                    .bind(format!("Transfer issue {number}"))
                    .execute(&pool)
                    .await
                    .unwrap()
                    .last_insert_id();
            issues.push(u32::try_from(id).unwrap());
        }
        let root = std::env::temp_dir().join(format!("lilly-transfer-integration-{suffix}"));
        transfer::private_directory(&root).await.unwrap();
        let media_path = root.join("media");
        let media_storage = crate::services::media::MediaStorage::new(&media_path);
        media_storage.prepare().await.unwrap();
        let ledger_path = root.join("ledger");
        transfer::write_private(&ledger_path, b"").await.unwrap();
        let mut config = transfer::TransferConfig::for_tests();
        config.path = root.join("transfer");
        let state = AppState {
            inner: Arc::new(AppStateInner {
                pool,
                jwt_secret: "transfer-integration-test-secret".to_string(),
                jwt_access_expiry: 900,
                jwt_refresh_expiry: 2_592_000,
                password_reset_ttl_seconds: 3600,
                email_service: crate::services::email::EmailService::Log {
                    from: "test@lilly.app".to_string(),
                },
                app_base_url: "http://localhost".to_string(),
                cookie_secure: false,
                oauth_service: crate::services::oauth::OAuthService::disabled(),
                privacy_policy_version: "test-v1".to_string(),
                adapter_registry: lilly_importer_core::AdapterRegistry::new(),
                media_path,
                media_url_prefix: "/media".to_string(),
                photo_upload_config: crate::config::PhotoUploadConfig::default(),
                media_storage,
                erasure_ledger: crate::services::account_erasure::ErasureLedger::new(ledger_path),
                import_scheduler_config: crate::services::import_scheduler::ImportSchedulerConfig {
                    enabled: false,
                    schedule: "0 10 6 * * Sat *".to_string(),
                    timezone: "Europe/Berlin".to_string(),
                    adapters: vec![],
                },
                request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
                transfer_config: config,
            }),
        };
        Self {
            state,
            owner: users[0],
            other: users[1],
            admin: users[2],
            series,
            issues,
            slug,
            root,
        }
    }

    fn entry(&self, key: &str, number: u32) -> TransferEntry {
        TransferEntry {
            copy_key: key.to_string(),
            series_slug: self.slug.clone(),
            issue_number: number,
            ..format::template_entry()
        }
    }

    async fn import(
        &self,
        entries: &[TransferEntry],
        kind: &str,
    ) -> crate::models::collection_transfer::TransferJob {
        let bytes = format::encode_document(entries, kind, "now").unwrap();
        let job = transfer::preview(
            &self.state.inner,
            self.owner,
            &bytes,
            kind,
            Utc::now().naive_utc(),
        )
        .await
        .unwrap();
        transfer::commit_import(
            &self.state.inner.pool,
            &job.id,
            self.owner,
            Utc::now().naive_utc(),
        )
        .await
        .unwrap();
        assert!(transfer::process_next(&self.state.inner).await.unwrap());
        db::find_job(&self.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
    }

    async fn request(
        &self,
        user: Option<u32>,
        method: &str,
        uri: &str,
        body: Body,
        stale: bool,
        content_type: Option<&str>,
    ) -> axum::response::Response {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header("origin", "http://localhost");
        if let Some(user_id) = user {
            let auth_time = usize::try_from(
                (Utc::now() - Duration::seconds(if stale { 601 } else { 0 })).timestamp(),
            )
            .unwrap();
            let version: u32 = sqlx::query_scalar("SELECT session_version FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_one(&self.state.inner.pool)
                .await
                .unwrap();
            let token = crate::auth::jwt::create_token_with_auth(
                user_id,
                "Transfer",
                if user_id == self.admin {
                    "admin"
                } else {
                    "user"
                },
                &self.state.inner.jwt_secret,
                900,
                auth_time,
                version,
                false,
            )
            .unwrap();
            request = request.header("cookie", format!("access_token={token}"));
        }
        if let Some(content_type) = content_type {
            request = request.header("content-type", content_type);
        }
        crate::api::application(self.state.clone(), false)
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap()
    }

    async fn export(&self) -> crate::models::collection_transfer::TransferJob {
        let response = self
            .request(
                Some(self.owner),
                "POST",
                "/api/v1/me/data-exports",
                Body::empty(),
                false,
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = value(response).await;
        let id = body["id"].as_str().unwrap();
        assert!(transfer::process_next(&self.state.inner).await.unwrap());
        let job = db::find_job(&self.state.inner.pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.status, "ready", "error: {:?}", job.error_code);
        job
    }

    async fn finish(self) {
        sqlx::query(
            "DELETE FROM trades WHERE initiator_id IN (?, ?, ?) OR responder_id IN (?, ?, ?)",
        )
        .bind(self.owner)
        .bind(self.other)
        .bind(self.admin)
        .bind(self.owner)
        .bind(self.other)
        .bind(self.admin)
        .execute(&self.state.inner.pool)
        .await
        .unwrap();
        for id in [self.owner, self.other, self.admin] {
            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(id)
                .execute(&self.state.inner.pool)
                .await
                .unwrap();
        }
        sqlx::query("DELETE FROM series_publication_events WHERE series_id = ?")
            .bind(self.series)
            .execute(&self.state.inner.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(self.series)
            .execute(&self.state.inner.pool)
            .await
            .unwrap();
        transfer::cleanup(
            &self.state.inner,
            Utc::now().naive_utc() + Duration::days(8),
        )
        .await
        .unwrap();
        assert!(self.root.starts_with(std::env::temp_dir()));
        tokio::fs::remove_dir_all(&self.root).await.unwrap();
    }
}

async fn create_thread(fixture: &Fixture, first: u32, second: u32) -> (u32, u32) {
    let pool = &fixture.state.inner.pool;
    let trade = u32::try_from(sqlx::query("INSERT INTO trades (initiator_id, responder_id, status, cancelled_at) VALUES (?, ?, 'cancelled', CURRENT_TIMESTAMP)")
        .bind(first).bind(second).execute(pool).await.unwrap().last_insert_id()).unwrap();
    let thread = u32::try_from(
        sqlx::query("INSERT INTO message_threads (trade_id) VALUES (?)")
            .bind(trade)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_id(),
    )
    .unwrap();
    (trade, thread)
}

fn archive_json(fixture: &Fixture, id: &str, name: &str) -> Value {
    let file = std::fs::File::open(
        fixture
            .state
            .inner
            .transfer_config
            .job_path(id)
            .unwrap()
            .join("archive.zip"),
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    serde_json::from_reader(archive.by_name(name).unwrap()).unwrap()
}

#[tokio::test]
async fn all_visible_messages_and_audit_data_export_without_other_accounts_private_fields() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let (trade, thread) = create_thread(&fixture, fixture.owner, fixture.other).await;
    let (_, unrelated) = create_thread(&fixture, fixture.other, fixture.admin).await;
    for number in 0..81 {
        sqlx::query("INSERT INTO messages (thread_id, sender_id, client_message_id, content) VALUES (?, ?, ?, ?)")
            .bind(thread).bind(if number % 2 == 0 { fixture.owner } else { fixture.other })
            .bind(format!("00000000-0000-4000-8000-{number:012}"))
            .bind(format!("Visible message {number}")).execute(&fixture.state.inner.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO messages (thread_id, sender_id, client_message_id, content) VALUES (?, ?, '00000000-0000-4000-8000-000000000001', 'UNRELATED-PRIVATE-MESSAGE')")
        .bind(unrelated).bind(fixture.admin).execute(&fixture.state.inner.pool).await.unwrap();
    sqlx::query("INSERT INTO trade_items (trade_id, issue_id, offered_by_user_id, receiving_user_id, copy_number_snapshot, condition_grade_snapshot, edition_label_snapshot) VALUES (?, ?, ?, ?, 1, 'Z1', 'Shared edition')")
        .bind(trade).bind(fixture.issues[0]).bind(fixture.other).bind(fixture.owner).execute(&fixture.state.inner.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO trade_completion_confirmations (trade_id, user_id) VALUES (?, ?), (?, ?)",
    )
    .bind(trade)
    .bind(fixture.owner)
    .bind(trade)
    .bind(fixture.other)
    .execute(&fixture.state.inner.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO notifications (user_id, actor_user_id, kind, trade_id, dedupe_key, payload) VALUES (?, ?, 'trade_message', ?, 'transfer-test-notification', JSON_OBJECT('thread_id', ?))")
        .bind(fixture.owner).bind(fixture.other).bind(trade).bind(thread).execute(&fixture.state.inner.pool).await.unwrap();
    sqlx::query("INSERT INTO role_change_events (target_user_id, previous_role, new_role, method) VALUES (?, 'user', 'admin', 'cli')")
        .bind(fixture.owner).execute(&fixture.state.inner.pool).await.unwrap();
    sqlx::query("INSERT INTO series_publication_events (series_id, actor_user_id, action, decision) VALUES (?, ?, 'activated', 'clean')")
        .bind(fixture.series).bind(fixture.owner).execute(&fixture.state.inner.pool).await.unwrap();
    let job = fixture.export().await;
    let messages = archive_json(&fixture, &job.id, "data/messages.json");
    assert_eq!(messages.as_array().unwrap().len(), 81);
    assert_eq!(messages[80]["content"], "Visible message 80");
    assert_eq!(messages[1]["is_mine"], false);
    assert!(!messages.to_string().contains("UNRELATED-PRIVATE"));
    let trades = archive_json(&fixture, &job.id, "data/trades.json");
    assert_eq!(trades.as_array().unwrap().len(), 1);
    assert!(!trades.to_string().contains("PRIVATE-other-location"));
    assert!(!trades.to_string().contains("example.test"));
    for name in [
        "trade_items",
        "confirmations",
        "notifications",
        "role_changes",
        "publication_activity",
    ] {
        assert!(
            !archive_json(&fixture, &job.id, &format!("data/{name}.json"))
                .as_array()
                .unwrap()
                .is_empty(),
            "{name}"
        );
    }
    fixture.finish().await;
}

#[tokio::test]
async fn export_covers_session_offline_matching_and_catalogue_actor_metadata() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture
        .import(&[fixture.entry("audit-copy", 3)], "json")
        .await;
    let pool = &fixture.state.inner.pool;
    for (user, issue, status) in [
        (fixture.owner, fixture.issues[0], "duplicate"),
        (fixture.other, fixture.issues[0], "wanted"),
        (fixture.other, fixture.issues[1], "duplicate"),
        (fixture.owner, fixture.issues[1], "wanted"),
    ] {
        sqlx::query("INSERT INTO collection_entries (user_id, issue_id, copy_number, status, condition_grade, edition_label) VALUES (?, ?, 1, ?, 'Z2', 'Audit edition')")
            .bind(user).bind(issue).bind(status).execute(pool).await.unwrap();
    }
    crate::services::trade_matching::reconcile_user(pool, fixture.owner)
        .await
        .unwrap();
    for user in [fixture.owner, fixture.other] {
        sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at, authenticated_at) VALUES (?, ?, DATE_ADD(CURRENT_TIMESTAMP, INTERVAL 1 DAY), CURRENT_TIMESTAMP(6))")
            .bind(user).bind(format!("SECRET-SESSION-HASH-{user}")).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO collection_mutation_receipts (user_id, mutation_id, operation, request_fingerprint, result_json) VALUES (?, '00000000-0000-4000-8000-000000000001', 'create', REPEAT('f', 64), JSON_OBJECT('status', 'applied'))")
            .bind(user).execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO import_jobs (series_id, adapter_name, status, started_by, completed_at) VALUES (?, 'test-adapter', 'completed', ?, CURRENT_TIMESTAMP)")
        .bind(fixture.series).bind(fixture.owner).execute(pool).await.unwrap();
    let job = fixture.export().await;
    for (name, count) in [
        ("sessions", 1),
        ("offline_receipts", 1),
        ("matches", 1),
        ("match_items", 2),
        ("wiki_import_activity", 1),
        ("collection_metadata", 3),
        ("copy_keys", 3),
        ("import_results", 1),
    ] {
        let data = archive_json(&fixture, &job.id, &format!("data/{name}.json"));
        assert_eq!(data.as_array().unwrap().len(), count, "{name}");
        assert!(!data.to_string().contains("SECRET-SESSION-HASH"));
        assert!(!data.to_string().contains("request_fingerprint"));
        assert!(!data.to_string().contains("PRIVATE-other-location"));
    }
    let manifest = archive_json(&fixture, &job.id, "manifest.json");
    assert_eq!(manifest["counts"]["matches"], 1);
    assert_eq!(manifest["counts"]["match_items"], 2);
    assert_eq!(
        manifest["not_implemented"],
        serde_json::json!(["comments", "ratings"])
    );
    // The configured providers are disabled even when an identity remains linked.
    sqlx::query("INSERT INTO oauth_identities (user_id, provider, provider_subject) VALUES (?, 'github', 'disabled-provider-test')")
        .bind(fixture.owner).execute(pool).await.unwrap();
    let options = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                "/api/v1/me/reauthentication/options",
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(options["github"], false);
    fixture.finish().await;
}

#[tokio::test]
async fn partner_deletion_invalidates_ready_archives_and_new_exports_keep_anonymized_history() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let (_, thread) = create_thread(&fixture, fixture.owner, fixture.other).await;
    sqlx::query("INSERT INTO messages (thread_id, sender_id, client_message_id, content) VALUES (?, ?, '00000000-0000-4000-8000-000000000001', 'Partner text before erasure')")
        .bind(thread).bind(fixture.other).execute(&fixture.state.inner.pool).await.unwrap();
    let old = fixture.export().await;
    crate::services::account_erasure::schedule(
        &fixture.state.inner.pool,
        fixture.other,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let cancelled = db::find_job(&fixture.state.inner.pool, &old.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let uri = format!("/api/v1/me/data-exports/{}/download", old.id);
    assert_eq!(
        fixture
            .request(Some(fixture.owner), "GET", &uri, Body::empty(), false, None)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE account_erasure_jobs SET scheduled_for = ? WHERE user_id = ?")
        .bind(Utc::now().naive_utc() - Duration::seconds(1))
        .bind(fixture.other)
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    crate::services::account_erasure::process_due_jobs(&fixture.state.inner)
        .await
        .unwrap();
    let new = fixture.export().await;
    let messages = archive_json(&fixture, &new.id, "data/messages.json");
    assert!(!messages.to_string().contains("Partner text before erasure"));
    assert!(messages.to_string().contains("entfernt"));
    let trades = archive_json(&fixture, &new.id, "data/trades.json");
    assert!(trades.to_string().contains("Gelöschtes Konto"));
    assert!(!trades.to_string().contains("Transfer other"));
    assert!(
        !fixture
            .state
            .inner
            .transfer_config
            .job_path(&old.id)
            .unwrap()
            .exists()
    );
    // The erasure worker already removed the partner account.
    fixture.finish().await;
}

#[tokio::test]
async fn own_erasure_waits_for_transfer_files_and_removes_import_results() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let import = fixture
        .import(&[fixture.entry("private-copy", 1)], "json")
        .await;
    let export = fixture.export().await;
    crate::services::account_erasure::schedule(
        &fixture.state.inner.pool,
        fixture.owner,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE account_erasure_jobs SET scheduled_for = ? WHERE user_id = ?")
        .bind(Utc::now().naive_utc() - Duration::seconds(1))
        .bind(fixture.owner)
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    crate::services::account_erasure::process_due_jobs(&fixture.state.inner)
        .await
        .unwrap();
    assert!(
        !fixture
            .state
            .inner
            .transfer_config
            .job_path(&export.id)
            .unwrap()
            .exists()
    );
    let results: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM collection_import_results WHERE job_id = ?")
            .bind(import.id)
            .fetch_one(&fixture.state.inner.pool)
            .await
            .unwrap();
    assert_eq!(results, 0);
    let status: String = sqlx::query_scalar("SELECT status FROM account_erasure_jobs WHERE id = (SELECT erasure_job_id FROM transfer_jobs WHERE id = ?)")
        .bind(&export.id).fetch_one(&fixture.state.inner.pool).await.unwrap();
    assert_eq!(status, "completed");
    assert!(
        db::find_job(&fixture.state.inner.pool, &export.id)
            .await
            .unwrap()
            .unwrap()
            .files_deleted
    );
    fixture.finish().await;
}

#[tokio::test]
async fn recovery_preserves_valid_archives_and_rejects_missing_files_and_interrupted_jobs() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let job = fixture.export().await;
    let orphan = fixture
        .state
        .inner
        .transfer_config
        .job_path(&transfer::random_id())
        .unwrap();
    transfer::private_directory(&orphan).await.unwrap();
    transfer::write_private(&orphan.join("archive.partial"), b"orphan after restore")
        .await
        .unwrap();
    transfer::recover(&fixture.state.inner).await.unwrap();
    assert!(!orphan.exists());
    assert_eq!(
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .expires_at,
        job.expires_at
    );
    tokio::fs::remove_file(
        fixture
            .state
            .inner
            .transfer_config
            .job_path(&job.id)
            .unwrap()
            .join("archive.zip"),
    )
    .await
    .unwrap();
    transfer::recover(&fixture.state.inner).await.unwrap();
    assert_eq!(
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .error_code
            .as_deref(),
        Some("stored_file_missing")
    );
    let interrupted = transfer::new_job(fixture.owner, "export", "zip", Utc::now().naive_utc());
    db::create_job(&fixture.state.inner.pool, &interrupted, 100)
        .await
        .unwrap();
    sqlx::query("UPDATE transfer_jobs SET status = 'running' WHERE id = ?")
        .bind(&interrupted.id)
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    transfer::recover(&fixture.state.inner).await.unwrap();
    assert_eq!(
        db::find_job(&fixture.state.inner.pool, &interrupted.id)
            .await
            .unwrap()
            .unwrap()
            .error_code
            .as_deref(),
        Some("worker_interrupted")
    );
    let source =
        format::encode_document(&[fixture.entry("interrupted-import", 1)], "json", "now").unwrap();
    let import = transfer::preview(
        &fixture.state.inner,
        fixture.owner,
        &source,
        "json",
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE transfer_jobs SET status = 'running' WHERE id = ?")
        .bind(&import.id)
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    transfer::recover(&fixture.state.inner).await.unwrap();
    let import = db::find_job(&fixture.state.inner.pool, &import.id)
        .await
        .unwrap()
        .unwrap();
    assert!(import.completed_at.is_some());
    let response = db::response(&fixture.state.inner.pool, import, 1, Utc::now().naive_utc())
        .await
        .unwrap();
    assert_eq!(response.counts.failed, 1);
    assert_eq!(response.counts.new, 0);
    assert_eq!(response.results[0].code.as_deref(), Some("job_interrupted"));
    fixture.finish().await;
}

#[tokio::test]
async fn quotas_inactive_catalogues_source_mismatches_and_pending_rows_are_reported() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let now = Utc::now().naive_utc();
    let mut source = fixture.entry("source", 2);
    source.source_key = Some("wrong".to_string());
    source.source_record_id = Some("wrong".to_string());
    let bytes = format::encode_document(
        &[
            fixture.entry("valid", 1),
            source,
            fixture.entry("unknown", 999),
        ],
        "json",
        "now",
    )
    .unwrap();
    let preview = transfer::preview(&fixture.state.inner, fixture.owner, &bytes, "json", now)
        .await
        .unwrap();
    let response = db::response(&fixture.state.inner.pool, preview.clone(), 1, now)
        .await
        .unwrap();
    assert_eq!((response.counts.new, response.counts.invalid), (1, 2));
    assert_eq!(
        response.results[1].code.as_deref(),
        Some("source_identity_mismatch")
    );
    assert!(
        db::create_job(
            &fixture.state.inner.pool,
            &transfer::new_job(fixture.owner, "export", "zip", now),
            1
        )
        .await
        .is_err()
    );
    sqlx::query("UPDATE series SET active = FALSE WHERE id = ?")
        .bind(fixture.series)
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    transfer::commit_import(&fixture.state.inner.pool, &preview.id, fixture.owner, now)
        .await
        .unwrap();
    transfer::process_next(&fixture.state.inner).await.unwrap();
    let result = db::response(
        &fixture.state.inner.pool,
        db::find_job(&fixture.state.inner.pool, &preview.id)
            .await
            .unwrap()
            .unwrap(),
        1,
        now,
    )
    .await
    .unwrap();
    assert_eq!(result.counts.invalid, 3);
    assert_eq!(result.results[0].code.as_deref(), Some("series_inactive"));
    fixture.finish().await;
}

#[tokio::test]
async fn failed_import_reports_partial_results_and_retry_never_duplicates_committed_rows() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    sqlx::query("CREATE TRIGGER transfer_test_fail BEFORE INSERT ON collection_entries FOR EACH ROW BEGIN IF NEW.notes = 'inject-transfer-failure' THEN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'Injected transfer test failure'; END IF; END")
        .execute(&fixture.state.inner.pool).await.unwrap();
    let first = fixture.entry("first-row", 1);
    let mut second = fixture.entry("second-row", 2);
    second.notes = Some("inject-transfer-failure".to_string());
    let job = fixture
        .import(&[first.clone(), second.clone()], "json")
        .await;
    sqlx::query("DROP TRIGGER transfer_test_fail")
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(job.status, "failed");
    let results = db::response(&fixture.state.inner.pool, job, 1, Utc::now().naive_utc())
        .await
        .unwrap();
    assert_eq!((results.counts.created, results.counts.failed), (1, 1));
    let retry = fixture.import(&[first, second], "csv").await;
    let results = db::response(&fixture.state.inner.pool, retry, 1, Utc::now().naive_utc())
        .await
        .unwrap();
    assert_eq!((results.counts.created, results.counts.unchanged), (1, 1));
    fixture.finish().await;
}

#[tokio::test]
async fn parallel_import_requests_allocate_one_copy_and_confirmation_is_idempotent() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let bytes = format::encode_document(&[fixture.entry("parallel", 1)], "json", "now").unwrap();
    let job = transfer::preview(
        &fixture.state.inner,
        fixture.owner,
        &bytes,
        "json",
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let (first, second) = tokio::join!(
        transfer::commit_import(
            &fixture.state.inner.pool,
            &job.id,
            fixture.owner,
            Utc::now().naive_utc()
        ),
        transfer::commit_import(
            &fixture.state.inner.pool,
            &job.id,
            fixture.owner,
            Utc::now().naive_utc()
        )
    );
    first.unwrap();
    second.unwrap();
    let (first, second) = tokio::join!(
        transfer::process_next(&fixture.state.inner),
        transfer::process_next(&fixture.state.inner)
    );
    assert_ne!(first.unwrap(), second.unwrap());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM collection_entries WHERE user_id = ?")
            .bind(fixture.owner)
            .fetch_one(&fixture.state.inner.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let job = db::find_job(&fixture.state.inner.pool, &job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, "completed");
    fixture.finish().await;
}

#[tokio::test]
async fn cleanup_retries_failed_file_removal_and_eventually_expires_metadata() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let now = Utc::now().naive_utc();
    let job = transfer::new_job(fixture.owner, "export", "zip", now);
    db::create_job(&fixture.state.inner.pool, &job, 100)
        .await
        .unwrap();
    transfer::mark_failed(&fixture.state.inner.pool, &job.id, "injected_failure")
        .await
        .unwrap();
    fixture.state.inner.transfer_config.prepare().await.unwrap();
    let path = fixture
        .state
        .inner
        .transfer_config
        .job_path(&job.id)
        .unwrap();
    transfer::write_private(&path, b"blocks directory removal")
        .await
        .unwrap();
    transfer::cleanup(&fixture.state.inner, now).await.unwrap();
    assert!(
        !db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .files_deleted
    );
    tokio::fs::remove_file(&path).await.unwrap();
    transfer::private_directory(&path).await.unwrap();
    transfer::cleanup(&fixture.state.inner, now).await.unwrap();
    assert!(
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .files_deleted
    );
    transfer::cleanup(&fixture.state.inner, now + Duration::days(8))
        .await
        .unwrap();
    assert!(
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .is_none()
    );
    fixture.finish().await;
}

#[tokio::test]
async fn cleanup_waits_until_a_detached_file_producer_finishes() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let now = Utc::now().naive_utc();
    let job = transfer::new_job(fixture.owner, "export", "zip", now);
    db::create_job(&fixture.state.inner.pool, &job, 100)
        .await
        .unwrap();
    transfer::mark_failed(&fixture.state.inner.pool, &job.id, "cancelled_writer")
        .await
        .unwrap();
    let path = fixture
        .state
        .inner
        .transfer_config
        .job_path(&job.id)
        .unwrap();
    transfer::private_directory(&path).await.unwrap();
    let lease = fixture.state.inner.transfer_config.lease(&job.id);
    let (release, wait) = tokio::sync::oneshot::channel();
    let (done, finished) = tokio::sync::oneshot::channel();
    let producer_path = path.clone();
    // Dropping the caller's task handle must leave its owned writer protected.
    drop(tokio::spawn(async move {
        wait.await.unwrap();
        transfer::write_private(&producer_path.join("late-file"), b"private")
            .await
            .unwrap();
        drop(lease);
        done.send(()).unwrap();
    }));
    transfer::cleanup(&fixture.state.inner, now).await.unwrap();
    assert!(path.exists());
    assert!(
        !db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .files_deleted
    );
    release.send(()).unwrap();
    finished.await.unwrap();
    transfer::cleanup(&fixture.state.inner, now).await.unwrap();
    assert!(!path.exists());
    assert!(
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .files_deleted
    );
    fixture.finish().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn editions_keep_photos_status_notes_and_progress_independent() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let first = fixture.entry("photo-first", 1);
    let mut second = fixture.entry("photo-second", 1);
    second.copy_number = 2;
    second.edition_label = Some("Variant edition".to_string());
    fixture.import(&[first, second], "json").await;
    let ids: Vec<u32> = sqlx::query_scalar(
        "SELECT id FROM collection_entries WHERE user_id = ? ORDER BY copy_number",
    )
    .bind(fixture.owner)
    .fetch_all(&fixture.state.inner.pool)
    .await
    .unwrap();
    let mut photos = Vec::new();
    for (id, bytes) in ids
        .iter()
        .zip([b"first-photo".as_slice(), b"second-photo".as_slice()])
    {
        let staged = fixture
            .state
            .inner
            .media_storage
            .stage(bytes)
            .await
            .unwrap();
        fixture
            .state
            .inner
            .media_storage
            .commit(&staged)
            .await
            .unwrap();
        sqlx::query("INSERT INTO collection_photos (entry_id, storage_key, media_type, byte_size, width, height, sort_order) VALUES (?, ?, 'image/jpeg', ?, 1, 1, 0)")
            .bind(id).bind(&staged.storage_key).bind(u32::try_from(bytes.len()).unwrap()).execute(&fixture.state.inner.pool).await.unwrap();
        photos.push(staged.storage_key);
    }
    let stats = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                "/api/v1/me/collection/stats",
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(stats["total_physical_owned"], 2);
    assert_eq!(stats["total_owned"], 1);
    let patch = fixture
        .request(
            Some(fixture.owner),
            "PATCH",
            &format!("/api/v1/me/collection/{}", ids[0]),
            Body::from(
                r#"{"condition_grade":"Z4","status":"duplicate","notes":"First copy only"}"#,
            ),
            false,
            Some("application/json"),
        )
        .await;
    assert_eq!(patch.status(), StatusCode::OK);
    let other = crate::db::collection::find_entry_by_id_and_user(
        &fixture.state.inner.pool,
        ids[1],
        fixture.owner,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(other.status, "owned");
    assert_eq!(other.condition_grade.as_deref(), Some("Z2"));
    assert!(other.notes.is_none());
    let deleted = fixture
        .request(
            Some(fixture.owner),
            "DELETE",
            &format!("/api/v1/me/collection/{}", ids[0]),
            Body::empty(),
            false,
            None,
        )
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        fixture
            .state
            .inner
            .media_storage
            .read(&photos[1])
            .await
            .unwrap(),
        b"second-photo"
    );
    assert!(
        fixture
            .state
            .inner
            .media_storage
            .read(&photos[0])
            .await
            .is_err()
    );
    fixture.finish().await;
}

async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 20 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn imports_preserve_editions_repeat_across_formats_and_tombstone_deleted_copies() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let first = fixture.entry("first", 1);
    let mut second = fixture.entry("second", 1);
    second.copy_number = 2;
    second.edition_label = Some("Variant".to_string());
    second.notes = Some("Private second copy".to_string());
    let job = fixture
        .import(&[first.clone(), second.clone()], "csv")
        .await;
    assert_eq!(job.status, "completed");
    let response = db::response(&fixture.state.inner.pool, job, 1, Utc::now().naive_utc())
        .await
        .unwrap();
    assert_eq!(response.counts.created, 2);
    let repeated = fixture
        .import(&[second.clone(), first.clone()], "json")
        .await;
    assert_eq!(
        db::response(
            &fixture.state.inner.pool,
            repeated,
            1,
            Utc::now().naive_utc()
        )
        .await
        .unwrap()
        .counts
        .unchanged,
        2
    );
    let mut changed = first.clone();
    changed.notes = Some("Do not overwrite".to_string());
    let result = fixture
        .import(&[changed, fixture.entry("third", 2)], "csv")
        .await;
    let result = db::response(&fixture.state.inner.pool, result, 1, Utc::now().naive_utc())
        .await
        .unwrap();
    assert_eq!((result.counts.conflict, result.counts.created), (1, 1));
    sqlx::query(
        "DELETE FROM collection_entries WHERE user_id = ? AND issue_id = ? AND copy_number = 1",
    )
    .bind(fixture.owner)
    .bind(fixture.issues[0])
    .execute(&fixture.state.inner.pool)
    .await
    .unwrap();
    let removed = fixture.import(&[first], "json").await;
    let removed = db::response(
        &fixture.state.inner.pool,
        removed,
        1,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    assert_eq!(removed.results[0].code.as_deref(), Some("entry_removed"));
    let rows = db::export_entries(
        &mut fixture.state.inner.pool.acquire().await.unwrap(),
        fixture.owner,
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|entry| entry
        == &TransferEntry {
            series_name: Some(fixture.slug.clone()),
            title: Some("Transfer issue 1".to_string()),
            ..second.clone()
        }));
    fixture.finish().await;
}

#[tokio::test]
async fn preview_is_non_mutating_expiring_paginated_and_rechecks_changed_collection() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let bytes = format::encode_document(&[fixture.entry("original", 1)], "json", "now").unwrap();
    let now = Utc::now().naive_utc();
    let job = transfer::preview(&fixture.state.inner, fixture.owner, &bytes, "json", now)
        .await
        .unwrap();
    assert_eq!(
        db::export_entries(
            &mut fixture.state.inner.pool.acquire().await.unwrap(),
            fixture.owner
        )
        .await
        .unwrap()
        .len(),
        0
    );
    assert!(
        db::response(&fixture.state.inner.pool, job.clone(), 2, now)
            .await
            .unwrap()
            .results
            .is_empty()
    );
    assert!(
        transfer::commit_import(
            &fixture.state.inner.pool,
            &job.id,
            fixture.owner,
            now + Duration::minutes(30)
        )
        .await
        .is_err()
    );
    assert!(
        transfer::commit_import(&fixture.state.inner.pool, &job.id, fixture.other, now)
            .await
            .is_err()
    );
    sqlx::query("INSERT INTO collection_entries (user_id, issue_id, copy_number, status, condition_grade) VALUES (?, ?, 1, 'owned', 'Z1')").bind(fixture.owner).bind(fixture.issues[0]).execute(&fixture.state.inner.pool).await.unwrap();
    transfer::commit_import(&fixture.state.inner.pool, &job.id, fixture.owner, now)
        .await
        .unwrap();
    transfer::commit_import(&fixture.state.inner.pool, &job.id, fixture.owner, now)
        .await
        .unwrap();
    transfer::process_next(&fixture.state.inner).await.unwrap();
    let response = db::response(
        &fixture.state.inner.pool,
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap(),
        1,
        now,
    )
    .await
    .unwrap();
    assert_eq!(response.counts.conflict, 1);
    assert_eq!(
        response.results[0].code.as_deref(),
        Some("copy_number_occupied")
    );
    fixture.finish().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn upload_and_download_routes_enforce_owner_recent_auth_and_supported_formats() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    for path in [
        "/api/v1/me/collection/export",
        "/api/v1/me/collection/import/template",
        "/api/v1/me/data-exports",
        "/api/v1/me/reauthentication/options",
    ] {
        assert_eq!(
            fixture
                .request(None, "GET", path, Body::empty(), false, None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        fixture
            .request(
                Some(fixture.owner),
                "POST",
                "/api/v1/me/data-exports",
                Body::empty(),
                true,
                None
            )
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                "/api/v1/me/collection/export?format=xml",
                Body::empty(),
                false,
                None
            )
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let bytes = format::encode_document(&[fixture.entry("uploaded", 1)], "json", "now").unwrap();
    let multipart = format!(
        "--test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"collection.json\"\r\nContent-Type: application/json\r\n\r\n{}\r\n--test-boundary--\r\n",
        String::from_utf8(bytes).unwrap()
    );
    let response = fixture
        .request(
            Some(fixture.owner),
            "POST",
            "/api/v1/me/collection/imports/preview?format=json",
            Body::from(multipart),
            false,
            Some("multipart/form-data; boundary=test-boundary"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = value(response).await;
    let id = body["id"].as_str().unwrap();
    for user in [fixture.other, fixture.admin] {
        assert_eq!(
            fixture
                .request(
                    Some(user),
                    "GET",
                    &format!("/api/v1/me/collection/imports/{id}"),
                    Body::empty(),
                    false,
                    None
                )
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            fixture
                .request(
                    Some(user),
                    "POST",
                    &format!("/api/v1/me/collection/imports/{id}/commit"),
                    Body::empty(),
                    false,
                    None
                )
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    let options = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                "/api/v1/me/reauthentication/options",
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(options["recent_authentication"], true);
    fixture.finish().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn transfer_routes_download_templates_confirm_imports_and_list_private_exports() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture
        .import(&[fixture.entry("existing", 1)], "json")
        .await;
    for kind in ["csv", "json"] {
        for endpoint in ["export", "import/template"] {
            let response = fixture
                .request(
                    Some(fixture.owner),
                    "GET",
                    &format!("/api/v1/me/collection/{endpoint}?format={kind}"),
                    Body::empty(),
                    false,
                    None,
                )
                .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert!(
                response.headers()["content-disposition"]
                    .to_str()
                    .unwrap()
                    .ends_with(&format!(".{kind}\""))
            );
            let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
            assert_eq!(
                format::parse_document(&bytes, kind, 100_000, 100)
                    .unwrap()
                    .len(),
                1
            );
        }
    }
    let entry = fixture.entry("route-preview", 2);
    let mut changed = fixture.entry("file-conflict", 3);
    let original = changed.clone();
    changed.notes = Some("Conflicting file row".to_string());
    let source = serde_json::to_vec(
        &serde_json::json!({"format":"lilly.collection", "format_version":1,
        "entries":[entry, entry, original, changed, {"copy_key":"invalid"}]}),
    )
    .unwrap();
    let multipart = format!(
        "--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"source.json\"\r\n\r\n{}\r\n--boundary--\r\n",
        String::from_utf8(source.clone()).unwrap()
    );
    let preview = value(
        fixture
            .request(
                Some(fixture.owner),
                "POST",
                "/api/v1/me/collection/imports/preview?format=json",
                Body::from(multipart),
                false,
                Some("multipart/form-data; boundary=boundary"),
            )
            .await,
    )
    .await;
    assert_eq!(preview["counts"]["new"], 1);
    assert_eq!(preview["counts"]["unchanged"], 1);
    assert_eq!(preview["counts"]["conflict"], 2);
    assert_eq!(preview["counts"]["invalid"], 1);
    let id = preview["id"].as_str().unwrap();
    let status = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                &format!("/api/v1/me/collection/imports/{id}?page=0"),
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(status["page"], 1);
    assert_eq!(status["format"], "json");
    // A retained raw source is itself personal data and must accompany the ZIP.
    let export = fixture.export().await;
    let archive_path = fixture
        .state
        .inner
        .transfer_config
        .job_path(&export.id)
        .unwrap()
        .join("archive.zip");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&archive_path).unwrap()).unwrap();
    let mut actual = Vec::new();
    std::io::Read::read_to_end(
        &mut archive
            .by_name(&format!("files/imports/{id}.json"))
            .unwrap(),
        &mut actual,
    )
    .unwrap();
    assert_eq!(actual, source);
    drop(archive);
    for _ in 0..2 {
        let response = fixture
            .request(
                Some(fixture.owner),
                "POST",
                &format!("/api/v1/me/collection/imports/{id}/commit"),
                Body::empty(),
                false,
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
    transfer::process_next(&fixture.state.inner).await.unwrap();
    let status = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                &format!("/api/v1/me/collection/imports/{id}"),
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(status["status"], "completed");
    assert_eq!(status["counts"]["created"], 1);
    let list = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                "/api/v1/me/data-exports?page=0",
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], export.id);
    let empty = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                "/api/v1/me/data-exports?page=2",
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(empty.as_array().unwrap().len(), 0);
    let detail_url = format!("/api/v1/me/data-exports/{}", export.id);
    let detail = value(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                &detail_url,
                Body::empty(),
                false,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(detail["status"], "ready");
    assert_eq!(detail["format"], "zip");
    for user in [fixture.other, fixture.admin] {
        assert_eq!(
            fixture
                .request(Some(user), "GET", &detail_url, Body::empty(), false, None)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        let list = value(
            fixture
                .request(
                    Some(user),
                    "GET",
                    "/api/v1/me/data-exports",
                    Body::empty(),
                    false,
                    None,
                )
                .await,
        )
        .await;
        assert_eq!(list, serde_json::json!([]));
    }
    tokio::fs::write(&archive_path, b"incomplete")
        .await
        .unwrap();
    assert_eq!(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                &format!("{detail_url}/download"),
                Body::empty(),
                false,
                None
            )
            .await
            .status(),
        StatusCode::CONFLICT
    );
    tokio::fs::remove_file(&archive_path).await.unwrap();
    assert_eq!(
        fixture
            .request(
                Some(fixture.owner),
                "GET",
                &format!("{detail_url}/download"),
                Body::empty(),
                false,
                None
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    fixture.finish().await;
}

#[tokio::test]
async fn multipart_upload_rejects_missing_duplicate_unexpected_and_oversized_file_fields() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let mut fixture = Fixture::new().await;
    Arc::get_mut(&mut fixture.state.inner)
        .unwrap()
        .transfer_config
        .max_import_bytes = 8;
    let field = "--boundary\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\n{}\r\n";
    for (body, expected) in [
        ("--boundary--\r\n".to_string(), StatusCode::BAD_REQUEST),
        (
            format!(
                "{}--boundary--\r\n",
                field.replace("name=\"file\"", "name=\"other\"")
            ),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("{field}{field}--boundary--\r\n"),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("{}--boundary--\r\n", field.replace("{}", "0123456789")),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            "--boundary\r\ninvalid headers".to_string(),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = fixture
            .request(
                Some(fixture.owner),
                "POST",
                "/api/v1/me/collection/imports/preview?format=json",
                Body::from(body),
                false,
                Some("multipart/form-data; boundary=boundary"),
            )
            .await;
        assert_eq!(response.status(), expected);
    }
    let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer_jobs WHERE user_id = ?")
        .bind(fixture.owner)
        .fetch_one(&fixture.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
    fixture.finish().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn personal_export_is_complete_private_includes_real_media_and_expires_without_extension() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.import(&[fixture.entry("owned", 1)], "json").await;
    sqlx::query("INSERT INTO privacy_consents (user_id, policy_version, consented_at, registration_method) VALUES (?, 'test-v1', CURRENT_TIMESTAMP(6), 'password')").bind(fixture.owner).execute(&fixture.state.inner.pool).await.unwrap();
    sqlx::query("INSERT INTO oauth_identities (user_id, provider, provider_subject) VALUES (?, 'github', 'own-provider-subject')").bind(fixture.owner).execute(&fixture.state.inner.pool).await.unwrap();
    let entry: u32 = sqlx::query_scalar("SELECT id FROM collection_entries WHERE user_id = ?")
        .bind(fixture.owner)
        .fetch_one(&fixture.state.inner.pool)
        .await
        .unwrap();
    let photo = fixture
        .state
        .inner
        .media_storage
        .stage(b"private-photo-original")
        .await
        .unwrap();
    fixture
        .state
        .inner
        .media_storage
        .commit(&photo)
        .await
        .unwrap();
    sqlx::query("INSERT INTO collection_photos (entry_id, storage_key, media_type, byte_size, width, height, sort_order) VALUES (?, ?, 'image/jpeg', 22, 1, 1, 0)").bind(entry).bind(&photo.storage_key).execute(&fixture.state.inner.pool).await.unwrap();
    let avatar = fixture
        .state
        .inner
        .media_storage
        .stage(b"own-avatar-original")
        .await
        .unwrap();
    fixture
        .state
        .inner
        .media_storage
        .commit(&avatar)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET avatar_path = ? WHERE id = ?")
        .bind(&avatar.storage_key)
        .bind(fixture.owner)
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    let job = fixture.export().await;
    assert_eq!(
        (job.expires_at - job.completed_at.unwrap()).num_seconds(),
        86_400
    );
    let uri = format!("/api/v1/me/data-exports/{}/download", job.id);
    for user in [fixture.other, fixture.admin] {
        assert_eq!(
            fixture
                .request(Some(user), "GET", &uri, Body::empty(), false, None)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        fixture
            .request(Some(fixture.owner), "GET", &uri, Body::empty(), true, None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = fixture
        .request(Some(fixture.owner), "GET", &uri, Body::empty(), false, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut account = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("data/account.json").unwrap(), &mut account)
        .unwrap();
    assert!(account.contains("PRIVATE-owner-location"));
    assert!(!account.contains("PRIVATE-other-location"));
    assert!(!account.contains("password_hash"));
    assert!(!account.contains("erasure_subject"));
    let mut actual_avatar = Vec::new();
    std::io::Read::read_to_end(
        &mut zip.by_name("files/avatar.jpg").unwrap(),
        &mut actual_avatar,
    )
    .unwrap();
    assert_eq!(actual_avatar, b"own-avatar-original");
    assert!(
        zip.file_names()
            .any(|name| name.is_ok_and(|name| name.starts_with("files/photos/")))
    );
    assert_eq!(
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .expires_at,
        job.expires_at
    );
    transfer::cleanup(&fixture.state.inner, job.expires_at)
        .await
        .unwrap();
    assert!(
        !fixture
            .state
            .inner
            .transfer_config
            .job_path(&job.id)
            .unwrap()
            .exists()
    );
    assert_eq!(
        fixture
            .request(Some(fixture.owner), "GET", &uri, Body::empty(), false, None)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    drop(zip);
    fixture.finish().await;
}

#[tokio::test]
async fn missing_media_and_changed_sources_fail_without_publishing_partial_archives() {
    let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let bytes = format::encode_document(&[fixture.entry("tampered", 1)], "json", "now").unwrap();
    let job = transfer::preview(
        &fixture.state.inner,
        fixture.owner,
        &bytes,
        "json",
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    tokio::fs::write(
        fixture
            .state
            .inner
            .transfer_config
            .job_path(&job.id)
            .unwrap()
            .join("source"),
        b"changed",
    )
    .await
    .unwrap();
    transfer::commit_import(
        &fixture.state.inner.pool,
        &job.id,
        fixture.owner,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    transfer::process_next(&fixture.state.inner).await.unwrap();
    assert_eq!(
        db::find_job(&fixture.state.inner.pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
    sqlx::query("UPDATE users SET avatar_path = ? WHERE id = ?")
        .bind("a".repeat(64) + ".jpg")
        .bind(fixture.owner)
        .execute(&fixture.state.inner.pool)
        .await
        .unwrap();
    let export = transfer::new_job(fixture.owner, "export", "zip", Utc::now().naive_utc());
    db::create_job(&fixture.state.inner.pool, &export, 100)
        .await
        .unwrap();
    transfer::process_next(&fixture.state.inner).await.unwrap();
    assert_eq!(
        db::find_job(&fixture.state.inner.pool, &export.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
    assert!(
        !fixture
            .state
            .inner
            .transfer_config
            .job_path(&export.id)
            .unwrap()
            .join("archive.zip")
            .exists()
    );
    fixture.finish().await;
}
