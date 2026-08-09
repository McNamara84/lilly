use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use lilly_importer_core::{
    AdapterError, IssueData, SourceDescriptor, WikiAdapter, normalize_and_validate_issue,
    normalize_and_validate_series,
};

use crate::db::import_jobs::ImportProgress;
use crate::db::{import_jobs, issues, series};
use crate::error::AppError;
use crate::models::series::{ImportJobResponse, IssueResponse};
use crate::routes::AppStateInner;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IssueOutcome {
    Created,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonicalIssue {
    issue_number: u32,
    title: String,
    authors: Vec<String>,
    published_at: Option<NaiveDate>,
    part_number: Option<u32>,
    part_total: Option<u32>,
    cycle: Option<String>,
    cover_artists: Vec<String>,
    keywords: Vec<String>,
    notes: Vec<String>,
    source_key: String,
    source_record_id: String,
    source_url: String,
}

impl From<&IssueData> for CanonicalIssue {
    fn from(issue: &IssueData) -> Self {
        Self {
            issue_number: issue.issue_number,
            title: issue.title.clone(),
            authors: issue.authors.clone(),
            published_at: issue.published_at,
            part_number: issue.part_number,
            part_total: issue.part_total,
            cycle: issue.cycle.clone(),
            cover_artists: issue.cover_artists.clone(),
            keywords: issue.keywords.clone(),
            notes: issue.notes.clone(),
            source_key: issue.source.source_key.clone(),
            source_record_id: issue.source.source_record_id.clone(),
            source_url: issue.source.source_url.clone(),
        }
    }
}

impl From<&IssueResponse> for CanonicalIssue {
    fn from(issue: &IssueResponse) -> Self {
        Self {
            issue_number: issue.issue_number,
            title: issue.title.clone(),
            authors: issue.authors.clone(),
            published_at: issue.published_at,
            part_number: issue.part_number,
            part_total: issue.part_total,
            cycle: issue.cycle.clone(),
            cover_artists: issue.cover_artists.clone(),
            keywords: issue.keywords.clone(),
            notes: issue.notes.clone(),
            source_key: issue.source_key.clone().unwrap_or_default(),
            source_record_id: issue.source_record_id.clone().unwrap_or_default(),
            source_url: issue.source_wiki_url.clone().unwrap_or_default(),
        }
    }
}

pub async fn start_import(
    state: Arc<AppStateInner>,
    adapter_name: &str,
    trigger: ImportTrigger,
) -> Result<ImportJobResponse, AppError> {
    start_import_linked(state, adapter_name, trigger, None).await
}

pub async fn retry_import(
    state: Arc<AppStateInner>,
    job_id: u32,
    user_id: u32,
) -> Result<ImportJobResponse, AppError> {
    let previous = import_jobs::find_import_job_by_id(&state.pool, job_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Import job {job_id} not found")))?;
    if !matches!(previous.status.as_str(), "failed" | "cancelled" | "interrupted") {
        return Err(AppError::Conflict(format!(
            "Import job {job_id} cannot be retried from status '{}'",
            previous.status
        )));
    }
    start_import_linked(
        state,
        &previous.adapter_name,
        ImportTrigger::Manual { user_id },
        Some(previous.id),
    )
    .await
}

async fn start_import_linked(
    state: Arc<AppStateInner>,
    adapter_name: &str,
    trigger: ImportTrigger,
    retry_of_job_id: Option<u32>,
) -> Result<ImportJobResponse, AppError> {
    let adapter = state
        .adapter_registry
        .get(adapter_name)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown adapter: '{adapter_name}'")))?;
    let descriptor = adapter.source_descriptor();
    let (started_by, trigger_type, scheduled_for) = trigger.database_values();

    if let Some(scheduled_for) = scheduled_for
        && import_jobs::has_scheduled_job(&state.pool, adapter_name, scheduled_for).await?
    {
        return Err(AppError::Conflict(format!(
            "Scheduled import already exists for adapter '{adapter_name}' at {scheduled_for} UTC"
        )));
    }

    let series_id = resolve_series_for_source(&state, descriptor).await?;
    if let Some(previous_id) = retry_of_job_id {
        let previous = import_jobs::find_import_job_by_id(&state.pool, previous_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Import job {previous_id} not found")))?;
        if previous.series_id != series_id
            || previous
                .source_key
                .as_deref()
                .is_some_and(|source_key| source_key != descriptor.source_key)
        {
            return Err(AppError::Conflict(
                "Retry source does not match the original import".to_string(),
            ));
        }
    }

    let job_id = import_jobs::create_import_job_if_idle(
        &state.pool,
        series_id,
        adapter_name,
        descriptor.source_key,
        started_by,
        trigger_type,
        scheduled_for,
        retry_of_job_id,
    )
    .await
    .map_err(map_job_creation_error)?
    .ok_or_else(|| {
        AppError::Conflict("An import is already running for this series".to_string())
    })?;

    spawn_import_task(
        state.clone(),
        series_id,
        job_id,
        adapter_name.to_string(),
        trigger_type,
    );

    let job = import_jobs::find_import_job_by_id(&state.pool, job_id)
        .await?
        .ok_or_else(|| {
            AppError::InternalError(anyhow::anyhow!("Failed to retrieve created import job"))
        })?;
    Ok(ImportJobResponse::from_job_with_slug(
        &job,
        descriptor.series_slug.to_string(),
    ))
}

fn spawn_import_task(
    state: Arc<AppStateInner>,
    series_id: u32,
    job_id: u32,
    adapter_name: String,
    trigger_type: &'static str,
) {
    let pool = state.pool.clone();
    let source_key = state
        .adapter_registry
        .get(&adapter_name)
        .map_or_else(|| adapter_name.clone(), |adapter| adapter.source_descriptor().source_key.to_string());
    tokio::spawn(async move {
        if let Err(error) = execute_import(
            state,
            series_id,
            job_id,
            &adapter_name,
            trigger_type,
        )
        .await
        {
            tracing::error!(job_id, adapter = adapter_name, error = %error, "Import task failed");
            let _ = import_jobs::record_import_error(
                &pool,
                job_id,
                &source_key,
                None,
                None,
                "job",
                &error.to_string(),
            )
            .await;
            match import_jobs::is_cancel_requested(&pool, job_id).await {
                Ok(true) => {
                    let _ = import_jobs::cancel_import_job(&pool, job_id).await;
                }
                Ok(false) | Err(_) => {
                    if let Err(database_error) =
                        import_jobs::fail_import_job(&pool, job_id, &error.to_string()).await
                    {
                        tracing::error!(job_id, error = %database_error, "Failed to mark import job as failed");
                    }
                }
            }
        }
    });
}

fn map_job_creation_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref database_error) = error
        && database_error.kind() == sqlx::error::ErrorKind::UniqueViolation
    {
        return AppError::Conflict("This scheduled import already exists".to_string());
    }
    AppError::from(error)
}

async fn resolve_series_for_source(
    state: &AppStateInner,
    descriptor: SourceDescriptor,
) -> Result<u32, AppError> {
    if let Some(existing) = series::find_series_by_source_identity(
        &state.pool,
        descriptor.source_key,
        descriptor.series_record_id,
    )
    .await?
    {
        if existing.slug != descriptor.series_slug {
            return Err(AppError::BadRequest(format!(
                "Source '{}' is already assigned to series '{}'",
                descriptor.source_key, existing.slug
            )));
        }
        return Ok(existing.id);
    }

    if let Some(existing) = series::find_series_by_slug(&state.pool, descriptor.series_slug).await? {
        match (
            existing.source_key.as_deref(),
            existing.source_record_id.as_deref(),
        ) {
            (None, None) => {
                series::bind_series_source_identity(
                    &state.pool,
                    existing.id,
                    descriptor.source_key,
                    descriptor.series_record_id,
                    descriptor.series_url,
                )
                .await?;
                return Ok(existing.id);
            }
            (Some(key), Some(record_id))
                if key == descriptor.source_key && record_id == descriptor.series_record_id =>
            {
                return Ok(existing.id);
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Series '{}' is assigned to a different source",
                    descriptor.series_slug
                )));
            }
        }
    }

    match series::create_series(
        &state.pool,
        descriptor.series_name,
        descriptor.series_slug,
        None,
        None,
        None,
        None,
        "running",
        descriptor.source_key,
        descriptor.series_record_id,
        descriptor.series_url,
    )
    .await
    {
        Ok(series_id) => Ok(series_id),
        Err(error)
            if matches!(
                &error,
                sqlx::Error::Database(database_error)
                    if database_error.kind() == sqlx::error::ErrorKind::UniqueViolation
            ) =>
        {
            let existing = series::find_series_by_source_identity(
                &state.pool,
                descriptor.source_key,
                descriptor.series_record_id,
            )
            .await?
            .or(series::find_series_by_slug(&state.pool, descriptor.series_slug).await?)
            .ok_or_else(|| AppError::from(error))?;
            Ok(existing.id)
        }
        Err(error) => Err(AppError::from(error)),
    }
}

