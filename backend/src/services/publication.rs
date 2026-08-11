use std::collections::{BTreeSet, HashMap};

use lilly_importer_core::{ReferenceRecord, WikiAdapter};

use crate::db::{import_jobs, import_review, series};
use crate::error::AppError;
use crate::models::import_review::{
    ActivationEligibility, ActivationResponse, EligibilityReason, PublicationEvent, ReferenceCheck,
    ReviewItem, ReviewSummary,
};
use crate::routes::AppStateInner;

#[allow(clippy::too_many_lines)]
pub async fn evaluate_activation_eligibility(
    state: &AppStateInner,
    job_id: u32,
) -> Result<ReviewSummary, AppError> {
    let job = import_jobs::find_import_job_by_id(&state.pool, job_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Import job {job_id} not found")))?;
    let target_series = series::find_series_by_id(&state.pool, job.series_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series {} not found", job.series_id)))?;
    let outcomes = import_review::review_outcome_counts(&state.pool, job_id).await?;
    let (warning_count, blocking_count) =
        import_review::review_risk_counts(&state.pool, job_id).await?;
    let latest_job_id =
        import_review::latest_import_job_id_for_series(&state.pool, job.series_id).await?;
    let range_samples = import_review::range_sample_numbers(&state.pool, job_id).await?;

    let adapter = state.adapter_registry.get(&job.adapter_name);
    let references = adapter.map_or_else(Vec::new, WikiAdapter::reference_records);
    let reference_numbers = references
        .iter()
        .map(|reference| reference.issue_number)
        .collect::<Vec<_>>();
    let reference_items =
        import_review::find_review_items_by_numbers(&state.pool, job_id, &reference_numbers)
            .await?;
    let reference_checks = build_reference_checks(&references, &reference_items);

    let mut reasons = Vec::new();
    if target_series.active {
        reasons.push(reason(
            "series_already_active",
            "The series is already published",
        ));
    }
    if !matches!(job.status.as_str(), "completed" | "completed_with_errors") {
        reasons.push(reason(
            "job_not_completed",
            "The import job has not completed successfully",
        ));
    }
    if latest_job_id != Some(job_id) {
        reasons.push(reason(
            "newer_import_exists",
            "A newer import job exists for this series",
        ));
    }
    if outcomes.total == 0 && job.total_issues > 0 {
        reasons.push(reason(
            "review_data_unavailable",
            "This import predates persistent review results; run a new full import",
        ));
    } else if outcomes.total != job.total_issues || outcomes.not_processed > 0 {
        reasons.push(reason(
            "review_incomplete",
            "The persisted review results are incomplete",
        ));
    }
    if adapter.is_none() || references.is_empty() {
        reasons.push(reason(
            "reference_contract_unavailable",
            "No reference-record contract is available for this adapter",
        ));
    }
    if reference_checks
        .iter()
        .any(|check| check.status != "passed")
    {
        reasons.push(reason(
            "reference_check_failed",
            "At least one pinned reference record is missing or differs",
        ));
    }
    if blocking_count > 0 {
        reasons.push(reason(
            "blocking_findings",
            "The import contains blocking findings",
        ));
    }

    let sample_issue_numbers = range_samples
        .into_iter()
        .chain(reference_numbers)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let last_publication_event =
        import_review::find_last_publication_event(&state.pool, job.series_id).await?;

    Ok(ReviewSummary {
        job_id,
        series_id: job.series_id,
        series_name: target_series.name,
        series_slug: target_series.slug,
        series_active: target_series.active,
        job_status: job.status,
        outcomes,
        warning_count,
        blocking_count,
        eligibility: ActivationEligibility {
            eligible: reasons.is_empty(),
            requires_acknowledgement: reasons.is_empty() && warning_count > 0,
            reasons,
        },
        reference_checks,
        sample_issue_numbers,
        last_publication_event,
    })
}

fn reason(code: &str, message: &str) -> EligibilityReason {
    EligibilityReason {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn build_reference_checks(
    references: &[ReferenceRecord],
    items: &[ReviewItem],
) -> Vec<ReferenceCheck> {
    let items_by_number: HashMap<u32, &ReviewItem> =
        items.iter().map(|item| (item.issue_number, item)).collect();
    references
        .iter()
        .map(|reference| {
            let expected_authors = reference
                .authors
                .iter()
                .map(|author| (*author).to_string())
                .collect::<Vec<_>>();
            let item = items_by_number.get(&reference.issue_number);
            let passed = item.is_some_and(|item| {
                item.outcome != "failed"
                    && item.title.as_deref() == Some(reference.title)
                    && item.authors == expected_authors
                    && item.published_at == Some(reference.published_at)
            });
            ReferenceCheck {
                issue_number: reference.issue_number,
                expected_title: reference.title.to_string(),
                expected_authors,
                expected_published_at: reference.published_at,
                status: if passed { "passed" } else { "failed" }.to_string(),
                message: (!passed).then(|| {
                    if item.is_some() {
                        "The imported record differs from the pinned reference".to_string()
                    } else {
                        "The pinned reference is missing from this import".to_string()
                    }
                }),
            }
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
pub async fn activate_from_import(
    state: &AppStateInner,
    job_id: u32,
    actor_user_id: u32,
    acknowledge_warnings: bool,
) -> Result<ActivationResponse, AppError> {
    let summary = evaluate_activation_eligibility(state, job_id).await?;
    if summary.series_active {
        return Ok(ActivationResponse {
            series_id: summary.series_id,
            active: true,
            event: summary.last_publication_event,
        });
    }
    if let Some(reason) = summary.eligibility.reasons.first() {
        return Err(conflict(&reason.code, &reason.message));
    }
    if summary.eligibility.requires_acknowledgement && !acknowledge_warnings {
        return Err(conflict(
            "warning_acknowledgement_required",
            "The import has warnings that must be acknowledged",
        ));
    }

    let mut transaction = state.pool.begin().await?;
    let job: Option<(u32, String, u32)> = sqlx::query_as(
        "SELECT series_id, status, total_issues FROM import_jobs WHERE id = ? FOR UPDATE",
    )
    .bind(job_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let (series_id, status, total_issues) =
        job.ok_or_else(|| AppError::NotFound(format!("Import job {job_id} not found")))?;
    let (active,): (bool,) = sqlx::query_as("SELECT active FROM series WHERE id = ? FOR UPDATE")
        .bind(series_id)
        .fetch_one(&mut *transaction)
        .await?;
    if active {
        transaction.commit().await?;
        return Ok(ActivationResponse {
            series_id,
            active: true,
            event: import_review::find_last_publication_event(&state.pool, series_id).await?,
        });
    }
    if !matches!(status.as_str(), "completed" | "completed_with_errors") {
        return Err(conflict(
            "job_not_completed",
            "The import job is no longer eligible",
        ));
    }
    let (latest_job_id,): (Option<u32>,) =
        sqlx::query_as("SELECT MAX(id) FROM import_jobs WHERE series_id = ?")
            .bind(series_id)
            .fetch_one(&mut *transaction)
            .await?;
    if latest_job_id != Some(job_id) {
        return Err(conflict(
            "newer_import_exists",
            "A newer import job exists for this series",
        ));
    }
    let result_counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(CASE WHEN outcome = 'not_processed' THEN 1 END), \
         COUNT(CASE WHEN severity = 'warning' THEN 1 END), \
         COUNT(CASE WHEN severity = 'blocking' THEN 1 END) \
         FROM import_job_results WHERE job_id = ?",
    )
    .bind(job_id)
    .fetch_one(&mut *transaction)
    .await?;
    let job_findings: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(CASE WHEN severity = 'warning' AND issue_number IS NULL THEN 1 END), \
         COUNT(CASE WHEN severity = 'blocking' AND issue_number IS NULL THEN 1 END) \
         FROM import_job_errors WHERE job_id = ?",
    )
    .bind(job_id)
    .fetch_one(&mut *transaction)
    .await?;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let warning_count = (result_counts.2 + job_findings.0) as u32;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let blocking_count = (result_counts.3 + job_findings.1) as u32;
    if result_counts.0 != i64::from(total_issues) || result_counts.1 > 0 {
        return Err(conflict(
            "review_incomplete",
            "The persisted review results are incomplete",
        ));
    }
    if blocking_count > 0 {
        return Err(conflict(
            "blocking_findings",
            "The import contains blocking findings",
        ));
    }
    if warning_count > 0 && !acknowledge_warnings {
        return Err(conflict(
            "warning_acknowledgement_required",
            "The import has warnings that must be acknowledged",
        ));
    }

    sqlx::query("UPDATE series SET active = TRUE WHERE id = ?")
        .bind(series_id)
        .execute(&mut *transaction)
        .await?;
    let decision = if warning_count > 0 {
        "warnings_acknowledged"
    } else {
        "clean"
    };
    let insert = sqlx::query(
        "INSERT INTO series_publication_events (series_id, import_job_id, actor_user_id, action, \
         decision, warning_count, blocking_count) VALUES (?, ?, ?, 'activated', ?, ?, ?)",
    )
    .bind(series_id)
    .bind(job_id)
    .bind(actor_user_id)
    .bind(decision)
    .bind(warning_count)
    .bind(blocking_count)
    .execute(&mut *transaction)
    .await?;
    #[allow(clippy::cast_possible_truncation)]
    let event_id = insert.last_insert_id() as u32;
    let event = sqlx::query_as::<_, PublicationEvent>(
        "SELECT id, series_id, import_job_id, actor_user_id, action, decision, warning_count, \
         blocking_count, created_at FROM series_publication_events WHERE id = ?",
    )
    .bind(event_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(ActivationResponse {
        series_id,
        active: true,
        event: Some(event),
    })
}

pub async fn deactivate_series(
    state: &AppStateInner,
    series_id: u32,
    actor_user_id: u32,
) -> Result<Option<PublicationEvent>, AppError> {
    let mut transaction = state.pool.begin().await?;
    let row: Option<(bool,)> = sqlx::query_as("SELECT active FROM series WHERE id = ? FOR UPDATE")
        .bind(series_id)
        .fetch_optional(&mut *transaction)
        .await?;
    let (active,) =
        row.ok_or_else(|| AppError::NotFound(format!("Series {series_id} not found")))?;
    if !active {
        transaction.commit().await?;
        return Ok(None);
    }
    sqlx::query("UPDATE series SET active = FALSE WHERE id = ?")
        .bind(series_id)
        .execute(&mut *transaction)
        .await?;
    let insert = sqlx::query(
        "INSERT INTO series_publication_events \
         (series_id, actor_user_id, action, warning_count, blocking_count) \
         VALUES (?, ?, 'deactivated', 0, 0)",
    )
    .bind(series_id)
    .bind(actor_user_id)
    .execute(&mut *transaction)
    .await?;
    #[allow(clippy::cast_possible_truncation)]
    let event_id = insert.last_insert_id() as u32;
    let event = sqlx::query_as::<_, PublicationEvent>(
        "SELECT id, series_id, import_job_id, actor_user_id, action, decision, warning_count, \
         blocking_count, created_at FROM series_publication_events WHERE id = ?",
    )
    .bind(event_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Some(event))
}

fn conflict(code: &str, message: &str) -> AppError {
    AppError::ConflictWithCode {
        message: message.to_string(),
        code: code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use lilly_importer_adapters::adapters::maddrax::MaddraxAdapter;
    use lilly_importer_core::AdapterRegistry;
    use sqlx::mysql::MySqlPoolOptions;

    use super::*;
    use crate::db::import_jobs::{ImportProgress, NewImportJob};
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;

    fn reference() -> ReferenceRecord {
        ReferenceRecord {
            issue_number: 1,
            title: "Pinned title",
            authors: &["Pinned Author"],
            published_at: NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(),
        }
    }

    fn item() -> ReviewItem {
        ReviewItem {
            id: 1,
            job_id: 2,
            issue_id: Some(3),
            issue_number: 1,
            outcome: "created".to_string(),
            severity: "info".to_string(),
            stage: Some("complete".to_string()),
            message: None,
            source_key: "test".to_string(),
            source_record_id: Some("Issue:1".to_string()),
            source_url: Some("https://example.test/1".to_string()),
            title: Some("Pinned title".to_string()),
            authors: vec!["Pinned Author".to_string()],
            cover_artists: Vec::new(),
            published_at: Some(NaiveDate::from_ymd_opt(2025, 1, 2).unwrap()),
            part_number: None,
            part_total: None,
            cycle: None,
            cover_status: "imported".to_string(),
            cover_reason: None,
            cover_local_path: Some("/cover.jpg".to_string()),
            processed_at: None,
        }
    }

    #[test]
    fn reference_check_passes_only_for_an_exact_snapshot() {
        let reference = reference();
        assert_eq!(
            build_reference_checks(std::slice::from_ref(&reference), &[item()])[0].status,
            "passed"
        );
        let mut changed = item();
        changed.title = Some("Changed".to_string());
        assert_eq!(
            build_reference_checks(&[reference], &[changed])[0].status,
            "failed"
        );
    }

    #[test]
    fn missing_reference_is_reported_as_failed() {
        assert_eq!(
            build_reference_checks(&[reference()], &[])[0].status,
            "failed"
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn warning_acknowledgement_activation_and_audit_are_persistent() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _database_guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        crate::db::migrate_test_database(&pool)
            .await
            .expect("test migrations must succeed");
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let actor_user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role) VALUES (?, 'Review Tester', 'admin')",
        )
        .bind(format!("review-{suffix}@example.test"))
        .execute(&pool)
        .await
        .expect("admin fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("admin fixture ID must fit u32");
        let series_id: u32 = sqlx::query(
            "INSERT INTO series (name, slug, source_key, source_record_id, source_url) \
             VALUES ('Review Test', ?, 'maddraxikon', ?, 'https://de.maddraxikon.com/wiki/Hauptseite')",
        )
        .bind(format!("review-test-{suffix}"))
        .bind(format!("Review:{suffix}"))
        .execute(&pool)
        .await
        .expect("series fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("series fixture ID must fit u32");
        let job_id = import_jobs::create_import_job_if_idle(
            &pool,
            &NewImportJob {
                series_id,
                adapter_name: "maddrax",
                source_key: "maddraxikon",
                started_by: Some(actor_user_id),
                trigger_type: "manual",
                scheduled_for: None,
                retry_of_job_id: None,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            import_jobs::mark_import_running(&pool, job_id)
                .await
                .unwrap()
        );

        let adapter = MaddraxAdapter::new().expect("Maddrax adapter must be constructible");
        let references = adapter.reference_records();
        let numbers = references
            .iter()
            .map(|reference| reference.issue_number)
            .collect::<Vec<_>>();
        import_review::seed_import_results(&pool, job_id, "maddraxikon", &numbers)
            .await
            .unwrap();
        for reference in &references {
            let authors = reference
                .authors
                .iter()
                .map(|author| (*author).to_string())
                .collect::<Vec<_>>();
            let warning = reference.issue_number == 555;
            import_review::record_import_result(
                &pool,
                job_id,
                "maddraxikon",
                &import_review::ReviewResultUpdate {
                    issue_id: None,
                    issue_number: reference.issue_number,
                    outcome: "created",
                    severity: if warning { "warning" } else { "info" },
                    stage: "complete",
                    message: warning.then_some("The source does not provide a cover"),
                    source_record_id: Some("test-reference"),
                    source_url: Some("https://de.maddraxikon.com/wiki/Test"),
                    title: Some(reference.title),
                    authors: &authors,
                    cover_artists: &[],
                    published_at: Some(reference.published_at),
                    part_number: None,
                    part_total: None,
                    cycle: None,
                    cover_status: if warning {
                        "missing_at_source"
                    } else {
                        "imported"
                    },
                    cover_reason: warning.then_some("The source does not provide a cover"),
                    cover_local_path: (!warning).then_some("/media/test.jpg"),
                },
            )
            .await
            .unwrap();
            if warning {
                import_jobs::record_import_finding(
                    &pool,
                    job_id,
                    "maddraxikon",
                    Some(reference.issue_number),
                    Some("test-reference"),
                    "cover",
                    "warning",
                    "missing_at_source",
                    "The source does not provide a cover",
                )
                .await
                .unwrap();
            }
        }
        let progress = ImportProgress {
            total: 3,
            created: 3,
            ..ImportProgress::default()
        };
        assert!(
            import_jobs::complete_import_job(&pool, job_id, progress, None)
                .await
                .unwrap()
        );

        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(adapter)).unwrap();
        let state = AppStateInner {
            pool: pool.clone(),
            jwt_secret: "test-secret".to_string(),
            jwt_access_expiry: 900,
            jwt_refresh_expiry: 2_592_000,
            email_service: EmailService::Log {
                from: "test@example.test".to_string(),
            },
            app_base_url: "http://localhost".to_string(),
            cookie_secure: false,
            adapter_registry: registry,
            media_path: std::path::PathBuf::from("/tmp/lilly-review-test"),
            media_url_prefix: "/media".to_string(),
            photo_upload_config: crate::config::PhotoUploadConfig::default(),
            media_storage: crate::services::media::MediaStorage::new(std::path::Path::new(
                "/tmp/lilly-review-test",
            )),
            import_scheduler_config: ImportSchedulerConfig {
                enabled: false,
                schedule: "0 10 6 * * Sat *".to_string(),
                timezone: "Europe/Berlin".to_string(),
                adapters: Vec::new(),
            },
        };

        let summary = evaluate_activation_eligibility(&state, job_id)
            .await
            .unwrap();
        assert!(summary.eligibility.eligible);
        assert!(summary.eligibility.requires_acknowledgement);
        assert_eq!(summary.warning_count, 1);
        assert!(
            summary
                .reference_checks
                .iter()
                .all(|check| check.status == "passed")
        );
        assert!(matches!(
            activate_from_import(&state, job_id, actor_user_id, false).await,
            Err(AppError::ConflictWithCode { code, .. })
                if code == "warning_acknowledgement_required"
        ));
        let activated = activate_from_import(&state, job_id, actor_user_id, true)
            .await
            .unwrap();
        let activation_event = activated.event.expect("activation event must be returned");
        assert_eq!(activation_event.actor_user_id, Some(actor_user_id));
        assert_eq!(
            activation_event.decision.as_deref(),
            Some("warnings_acknowledged")
        );
        assert_eq!(activation_event.warning_count, 1);

        let repeated = activate_from_import(&state, job_id, actor_user_id, true)
            .await
            .unwrap();
        assert_eq!(repeated.event.unwrap().id, activation_event.id);
        let deactivation_event = deactivate_series(&state, series_id, actor_user_id)
            .await
            .unwrap()
            .expect("deactivation event must be written");
        assert_eq!(deactivation_event.action, "deactivated");
        assert!(
            !series::find_series_by_id(&pool, series_id)
                .await
                .unwrap()
                .unwrap()
                .active
        );

        assert!(
            sqlx::query("DELETE FROM series WHERE id = ?")
                .bind(series_id)
                .execute(&pool)
                .await
                .is_err(),
            "publication history must prevent deletion of its series"
        );
        sqlx::query("DELETE FROM import_jobs WHERE id = ?")
            .bind(job_id)
            .execute(&pool)
            .await
            .expect("import job fixture must be deleted");
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(actor_user_id)
            .execute(&pool)
            .await
            .expect("actor fixture must be deletable");
        let retained_event = import_review::find_last_publication_event(&pool, series_id)
            .await
            .unwrap()
            .expect("audit event must survive actor deletion");
        assert_eq!(retained_event.actor_user_id, None);
        assert_eq!(retained_event.import_job_id, None);

        sqlx::query("DELETE FROM series_publication_events WHERE series_id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("audit fixture must be deleted explicitly");
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series fixture must be deleted after its audit fixtures");
    }
}
