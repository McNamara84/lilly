use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use lilly_importer_core::{AdapterError, IssueData, WikiAdapter};

use crate::db::{import_jobs, issues, series};
use crate::error::AppError;
use crate::models::series::ImportJobResponse;
use crate::routes::AppStateInner;

const RECENT_REFRESH_COUNT: usize = 12;
const MAX_FETCH_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, Copy)]
pub enum ImportTrigger {
    Manual { user_id: u32 },
    Scheduled { scheduled_for: DateTime<Utc> },
}

impl ImportTrigger {
    fn database_values(self) -> (Option<u32>, &'static str, Option<chrono::NaiveDateTime>) {
        match self {
            Self::Manual { user_id } => (Some(user_id), "manual", None),
            Self::Scheduled { scheduled_for } => {
                (None, "scheduled", Some(scheduled_for.naive_utc()))
            }
        }
    }
}

pub async fn start_import(
    state: Arc<AppStateInner>,
    adapter_name: &str,
    trigger: ImportTrigger,
) -> Result<ImportJobResponse, AppError> {
    let adapter = state
        .adapter_registry
        .get(adapter_name)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown adapter: '{adapter_name}'")))?;

    let (started_by, trigger_type, scheduled_for) = trigger.database_values();
    if let Some(scheduled_for) = scheduled_for
        && import_jobs::has_scheduled_job(&state.pool, adapter_name, scheduled_for).await?
    {
        return Err(AppError::BadRequest(format!(
            "Scheduled import already exists for adapter '{adapter_name}' at {scheduled_for} UTC"
        )));
    }

    let metadata = adapter
        .fetch_series_metadata()
        .await
        .map_err(|error| AppError::InternalError(error.into()))?;

    let series_id = match series::find_series_by_slug(&state.pool, &metadata.slug).await? {
        Some(existing) => existing.id,
        None => {
            series::create_series(
                &state.pool,
                &metadata.name,
                &metadata.slug,
                metadata.publisher.as_deref(),
                metadata.genre.as_deref(),
                metadata.frequency.as_deref(),
                metadata.total_issues,
                &metadata.status.to_string(),
                metadata.source_url.as_deref(),
            )
            .await?
        }
    };

    let job_id = import_jobs::create_import_job_if_idle(
        &state.pool,
        series_id,
        adapter_name,
        started_by,
        trigger_type,
        scheduled_for,
    )
    .await
    .map_err(|error| {
        if let sqlx::Error::Database(ref database_error) = error
            && database_error.kind() == sqlx::error::ErrorKind::UniqueViolation
        {
            return AppError::BadRequest("This scheduled import already exists".to_string());
        }
        AppError::from(error)
    })?
    .ok_or_else(|| {
        AppError::BadRequest("An import is already running for this series".to_string())
    })?;

    let pool = state.pool.clone();
    let task_state = state.clone();
    let task_adapter_name = adapter_name.to_string();
    let task_trigger_type = trigger_type;
    tokio::spawn(async move {
        if let Err(error) = execute_import(
            task_state,
            series_id,
            job_id,
            &task_adapter_name,
            task_trigger_type,
        )
        .await
        {
            tracing::error!(
                job_id,
                adapter = task_adapter_name,
                trigger_type = task_trigger_type,
                error = %error,
                "Import task failed"
            );
            if let Err(database_error) =
                import_jobs::fail_import_job(&pool, job_id, &error.to_string()).await
            {
                tracing::error!(job_id, error = %database_error, "Failed to mark import job as failed");
            }
        }
    });

    let job = import_jobs::find_import_job_by_id(&state.pool, job_id)
        .await?
        .ok_or_else(|| {
            AppError::InternalError(anyhow::anyhow!("Failed to retrieve created import job"))
        })?;

    Ok(ImportJobResponse::from_job_with_slug(&job, metadata.slug))
}