async fn execute_import(
    state: Arc<AppStateInner>,
    series_id: u32,
    job_id: u32,
    adapter_name: &str,
    trigger_type: &str,
) -> Result<(), anyhow::Error> {
    let started_at = std::time::Instant::now();
    if !import_jobs::mark_import_running(&state.pool, job_id).await? {
        return Ok(());
    }

    let adapter = state
        .adapter_registry
        .get(adapter_name)
        .ok_or_else(|| anyhow::anyhow!("Adapter '{adapter_name}' not found"))?;
    let descriptor = adapter.source_descriptor();

    if cancel_if_requested(&state, job_id).await? {
        return Ok(());
    }
    let metadata = normalize_and_validate_series(
        descriptor,
        adapter
            .fetch_series_metadata()
            .await
            .map_err(|error| anyhow::anyhow!("Failed to fetch series metadata: {error}"))?,
    )
    .map_err(|error| anyhow::anyhow!("Invalid series metadata: {error}"))?;
    if cancel_if_requested(&state, job_id).await? {
        return Ok(());
    }
    series::update_series_metadata(
        &state.pool,
        series_id,
        &metadata.name,
        metadata.publisher.as_deref(),
        metadata.genre.as_deref(),
        metadata.frequency.as_deref(),
        metadata.total_issues,
        &metadata.status.to_string(),
        &metadata.source.source_url,
    )
    .await?;

    let source_numbers = normalize_source_numbers(
        adapter
            .fetch_issue_list()
            .await
            .map_err(|error| anyhow::anyhow!("Failed to fetch issue list: {error}"))?,
    )?;
    if cancel_if_requested(&state, job_id).await? {
        return Ok(());
    }

    let stored_rows = issues::find_all_issues_by_series(&state.pool, series_id).await?;
    let stored_responses = issues::build_issue_responses(&state.pool, &stored_rows).await?;
    let stored: HashMap<u32, IssueResponse> = stored_responses
        .into_iter()
        .map(|issue| (issue.issue_number, issue))
        .collect();

    let source_number_set: BTreeSet<u32> = source_numbers.iter().copied().collect();
    let missing_from_source = stored
        .keys()
        .copied()
        .filter(|number| !source_number_set.contains(number))
        .collect::<Vec<_>>();
    if !missing_from_source.is_empty() {
        let message = summarize_missing_source_numbers(&missing_from_source);
        tracing::warn!(job_id, missing = missing_from_source.len(), %message, "Stored issues are absent from the current source list");
        import_jobs::record_import_error(
            &state.pool,
            job_id,
            descriptor.source_key,
            None,
            None,
            "source-list",
            &message,
        )
        .await?;
    }

    let mut progress = ImportProgress {
        total: u32::try_from(source_numbers.len()).unwrap_or(u32::MAX),
        ..ImportProgress::default()
    };
    import_jobs::update_import_progress(&state.pool, job_id, progress).await?;

    let cover_dir = state
        .media_path
        .join("covers")
        .join(format!("series-{series_id}"));
    let today = Utc::now()
        .with_timezone(&chrono_tz::Europe::Berlin)
        .date_naive();
    let mut error_messages = Vec::new();

    for issue_number in source_numbers {
        if cancel_if_requested(&state, job_id).await? {
            return Ok(());
        }

        let details = match fetch_issue_details_with_retry(adapter, issue_number).await {
            Ok(details) => details,
            Err(error) => {
                record_issue_failure(
                    &state,
                    job_id,
                    descriptor.source_key,
                    issue_number,
                    None,
                    "fetch",
                    &error.to_string(),
                    &mut progress,
                    &mut error_messages,
                )
                .await?;
                continue;
            }
        };
        if cancel_if_requested(&state, job_id).await? {
            return Ok(());
        }

        let source_record_id = details.source.source_record_id.clone();
        let details = match normalize_and_validate_issue(descriptor, issue_number, details) {
            Ok(details) => details,
            Err(error) => {
                record_issue_failure(
                    &state,
                    job_id,
                    descriptor.source_key,
                    issue_number,
                    Some(&source_record_id),
                    "validate",
                    &error.to_string(),
                    &mut progress,
                    &mut error_messages,
                )
                .await?;
                continue;
            }
        };

        if !is_published(details.published_at, today) {
            progress.skipped = progress.skipped.saturating_add(1);
            import_jobs::update_import_progress(&state.pool, job_id, progress).await?;
            continue;
        }

        let existing = stored.get(&issue_number);
        let outcome = classify_issue(&details, existing);
        if outcome == IssueOutcome::Unchanged {
            if let Some(existing) = existing {
                issues::mark_issue_checked(&state.pool, existing.id).await?;
            }
            progress.unchanged = progress.unchanged.saturating_add(1);
            import_jobs::update_import_progress(&state.pool, job_id, progress).await?;
            continue;
        }

        let needs_cover = existing.is_none_or(|issue| issue.cover_local_path.is_none());
        let cover_local_path = if needs_cover {
            if cancel_if_requested(&state, job_id).await? {
                return Ok(());
            }
            match fetch_and_store_cover(
                adapter,
                issue_number,
                &cover_dir,
                &state.media_url_prefix,
                series_id,
            )
            .await
            {
                Ok(path) => path,
                Err(error) => {
                    import_jobs::record_import_error(
                        &state.pool,
                        job_id,
                        descriptor.source_key,
                        Some(issue_number),
                        Some(&details.source.source_record_id),
                        "cover",
                        &error,
                    )
                    .await?;
                    None
                }
            }
        } else {
            None
        };
        if cancel_if_requested(&state, job_id).await? {
            return Ok(());
        }

        if let Err(error) = persist_issue(
            &state,
            series_id,
            &details,
            cover_local_path.as_deref(),
        )
        .await
        {
            record_issue_failure(
                &state,
                job_id,
                descriptor.source_key,
                issue_number,
                Some(&details.source.source_record_id),
                "persist",
                &error.to_string(),
                &mut progress,
                &mut error_messages,
            )
            .await?;
            continue;
        }

        match outcome {
            IssueOutcome::Created => progress.created = progress.created.saturating_add(1),
            IssueOutcome::Updated => progress.updated = progress.updated.saturating_add(1),
            IssueOutcome::Unchanged => unreachable!(),
        }
        import_jobs::update_import_progress(&state.pool, job_id, progress).await?;
    }

    let actual_count = issues::count_issues_by_series(&state.pool, series_id).await?;
    series::update_series_total_issues(&state.pool, series_id, actual_count).await?;
    if cancel_if_requested(&state, job_id).await? {
        return Ok(());
    }

    let summary = summarize_errors(&error_messages);
    validate_progress(progress)?;
    let completed = import_jobs::complete_import_job(
        &state.pool,
        job_id,
        progress,
        summary.as_deref(),
    )
    .await?;
    if !completed {
        import_jobs::cancel_import_job(&state.pool, job_id).await?;
    }
    tracing::info!(
        job_id,
        adapter = adapter_name,
        trigger_type,
        created = progress.created,
        updated = progress.updated,
        unchanged = progress.unchanged,
        skipped = progress.skipped,
        failed = progress.failed,
        duration_ms = started_at.elapsed().as_millis(),
        "Import synchronization completed"
    );
    Ok(())
}

async fn cancel_if_requested(state: &AppStateInner, job_id: u32) -> Result<bool, sqlx::Error> {
    if import_jobs::is_cancel_requested(&state.pool, job_id).await? {
        import_jobs::cancel_import_job(&state.pool, job_id).await?;
        return Ok(true);
    }
    Ok(false)
}

async fn record_issue_failure(
    state: &AppStateInner,
    job_id: u32,
    source_key: &str,
    issue_number: u32,
    source_record_id: Option<&str>,
    stage: &str,
    message: &str,
    progress: &mut ImportProgress,
    error_messages: &mut Vec<String>,
) -> Result<(), sqlx::Error> {
    progress.failed = progress.failed.saturating_add(1);
    error_messages.push(format!("#{issue_number} [{stage}]: {message}"));
    import_jobs::record_import_error(
        &state.pool,
        job_id,
        source_key,
        Some(issue_number),
        source_record_id,
        stage,
        message,
    )
    .await?;
    import_jobs::update_import_progress(&state.pool, job_id, *progress).await
}

fn normalize_source_numbers(numbers: Vec<u32>) -> Result<Vec<u32>, anyhow::Error> {
    if numbers.is_empty() {
        return Err(anyhow::anyhow!("Source returned no issue numbers"));
    }
    if numbers.contains(&0) {
        return Err(anyhow::anyhow!("Source returned invalid issue number 0"));
    }
    let unique: BTreeSet<u32> = numbers.iter().copied().collect();
    if unique.len() != numbers.len() {
        return Err(anyhow::anyhow!("Source returned duplicate issue numbers"));
    }
    Ok(unique.into_iter().collect())
}