async fn execute_import(
    state: Arc<AppStateInner>,
    series_id: u32,
    job_id: u32,
    adapter_name: &str,
    trigger_type: &str,
) -> Result<(), anyhow::Error> {
    let started_at = std::time::Instant::now();
    let adapter = state
        .adapter_registry
        .get(adapter_name)
        .ok_or_else(|| anyhow::anyhow!("Adapter '{adapter_name}' not found"))?;

    let source_numbers = adapter
        .fetch_issue_list()
        .await
        .map_err(|error| anyhow::anyhow!("Failed to fetch issue list: {error}"))?;
    let existing = issues::find_existing_issue_numbers(&state.pool, series_id).await?;
    let unsynced = issues::find_unsynced_issue_numbers(&state.pool, series_id).await?;
    let candidates = select_import_candidates(&source_numbers, &existing, &unsynced);

    let mut total = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
    let mut imported = 0u32;
    let mut failed = 0u32;
    let mut error_messages = Vec::new();
    import_jobs::update_import_progress(&state.pool, job_id, imported, failed, total).await?;

    let cover_dir = state
        .media_path
        .join("covers")
        .join(format!("series-{series_id}"));
    tokio::fs::create_dir_all(&cover_dir).await?;

    let today = Utc::now()
        .with_timezone(&chrono_tz::Europe::Berlin)
        .date_naive();

    for issue_number in candidates {
        let details = match fetch_issue_details_with_retry(adapter, issue_number).await {
            Ok(details) => details,
            Err(error) => {
                failed += 1;
                error_messages.push(format!("#{issue_number}: {error}"));
                import_jobs::update_import_progress(&state.pool, job_id, imported, failed, total)
                    .await?;
                continue;
            }
        };

        if !is_published(details.published_at, today) {
            total = total.saturating_sub(1);
            tracing::info!(job_id, issue_number, "Skipping future issue");
            import_jobs::update_import_progress(&state.pool, job_id, imported, failed, total)
                .await?;
            continue;
        }

        let (part_number, part_total) =
            normalize_part_position(details.part_number, details.part_total);
        let cover_local_path = fetch_and_store_cover(
            adapter,
            issue_number,
            &cover_dir,
            &state.media_url_prefix,
            series_id,
        )
        .await;

        let result = persist_issue(
            &state,
            series_id,
            &details,
            part_number,
            part_total,
            cover_local_path.as_deref(),
        )
        .await;

        match result {
            Ok(()) => imported += 1,
            Err(error) => {
                failed += 1;
                error_messages.push(format!("#{issue_number}: {error}"));
                tracing::warn!(job_id, issue_number, error = %error, "Failed to persist issue");
            }
        }

        import_jobs::update_import_progress(&state.pool, job_id, imported, failed, total).await?;
    }

    let actual_count = issues::count_issues_by_series(&state.pool, series_id).await?;
    series::update_series_total_issues(&state.pool, series_id, actual_count).await?;

    let summary = summarize_errors(&error_messages);
    import_jobs::complete_import_job(&state.pool, job_id, failed, summary.as_deref()).await?;
    tracing::info!(
        job_id,
        adapter = adapter_name,
        trigger_type,
        imported_issues = imported,
        failed_issues = failed,
        total_issues = total,
        duration_ms = started_at.elapsed().as_millis(),
        "Import completed"
    );
    Ok(())
}

fn select_import_candidates(
    source_numbers: &[u32],
    existing: &HashSet<u32>,
    unsynced: &[u32],
) -> Vec<u32> {
    let source_set: HashSet<u32> = source_numbers.iter().copied().collect();
    let mut candidates: BTreeSet<u32> = source_numbers
        .iter()
        .copied()
        .filter(|number| !existing.contains(number))
        .collect();

    candidates.extend(
        unsynced
            .iter()
            .copied()
            .filter(|number| source_set.contains(number)),
    );
    candidates.extend(
        source_numbers
            .iter()
            .rev()
            .take(RECENT_REFRESH_COUNT)
            .copied(),
    );
    candidates.into_iter().collect()
}