fn classify_issue(details: &IssueData, existing: Option<&IssueResponse>) -> IssueOutcome {
    match existing {
        None => IssueOutcome::Created,
        Some(existing) if CanonicalIssue::from(existing) == CanonicalIssue::from(details) => {
            IssueOutcome::Unchanged
        }
        Some(_) => IssueOutcome::Updated,
    }
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
) -> Result<Option<String>, String> {
    let cover = match adapter.fetch_cover(issue_number).await {
        Ok(Some(cover)) => cover,
        Ok(None) => return Ok(None),
        Err(error) => return Err(format!("Failed to fetch cover: {error}")),
    };

    let extension = cover_extension(&cover.content_type)
        .ok_or_else(|| format!("Unsupported cover content type '{}'", cover.content_type))?;
    tokio::fs::create_dir_all(cover_dir)
        .await
        .map_err(|error| format!("Failed to create cover directory: {error}"))?;
    let target = cover_dir.join(format!("{issue_number}.{extension}"));
    let temporary = temporary_cover_path(cover_dir, issue_number, extension);
    tokio::fs::write(&temporary, &cover.bytes)
        .await
        .map_err(|error| format!("Failed to write temporary cover: {error}"))?;
    if let Err(error) = tokio::fs::rename(&temporary, &target).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(format!("Failed to atomically store cover: {error}"));
    }

    Ok(Some(format!(
        "{media_url_prefix}/covers/series-{series_id}/{issue_number}.{extension}"
    )))
}

async fn persist_issue(
    state: &AppStateInner,
    series_id: u32,
    details: &IssueData,
    cover_local_path: Option<&str>,
) -> Result<(), anyhow::Error> {
    issues::replace_issue_metadata(
        &state.pool,
        &issues::IssueMetadataUpdate {
            series_id,
            issue_number: details.issue_number,
            title: &details.title,
            published_at: details.published_at,
            part_number: details.part_number,
            part_total: details.part_total,
            cycle: details.cycle.as_deref(),
            cover_url: None,
            cover_local_path,
            source_key: &details.source.source_key,
            source_record_id: &details.source.source_record_id,
            source_wiki_url: Some(&details.source.source_url),
            authors: &details.authors,
            cover_artists: &details.cover_artists,
            keywords: &details.keywords,
            notes: &details.notes,
        },
    )
    .await?;
    Ok(())
}

fn is_published(published_at: Option<NaiveDate>, today: NaiveDate) -> bool {
    published_at.is_some_and(|date| date <= today)
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

fn summarize_missing_source_numbers(numbers: &[u32]) -> String {
    let shown = numbers
        .iter()
        .take(20)
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    if numbers.len() > 20 {
        format!(
            "Stored issues absent from source list: {shown} (and {} more)",
            numbers.len() - 20
        )
    } else {
        format!("Stored issues absent from source list: {shown}")
    }
}

fn validate_progress(progress: ImportProgress) -> Result<(), anyhow::Error> {
    if progress.processed() != progress.total {
        return Err(anyhow::anyhow!(
            "Import progress invariant violated: processed {} of {} issues",
            progress.processed(),
            progress.total
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use async_trait::async_trait;
    use lilly_importer_core::{
        AdapterRegistry, CoverData, SeriesData, SeriesStatus, SourceReference,
    };
    use sqlx::mysql::MySqlPoolOptions;
    use tokio::sync::Notify;

    use super::*;
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;

    const BLOCKING_DESCRIPTOR: SourceDescriptor = SourceDescriptor {
        source_key: "blocking-test-wiki",
        display_name: "Blocking Test Wiki",
        allowed_host: "example.test",
        series_name: "Blocking Import Test",
        series_slug: "blocking-import-test",
        series_record_id: "Series:Blocking",
        series_url: "https://example.test/series",
    };

    struct BlockingAdapter {
        entered_fetch: Arc<Notify>,
        release_fetch: Arc<Notify>,
    }

    #[async_trait]
    impl WikiAdapter for BlockingAdapter {
        fn name(&self) -> &'static str {
            "blocking-test"
        }

        fn display_name(&self) -> &'static str {
            "Blocking Test"
        }

        fn version(&self) -> &'static str {
            "1.0"
        }

        fn source_descriptor(&self) -> SourceDescriptor {
            BLOCKING_DESCRIPTOR
        }

        async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
            self.entered_fetch.notify_one();
            self.release_fetch.notified().await;
            Ok(SeriesData {
                name: BLOCKING_DESCRIPTOR.series_name.to_string(),
                slug: BLOCKING_DESCRIPTOR.series_slug.to_string(),
                publisher: None,
                genre: None,
                frequency: None,
                total_issues: None,
                status: SeriesStatus::Running,
                source: SourceReference {
                    source_key: BLOCKING_DESCRIPTOR.source_key.to_string(),
                    source_record_id: BLOCKING_DESCRIPTOR.series_record_id.to_string(),
                    source_url: BLOCKING_DESCRIPTOR.series_url.to_string(),
                },
            })
        }

        async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
            Ok(vec![1])
        }

        async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
            Err(AdapterError::Other(format!(
                "unexpected issue fetch for {issue_number}"
            )))
        }

        async fn fetch_cover(&self, _issue_number: u32) -> Result<Option<CoverData>, AdapterError> {
            Ok(None)
        }
    }

    fn issue_data(title: &str) -> IssueData {
        IssueData {
            issue_number: 1,
            title: title.to_string(),
            authors: vec!["Author".to_string()],
            published_at: NaiveDate::from_ymd_opt(2026, 1, 1),
            part_number: None,
            part_total: None,
            cycle: None,
            cover_artists: Vec::new(),
            keywords: Vec::new(),
            notes: Vec::new(),
            source: lilly_importer_core::SourceReference {
                source_key: "wiki".to_string(),
                source_record_id: "Issue:1".to_string(),
                source_url: "https://example.test/Issue:1".to_string(),
            },
        }
    }

    fn issue_response(title: &str) -> IssueResponse {
        IssueResponse {
            id: 10,
            series_id: 1,
            issue_number: 1,
            title: title.to_string(),
            authors: vec!["Author".to_string()],
            published_at: NaiveDate::from_ymd_opt(2026, 1, 1),
            part_number: None,
            part_total: None,
            cycle: None,
            cover_artists: Vec::new(),
            keywords: Vec::new(),
            notes: Vec::new(),
            cover_url: None,
            cover_local_path: None,
            source_key: Some("wiki".to_string()),
            source_record_id: Some("Issue:1".to_string()),
            source_wiki_url: Some("https://example.test/Issue:1".to_string()),
        }
    }

    #[test]
    fn source_numbers_are_sorted_and_reject_invalid_or_duplicate_values() {
        assert_eq!(normalize_source_numbers(vec![3, 1, 2]).unwrap(), vec![1, 2, 3]);
        assert!(normalize_source_numbers(Vec::new()).is_err());
        assert!(normalize_source_numbers(vec![0, 1]).is_err());
        assert!(normalize_source_numbers(vec![1, 1]).is_err());
    }

    #[test]
    fn comparison_distinguishes_created_updated_and_unchanged() {
        let original = issue_response("Original");
        assert_eq!(classify_issue(&issue_data("Original"), None), IssueOutcome::Created);
        assert_eq!(
            classify_issue(&issue_data("Original"), Some(&original)),
            IssueOutcome::Unchanged
        );
        assert_eq!(
            classify_issue(&issue_data("Changed"), Some(&original)),
            IssueOutcome::Updated
        );
    }

    #[test]
    fn publication_filter_skips_unknown_and_future_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        assert!(!is_published(None, today));
        assert!(is_published(Some(today), today));
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
    }

    #[test]
    fn summarize_errors_caps_inline_output() {
        assert_eq!(summarize_errors(&[]), None);
        let errors = (1..=12)
            .map(|number| format!("error {number}"))
            .collect::<Vec<_>>();
        let summary = summarize_errors(&errors).unwrap();
        assert!(summary.contains("and 2 more errors"));
        assert!(!summary.contains("error 11"));
    }

    #[test]
    fn progress_invariant_requires_every_source_issue_to_have_an_outcome() {
        let complete = ImportProgress {
            total: 5,
            created: 1,
            updated: 1,
            unchanged: 1,
            skipped: 1,
            failed: 1,
        };
        assert!(validate_progress(complete).is_ok());
        assert!(validate_progress(ImportProgress { total: 6, ..complete }).is_err());
    }

    #[test]
    fn missing_source_summary_is_bounded() {
        let numbers = (1..=22).collect::<Vec<_>>();
        let summary = summarize_missing_source_numbers(&numbers);
        assert!(summary.contains("and 2 more"));
        assert!(!summary.contains("21, 22"));
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

    #[tokio::test]
    async fn start_returns_while_fetch_is_blocked_and_cancel_prevents_persistence() {
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
        sqlx::query("DELETE FROM series WHERE slug = ?")
            .bind(BLOCKING_DESCRIPTOR.series_slug)
            .execute(&pool)
            .await
            .expect("old blocking test fixture must be removable");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role) VALUES (?, 'Blocking Tester', 'admin')",
        )
        .bind(format!("blocking-import-{suffix}@example.test"))
        .execute(&pool)
        .await
        .expect("user fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("user fixture ID must fit u32");

        let entered_fetch = Arc::new(Notify::new());
        let release_fetch = Arc::new(Notify::new());
        let mut adapter_registry = AdapterRegistry::new();
        adapter_registry.register(Box::new(BlockingAdapter {
            entered_fetch: entered_fetch.clone(),
            release_fetch: release_fetch.clone(),
        }));
        let state = Arc::new(AppStateInner {
            pool: pool.clone(),
            jwt_secret: "test-secret".to_string(),
            jwt_access_expiry: 900,
            jwt_refresh_expiry: 2_592_000,
            email_service: EmailService::Log {
                from: "test@example.test".to_string(),
            },
            app_base_url: "http://localhost".to_string(),
            cookie_secure: false,
            adapter_registry,
            media_path: PathBuf::from("/tmp/lilly-blocking-import-test"),
            media_url_prefix: "/media".to_string(),
            import_scheduler_config: ImportSchedulerConfig {
                enabled: false,
                schedule: "0 10 6 * * Sat *".to_string(),
                timezone: "Europe/Berlin".to_string(),
                adapters: Vec::new(),
            },
        });

        let job = tokio::time::timeout(
            Duration::from_secs(1),
            start_import(
                state,
                "blocking-test",
                ImportTrigger::Manual { user_id },
            ),
        )
        .await
        .expect("job start must not wait for the blocked adapter fetch")
        .expect("job start must succeed");
        tokio::time::timeout(Duration::from_secs(1), entered_fetch.notified())
            .await
            .expect("background worker must enter the adapter fetch");

        assert!(
            import_jobs::request_import_cancellation(&pool, job.id)
                .await
                .unwrap()
        );
        release_fetch.notify_one();

        let mut final_status = String::new();
        for _ in 0..100 {
            final_status = import_jobs::find_import_job_by_id(&pool, job.id)
                .await
                .unwrap()
                .unwrap()
                .status;
            if final_status == "cancelled" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(final_status, "cancelled");
        assert_eq!(issues::count_issues_by_series(&pool, job.series_id).await.unwrap(), 0);

        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(job.series_id)
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