async fn fetch_issue_details_with_retry(
    adapter: &dyn WikiAdapter,
    issue_number: u32,
) -> Result<IssueData, AdapterError> {
    let mut attempt = 1u8;
    loop {
        match adapter.fetch_issue_details(issue_number).await {
            Ok(details) => return Ok(details),
            Err(error) if attempt < MAX_FETCH_ATTEMPTS && is_transient(&error) => {
                tokio::time::sleep(Duration::from_millis(250 * u64::from(attempt))).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

const fn is_transient(error: &AdapterError) -> bool {
    matches!(error, AdapterError::Network(_) | AdapterError::RateLimited)
}

async fn fetch_and_store_cover(
    adapter: &dyn WikiAdapter,
    issue_number: u32,
    cover_dir: &std::path::Path,
    media_url_prefix: &str,
    series_id: u32,
) -> Option<String> {
    let cover = match adapter.fetch_cover(issue_number).await {
        Ok(Some(cover)) => cover,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(issue_number, error = %error, "Failed to fetch cover");
            return None;
        }
    };

    let extension = cover_extension(&cover.content_type)?;
    let target = cover_dir.join(format!("{issue_number}.{extension}"));
    let temporary = temporary_cover_path(cover_dir, issue_number, extension);
    if let Err(error) = tokio::fs::write(&temporary, &cover.bytes).await {
        tracing::warn!(issue_number, error = %error, "Failed to write temporary cover");
        return None;
    }
    if let Err(error) = tokio::fs::rename(&temporary, &target).await {
        tracing::warn!(issue_number, error = %error, "Failed to atomically store cover");
        let _ = tokio::fs::remove_file(&temporary).await;
        return None;
    }

    Some(format!(
        "{media_url_prefix}/covers/series-{series_id}/{issue_number}.{extension}"
    ))
}

async fn persist_issue(
    state: &AppStateInner,
    series_id: u32,
    details: &IssueData,
    part_number: Option<u32>,
    part_total: Option<u32>,
    cover_local_path: Option<&str>,
) -> Result<(), anyhow::Error> {
    issues::replace_issue_metadata(
        &state.pool,
        &issues::IssueMetadataUpdate {
            series_id,
            issue_number: details.issue_number,
            title: &details.title,
            published_at: details.published_at,
            part_number,
            part_total,
            cycle: details.cycle.as_deref(),
            cover_url: None,
            cover_local_path,
            source_wiki_url: details.source_wiki_url.as_deref(),
            authors: &details.authors,
            cover_artists: &details.cover_artists,
            keywords: &details.keywords,
            notes: &details.notes,
        },
    )
    .await?;
    Ok(())
}

fn normalize_part_position(
    part_number: Option<u32>,
    part_total: Option<u32>,
) -> (Option<u32>, Option<u32>) {
    match (part_number, part_total) {
        (Some(number), Some(total)) if number > 0 && number <= total => (Some(number), Some(total)),
        _ => (None, None),
    }
}

fn is_published(published_at: Option<NaiveDate>, today: NaiveDate) -> bool {
    published_at.is_none_or(|date| date <= today)
}

fn cover_extension(content_type: &str) -> Option<&'static str> {
    let normalized = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match normalized.as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

fn temporary_cover_path(
    cover_dir: &std::path::Path,
    issue_number: u32,
    extension: &str,
) -> PathBuf {
    cover_dir.join(format!(".{issue_number}.{extension}.tmp"))
}

fn summarize_errors(errors: &[String]) -> Option<String> {
    if errors.is_empty() {
        return None;
    }
    let shown = errors
        .iter()
        .take(10)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if errors.len() > 10 {
        Some(format!("{shown}; and {} more errors", errors.len() - 10))
    } else {
        Some(shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_candidates_contains_new_unsynced_and_recent_without_duplicates() {
        let source: Vec<u32> = (1..=20).collect();
        let existing: HashSet<u32> = (1..=18).collect();
        let unsynced = vec![2, 7, 99];
        let selected = select_import_candidates(&source, &existing, &unsynced);

        assert!(selected.contains(&2));
        assert!(selected.contains(&7));
        assert!(!selected.contains(&99));
        assert!(selected.contains(&19));
        assert!(selected.contains(&20));
        assert_eq!(selected.iter().filter(|&&number| number == 20).count(), 1);
        assert_eq!(selected.first(), Some(&2));
    }

    #[test]
    fn select_candidates_refreshes_twelve_latest_issues() {
        let source: Vec<u32> = (1..=20).collect();
        let existing: HashSet<u32> = source.iter().copied().collect();
        let selected = select_import_candidates(&source, &existing, &[]);
        assert_eq!(selected, (9..=20).collect::<Vec<_>>());
    }

    #[test]
    fn normalize_part_position_accepts_only_complete_valid_pairs() {
        assert_eq!(
            normalize_part_position(Some(2), Some(3)),
            (Some(2), Some(3))
        );
        assert_eq!(normalize_part_position(None, None), (None, None));
        assert_eq!(normalize_part_position(Some(1), None), (None, None));
        assert_eq!(normalize_part_position(Some(0), Some(2)), (None, None));
        assert_eq!(normalize_part_position(Some(3), Some(2)), (None, None));
    }

    #[test]
    fn publication_filter_allows_unknown_past_and_today_but_not_future() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        assert!(is_published(None, today));
        assert!(is_published(Some(today), today));
        assert!(is_published(
            Some(NaiveDate::from_ymd_opt(1978, 1, 17).unwrap()),
            today
        ));
        assert!(!is_published(
            Some(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap()),
            today
        ));
    }

    #[test]
    fn cover_extension_accepts_only_supported_image_types() {
        assert_eq!(cover_extension("image/jpeg"), Some("jpg"));
        assert_eq!(cover_extension("image/png; charset=binary"), Some("png"));
        assert_eq!(cover_extension("IMAGE/WEBP"), Some("webp"));
        assert_eq!(cover_extension("image/gif"), None);
        assert_eq!(cover_extension("text/html"), None);
    }

    #[test]
    fn summarize_errors_is_empty_for_no_errors_and_caps_output() {
        assert_eq!(summarize_errors(&[]), None);
        assert_eq!(
            summarize_errors(&["one".to_string()]),
            Some("one".to_string())
        );
        let errors = (1..=12)
            .map(|number| format!("error {number}"))
            .collect::<Vec<_>>();
        let summary = summarize_errors(&errors).unwrap();
        assert!(summary.contains("and 2 more errors"));
        assert!(!summary.contains("error 11"));
    }

    #[test]
    fn import_trigger_database_values_are_consistent() {
        assert_eq!(
            ImportTrigger::Manual { user_id: 42 }.database_values(),
            (Some(42), "manual", None)
        );
        let scheduled_for = DateTime::from_timestamp(1_786_164_600, 0).unwrap();
        assert_eq!(
            ImportTrigger::Scheduled { scheduled_for }.database_values(),
            (None, "scheduled", Some(scheduled_for.naive_utc()))
        );
    }
}
