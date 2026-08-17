use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use lilly_importer_core::{
    AdapterError, CoverData, CoverFetchResult, CoverIdentity, IssueData, SourceDescriptor,
    WikiAdapter, normalize_and_validate_issue, normalize_and_validate_series,
    validate_reference_record, validate_source_reference,
};

use crate::db::import_jobs::ImportProgress;
use crate::db::{import_jobs, import_review, issues, series};
use crate::error::AppError;
use crate::models::series::{ImportJobResponse, Issue, IssueResponse};
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

impl IssueOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
        }
    }
}

#[derive(Debug)]
enum PreparedIssue {
    Future(IssueData),
    Published(IssueData),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CoverImportResult {
    status: &'static str,
    local_path: Option<String>,
    reason: Option<String>,
    persistence: CoverPersistence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CoverPersistence {
    Keep,
    Set {
        local_path: String,
        identity: CoverIdentity,
        created_file: Option<PathBuf>,
    },
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredCover {
    local_path: Option<String>,
    remote_url: Option<String>,
    source_file: Option<String>,
    source_sha1: Option<String>,
    source_updated_at: Option<chrono::NaiveDateTime>,
}

impl StoredCover {
    fn preferred_path(&self) -> Option<String> {
        self.local_path.clone().or_else(|| self.remote_url.clone())
    }

    fn matches(&self, identity: &CoverIdentity) -> bool {
        self.source_file.as_deref() == Some(identity.file_name.as_str())
            && self.source_sha1.as_deref() == Some(identity.source_sha1.as_str())
            && self.source_updated_at == Some(identity.source_updated_at.naive_utc())
    }

    fn has_persisted_value(&self) -> bool {
        self.local_path.is_some()
            || self.remote_url.is_some()
            || self.source_file.is_some()
            || self.source_sha1.is_some()
            || self.source_updated_at.is_some()
    }
}

impl From<&Issue> for StoredCover {
    fn from(issue: &Issue) -> Self {
        Self {
            local_path: issue.cover_local_path.clone(),
            remote_url: issue.cover_url.clone(),
            source_file: issue.cover_source_file.clone(),
            source_sha1: issue.cover_source_sha1.clone(),
            source_updated_at: issue.cover_source_updated_at,
        }
    }
}

impl CoverImportResult {
    fn imported(local_path: String, identity: CoverIdentity, created_file: PathBuf) -> Self {
        Self {
            status: "imported",
            local_path: Some(local_path.clone()),
            reason: None,
            persistence: CoverPersistence::Set {
                local_path,
                identity,
                created_file: Some(created_file),
            },
        }
    }

    fn reused(existing: Option<&StoredCover>, identity: Option<CoverIdentity>) -> Self {
        let local_path = existing.and_then(StoredCover::preferred_path);
        let persistence = match (existing, identity) {
            (Some(existing), Some(identity)) if !existing.matches(&identity) => {
                if let Some(local_path) = existing.local_path.clone() {
                    CoverPersistence::Set {
                        local_path,
                        identity,
                        created_file: None,
                    }
                } else {
                    CoverPersistence::Keep
                }
            }
            _ => CoverPersistence::Keep,
        };
        Self {
            status: "reused",
            local_path,
            reason: None,
            persistence,
        }
    }

    fn warning_keep(status: &'static str, reason: String, existing: Option<&StoredCover>) -> Self {
        Self {
            status,
            local_path: existing.and_then(StoredCover::preferred_path),
            reason: Some(reason),
            persistence: CoverPersistence::Keep,
        }
    }

    fn missing(existing: Option<&StoredCover>, reason: String) -> Self {
        Self {
            status: "missing_at_source",
            local_path: None,
            reason: Some(reason),
            persistence: if existing.is_some_and(StoredCover::has_persisted_value) {
                CoverPersistence::Clear
            } else {
                CoverPersistence::Keep
            },
        }
    }

    fn severity(&self) -> &'static str {
        if matches!(
            self.status,
            "missing_at_source" | "not_permitted" | "fetch_failed" | "invalid" | "storage_failed"
        ) {
            "warning"
        } else {
            "info"
        }
    }

    fn requires_persistence(&self) -> bool {
        !matches!(self.persistence, CoverPersistence::Keep)
    }

    fn created_file(&self) -> Option<&Path> {
        match &self.persistence {
            CoverPersistence::Set {
                created_file: Some(path),
                ..
            } => Some(path),
            _ => None,
        }
    }
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
    if !matches!(
        previous.status.as_str(),
        "failed" | "cancelled" | "interrupted"
    ) {
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
        &import_jobs::NewImportJob {
            series_id,
            adapter_name,
            source_key: descriptor.source_key,
            started_by,
            trigger_type,
            scheduled_for,
            retry_of_job_id,
        },
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
    let source_key = state.adapter_registry.get(&adapter_name).map_or_else(
        || adapter_name.clone(),
        |adapter| adapter.source_descriptor().source_key.to_string(),
    );
    tokio::spawn(async move {
        if let Err(error) =
            execute_import(state, series_id, job_id, &adapter_name, trigger_type).await
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

    if let Some(existing) = series::find_series_by_slug(&state.pool, descriptor.series_slug).await?
    {
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

#[allow(clippy::too_many_lines)]
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

    let fetched_source_numbers = match fetch_issue_list_with_retry(adapter).await {
        Ok(numbers) => numbers,
        Err(error) => {
            let message = format!("Failed to fetch issue list: {error}");
            import_jobs::record_import_error(
                &state.pool,
                job_id,
                descriptor.source_key,
                None,
                None,
                "list",
                &message,
            )
            .await?;
            import_jobs::fail_import_job(&state.pool, job_id, &message).await?;
            cancel_if_requested(&state, job_id).await?;
            return Ok(());
        }
    };
    let source_numbers = match normalize_source_numbers(&fetched_source_numbers) {
        Ok(numbers) => numbers,
        Err(error) => {
            let message = error.to_string();
            import_jobs::record_import_error(
                &state.pool,
                job_id,
                descriptor.source_key,
                None,
                None,
                "list",
                &message,
            )
            .await?;
            import_jobs::fail_import_job(&state.pool, job_id, &message).await?;
            cancel_if_requested(&state, job_id).await?;
            return Ok(());
        }
    };
    if cancel_if_requested(&state, job_id).await? {
        return Ok(());
    }

    import_review::seed_import_results(&state.pool, job_id, descriptor.source_key, &source_numbers)
        .await?;

    let stored_rows = issues::find_all_issues_by_series(&state.pool, series_id).await?;
    let stored_covers: HashMap<u32, StoredCover> = stored_rows
        .iter()
        .map(|issue| (issue.issue_number, StoredCover::from(issue)))
        .collect();
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
        import_jobs::record_import_finding(
            &state.pool,
            job_id,
            descriptor.source_key,
            None,
            None,
            "source-list",
            "warning",
            "source_issue_missing",
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
    let reference_records = adapter.reference_records();

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
                    IssueFailure {
                        source_key: descriptor.source_key,
                        issue_number,
                        source_record_id: None,
                        processing_stage: "fetch",
                        message: &error.to_string(),
                        details: None,
                    },
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
        let raw_details = details.clone();
        let details = match prepare_issue_for_import(descriptor, issue_number, details, today) {
            Ok(PreparedIssue::Future(details)) => {
                progress.skipped = progress.skipped.saturating_add(1);
                record_review_result(
                    &state,
                    job_id,
                    descriptor.source_key,
                    &details,
                    None,
                    "skipped",
                    "info",
                    "publication-date",
                    Some("Issue has not been published yet"),
                    &CoverImportResult {
                        status: "not_checked",
                        local_path: None,
                        reason: Some("Cover was not checked for a future issue".to_string()),
                        persistence: CoverPersistence::Keep,
                    },
                )
                .await?;
                import_jobs::update_import_progress(&state.pool, job_id, progress).await?;
                continue;
            }
            Ok(PreparedIssue::Published(details)) => details,
            Err(error) => {
                record_issue_failure(
                    &state,
                    job_id,
                    IssueFailure {
                        source_key: descriptor.source_key,
                        issue_number,
                        source_record_id: Some(&source_record_id),
                        processing_stage: "validate",
                        message: &error.to_string(),
                        details: Some(&raw_details),
                    },
                    &mut progress,
                    &mut error_messages,
                )
                .await?;
                continue;
            }
        };

        if let Err(error) = validate_reference_record(&reference_records, &details) {
            record_issue_failure(
                &state,
                job_id,
                IssueFailure {
                    source_key: descriptor.source_key,
                    issue_number,
                    source_record_id: Some(&details.source.source_record_id),
                    processing_stage: "reference",
                    message: &error.to_string(),
                    details: Some(&details),
                },
                &mut progress,
                &mut error_messages,
            )
            .await?;
            continue;
        }

        let existing = stored.get(&issue_number);
        let existing_cover = stored_covers.get(&issue_number);
        let outcome = classify_issue(&details, existing);
        if cancel_if_requested(&state, job_id).await? {
            return Ok(());
        }
        let cover_result = fetch_and_store_cover(
            adapter,
            issue_number,
            &cover_dir,
            &state.media_url_prefix,
            series_id,
            existing_cover,
        )
        .await;
        if cover_result.severity() == "warning" {
            let message = cover_result
                .reason
                .as_deref()
                .unwrap_or("Cover is unavailable");
            import_jobs::record_import_finding(
                &state.pool,
                job_id,
                descriptor.source_key,
                Some(issue_number),
                Some(&details.source.source_record_id),
                "cover",
                "warning",
                cover_result.status,
                message,
            )
            .await?;
        }
        if cancel_if_requested(&state, job_id).await? {
            discard_created_cover(&cover_result).await;
            return Ok(());
        }

        if outcome == IssueOutcome::Unchanged && !cover_result.requires_persistence() {
            if let Some(existing) = existing {
                issues::mark_issue_checked(&state.pool, existing.id).await?;
                record_review_result(
                    &state,
                    job_id,
                    descriptor.source_key,
                    &details,
                    Some(existing.id),
                    outcome.as_str(),
                    cover_result.severity(),
                    "complete",
                    cover_result.reason.as_deref(),
                    &cover_result,
                )
                .await?;
            }
            progress.unchanged = progress.unchanged.saturating_add(1);
            import_jobs::update_import_progress(&state.pool, job_id, progress).await?;
            continue;
        }

        let issue_id =
            match persist_issue(&state, series_id, &details, &cover_result.persistence).await {
                Ok(issue_id) => issue_id,
                Err(error) => {
                    discard_created_cover(&cover_result).await;
                    record_issue_failure(
                        &state,
                        job_id,
                        IssueFailure {
                            source_key: descriptor.source_key,
                            issue_number,
                            source_record_id: Some(&details.source.source_record_id),
                            processing_stage: "persist",
                            message: &error.to_string(),
                            details: Some(&details),
                        },
                        &mut progress,
                        &mut error_messages,
                    )
                    .await?;
                    continue;
                }
            };
        cleanup_replaced_cover(existing_cover, &cover_result.persistence, &cover_dir).await;

        record_review_result(
            &state,
            job_id,
            descriptor.source_key,
            &details,
            Some(issue_id),
            outcome.as_str(),
            cover_result.severity(),
            "complete",
            cover_result.reason.as_deref(),
            &cover_result,
        )
        .await?;

        match outcome {
            IssueOutcome::Created => progress.created = progress.created.saturating_add(1),
            IssueOutcome::Updated => progress.updated = progress.updated.saturating_add(1),
            IssueOutcome::Unchanged => {
                progress.unchanged = progress.unchanged.saturating_add(1);
            }
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
    validate_review_progress(&state, job_id, progress).await?;
    let completed =
        import_jobs::complete_import_job(&state.pool, job_id, progress, summary.as_deref()).await?;
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

struct IssueFailure<'a> {
    source_key: &'a str,
    issue_number: u32,
    source_record_id: Option<&'a str>,
    processing_stage: &'a str,
    message: &'a str,
    details: Option<&'a IssueData>,
}

async fn record_issue_failure(
    state: &AppStateInner,
    job_id: u32,
    failure: IssueFailure<'_>,
    progress: &mut ImportProgress,
    error_messages: &mut Vec<String>,
) -> Result<(), sqlx::Error> {
    progress.failed = progress.failed.saturating_add(1);
    error_messages.push(format!(
        "#{} [{}]: {}",
        failure.issue_number, failure.processing_stage, failure.message
    ));
    import_jobs::record_import_error(
        &state.pool,
        job_id,
        failure.source_key,
        Some(failure.issue_number),
        failure.source_record_id,
        failure.processing_stage,
        failure.message,
    )
    .await?;
    let empty = Vec::new();
    let details = failure.details;
    import_review::record_import_result(
        &state.pool,
        job_id,
        failure.source_key,
        &import_review::ReviewResultUpdate {
            issue_id: None,
            issue_number: failure.issue_number,
            outcome: "failed",
            severity: "blocking",
            stage: failure.processing_stage,
            message: Some(failure.message),
            source_record_id: failure.source_record_id,
            source_url: details.map(|details| details.source.source_url.as_str()),
            title: details.map(|details| details.title.as_str()),
            authors: details.map_or(empty.as_slice(), |details| details.authors.as_slice()),
            cover_artists: details
                .map_or(empty.as_slice(), |details| details.cover_artists.as_slice()),
            published_at: details.and_then(|details| details.published_at),
            part_number: details.and_then(|details| details.part_number),
            part_total: details.and_then(|details| details.part_total),
            cycle: details.and_then(|details| details.cycle.as_deref()),
            cover_status: "not_checked",
            cover_reason: None,
            cover_local_path: None,
        },
    )
    .await?;
    import_jobs::update_import_progress(&state.pool, job_id, *progress).await
}

#[allow(clippy::too_many_arguments)]
async fn record_review_result(
    state: &AppStateInner,
    job_id: u32,
    source_key: &str,
    details: &IssueData,
    issue_id: Option<u32>,
    outcome: &str,
    severity: &str,
    result_stage: &str,
    message: Option<&str>,
    cover: &CoverImportResult,
) -> Result<(), sqlx::Error> {
    import_review::record_import_result(
        &state.pool,
        job_id,
        source_key,
        &import_review::ReviewResultUpdate {
            issue_id,
            issue_number: details.issue_number,
            outcome,
            severity,
            stage: result_stage,
            message,
            source_record_id: Some(&details.source.source_record_id),
            source_url: Some(&details.source.source_url),
            title: Some(&details.title),
            authors: &details.authors,
            cover_artists: &details.cover_artists,
            published_at: details.published_at,
            part_number: details.part_number,
            part_total: details.part_total,
            cycle: details.cycle.as_deref(),
            cover_status: cover.status,
            cover_reason: cover.reason.as_deref(),
            cover_local_path: cover.local_path.as_deref(),
        },
    )
    .await
}

fn normalize_source_numbers(numbers: &[u32]) -> Result<Vec<u32>, anyhow::Error> {
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

async fn fetch_issue_list_with_retry(adapter: &dyn WikiAdapter) -> Result<Vec<u32>, AdapterError> {
    let mut attempt = 1u8;
    loop {
        match adapter.fetch_issue_list().await {
            Ok(issue_numbers) => return Ok(issue_numbers),
            Err(error) if attempt < MAX_FETCH_ATTEMPTS && is_transient(&error) => {
                tracing::warn!(
                    adapter = adapter.name(),
                    attempt,
                    max_attempts = MAX_FETCH_ATTEMPTS,
                    error = %error,
                    "Transient issue list fetch failed; retrying"
                );
                tokio::time::sleep(Duration::from_millis(250 * u64::from(attempt))).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_transient(error: &AdapterError) -> bool {
    match error {
        AdapterError::RateLimited => true,
        AdapterError::Network(error) => error
            .status()
            .is_none_or(|status| matches!(status.as_u16(), 408 | 429 | 500..=599)),
        _ => false,
    }
}

async fn fetch_and_store_cover(
    adapter: &dyn WikiAdapter,
    issue_number: u32,
    cover_dir: &Path,
    media_url_prefix: &str,
    series_id: u32,
    existing: Option<&StoredCover>,
) -> CoverImportResult {
    let known_source_sha1 = if let Some(existing) = existing
        && let (Some(local_path), Some(source_sha1)) = (
            existing.local_path.as_deref(),
            existing.source_sha1.as_deref(),
        )
        && let Some(stored_file) = stored_cover_file(cover_dir, local_path)
        && tokio::fs::try_exists(stored_file).await.unwrap_or(false)
    {
        Some(source_sha1)
    } else {
        None
    };
    let (cover, identity) = match adapter.fetch_cover(issue_number, known_source_sha1).await {
        Ok(CoverFetchResult::Missing) => {
            return CoverImportResult::missing(
                existing,
                "The source does not provide an unambiguous canonical cover".to_string(),
            );
        }
        Ok(CoverFetchResult::Unchanged(identity)) => {
            if existing
                .and_then(|cover| cover.local_path.as_ref())
                .is_none()
            {
                return CoverImportResult::warning_keep(
                    "fetch_failed",
                    "The adapter reported an unchanged cover without a stored local file"
                        .to_string(),
                    existing,
                );
            }
            return CoverImportResult::reused(existing, Some(identity));
        }
        Ok(CoverFetchResult::Downloaded { data, identity }) => (data, identity),
        Err(error) => {
            let message = format!("Failed to fetch cover: {error}");
            let status = if matches!(error, AdapterError::Parse(_)) {
                "invalid"
            } else if message.to_ascii_lowercase().contains("permission")
                || message.to_ascii_lowercase().contains("copyright")
            {
                "not_permitted"
            } else {
                "fetch_failed"
            };
            return CoverImportResult::warning_keep(status, message, existing);
        }
    };

    store_downloaded_cover(
        cover,
        identity,
        issue_number,
        cover_dir,
        media_url_prefix,
        series_id,
        existing,
    )
    .await
}

async fn store_downloaded_cover(
    cover: CoverData,
    identity: CoverIdentity,
    issue_number: u32,
    cover_dir: &Path,
    media_url_prefix: &str,
    series_id: u32,
    existing: Option<&StoredCover>,
) -> CoverImportResult {
    let Some(extension) = cover_extension(&cover.content_type) else {
        return CoverImportResult::warning_keep(
            "invalid",
            format!("Unsupported cover content type '{}'", cover.content_type),
            existing,
        );
    };
    if let Err(error) = tokio::fs::create_dir_all(cover_dir).await {
        return CoverImportResult::warning_keep(
            "storage_failed",
            format!("Failed to create cover directory: {error}"),
            existing,
        );
    }
    let file_name = format!("{issue_number}-{}.{}", identity.source_sha1, extension);
    let target = cover_dir.join(&file_name);
    let created_file = if tokio::fs::try_exists(&target).await.unwrap_or(false) {
        None
    } else {
        let temporary =
            temporary_cover_path(cover_dir, issue_number, &identity.source_sha1, extension);
        if let Err(error) = tokio::fs::write(&temporary, &cover.bytes).await {
            return CoverImportResult::warning_keep(
                "storage_failed",
                format!("Failed to write temporary cover: {error}"),
                existing,
            );
        }
        if let Err(error) = tokio::fs::rename(&temporary, &target).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return CoverImportResult::warning_keep(
                "storage_failed",
                format!("Failed to atomically store cover: {error}"),
                existing,
            );
        }
        Some(target)
    };

    let local_path = format!("{media_url_prefix}/covers/series-{series_id}/{file_name}");
    match created_file {
        Some(created_file) => CoverImportResult::imported(local_path, identity, created_file),
        None => CoverImportResult {
            status: "imported",
            local_path: Some(local_path.clone()),
            reason: None,
            persistence: CoverPersistence::Set {
                local_path,
                identity,
                created_file: None,
            },
        },
    }
}

async fn discard_created_cover(cover: &CoverImportResult) {
    if let Some(path) = cover.created_file()
        && let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), %error, "Failed to discard uncommitted cover file");
    }
}

async fn cleanup_replaced_cover(
    existing: Option<&StoredCover>,
    update: &CoverPersistence,
    cover_dir: &Path,
) {
    let Some(old_local_path) = existing.and_then(|cover| cover.local_path.as_deref()) else {
        return;
    };
    let replacement = match update {
        CoverPersistence::Keep => return,
        CoverPersistence::Set { local_path, .. } => Some(local_path.as_str()),
        CoverPersistence::Clear => None,
    };
    if replacement == Some(old_local_path) {
        return;
    }
    let Some(old_file) = stored_cover_file(cover_dir, old_local_path) else {
        return;
    };
    if let Err(error) = tokio::fs::remove_file(&old_file).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %old_file.display(), %error, "Failed to remove replaced cover file");
    }
}

fn stored_cover_file(cover_dir: &Path, local_path: &str) -> Option<PathBuf> {
    Path::new(local_path)
        .file_name()
        .map(|file_name| cover_dir.join(file_name))
}

async fn persist_issue(
    state: &AppStateInner,
    series_id: u32,
    details: &IssueData,
    cover: &CoverPersistence,
) -> Result<u32, anyhow::Error> {
    let cover = match cover {
        CoverPersistence::Keep => issues::CoverUpdate::Keep,
        CoverPersistence::Set {
            local_path,
            identity,
            ..
        } => issues::CoverUpdate::Set {
            local_path,
            source_file: &identity.file_name,
            source_sha1: &identity.source_sha1,
            source_updated_at: identity.source_updated_at.naive_utc(),
        },
        CoverPersistence::Clear => issues::CoverUpdate::Clear,
    };
    let issue_id = issues::replace_issue_metadata(
        &state.pool,
        &issues::IssueMetadataUpdate {
            series_id,
            issue_number: details.issue_number,
            title: &details.title,
            published_at: details.published_at,
            part_number: details.part_number,
            part_total: details.part_total,
            cycle: details.cycle.as_deref(),
            cover,
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
    Ok(issue_id)
}

fn prepare_issue_for_import(
    descriptor: SourceDescriptor,
    expected_issue_number: u32,
    issue: IssueData,
    today: NaiveDate,
) -> Result<PreparedIssue, AdapterError> {
    if issue.published_at.is_some_and(|date| date > today) {
        if issue.issue_number == 0 || issue.issue_number != expected_issue_number {
            return Err(AdapterError::Parse(format!(
                "Returned issue number {} does not match requested issue {expected_issue_number}",
                issue.issue_number
            )));
        }
        validate_source_reference(descriptor, &issue.source)?;
        return Ok(PreparedIssue::Future(issue));
    }

    normalize_and_validate_issue(descriptor, expected_issue_number, issue)
        .map(PreparedIssue::Published)
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
    cover_dir: &Path,
    issue_number: u32,
    source_sha1: &str,
    extension: &str,
) -> PathBuf {
    cover_dir.join(format!(".{issue_number}-{source_sha1}.{extension}.tmp"))
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

async fn validate_review_progress(
    state: &AppStateInner,
    job_id: u32,
    progress: ImportProgress,
) -> Result<(), anyhow::Error> {
    let counts = import_review::review_outcome_counts(&state.pool, job_id).await?;
    if counts.total != progress.total
        || counts.not_processed != 0
        || counts.created != progress.created
        || counts.updated != progress.updated
        || counts.unchanged != progress.unchanged
        || counts.skipped != progress.skipped
        || counts.failed != progress.failed
    {
        return Err(anyhow::anyhow!(
            "Import review invariant violated for job {job_id}: results do not match progress"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, RwLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use async_trait::async_trait;
    use lilly_importer_core::{
        AdapterRegistry, CoverData, CoverFetchResult, CoverIdentity, ReferenceRecord, SeriesData,
        SeriesStatus, SourceReference,
    };
    use sqlx::mysql::MySqlPoolOptions;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

    const SYNC_DESCRIPTOR: SourceDescriptor = SourceDescriptor {
        source_key: "sync-test-wiki",
        display_name: "Synchronization Test Wiki",
        allowed_host: "example.test",
        series_name: "Synchronization Import Test",
        series_slug: "synchronization-import-test",
        series_record_id: "Series:Synchronization",
        series_url: "https://example.test/series/synchronization",
    };

    const MADDRAX_REFERENCE_DESCRIPTOR: SourceDescriptor = SourceDescriptor {
        source_key: "reference-maddrax-test",
        display_name: "Maddrax Reference Persistence Test",
        allowed_host: "example.test",
        series_name: "Maddrax Reference Persistence Test",
        series_slug: "maddrax-reference-persistence-test",
        series_record_id: "Series:MaddraxReferencePersistence",
        series_url: "https://example.test/series/maddrax-reference-persistence",
    };

    const JOHN_REFERENCE_DESCRIPTOR: SourceDescriptor = SourceDescriptor {
        source_key: "reference-john-sinclair-test",
        display_name: "John Sinclair Reference Persistence Test",
        allowed_host: "example.test",
        series_name: "John Sinclair Reference Persistence Test",
        series_slug: "john-sinclair-reference-persistence-test",
        series_record_id: "Series:JohnSinclairReferencePersistence",
        series_url: "https://example.test/series/john-sinclair-reference-persistence",
    };

    struct ReferenceSnapshotAdapter {
        name: &'static str,
        descriptor: SourceDescriptor,
        references: Vec<ReferenceRecord>,
    }

    #[async_trait]
    impl WikiAdapter for ReferenceSnapshotAdapter {
        fn name(&self) -> &'static str {
            self.name
        }

        fn display_name(&self) -> &'static str {
            self.descriptor.display_name
        }

        fn version(&self) -> &'static str {
            "reference-snapshot-v1"
        }

        fn source_descriptor(&self) -> SourceDescriptor {
            self.descriptor
        }

        fn reference_records(&self) -> Vec<ReferenceRecord> {
            self.references.clone()
        }

        async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
            Ok(SeriesData {
                name: self.descriptor.series_name.to_string(),
                slug: self.descriptor.series_slug.to_string(),
                publisher: Some("Reference Fixture Publisher".to_string()),
                genre: Some("Reference Fixture Genre".to_string()),
                frequency: None,
                total_issues: Some(self.references.len().try_into().unwrap()),
                status: SeriesStatus::Running,
                source: SourceReference {
                    source_key: self.descriptor.source_key.to_string(),
                    source_record_id: self.descriptor.series_record_id.to_string(),
                    source_url: self.descriptor.series_url.to_string(),
                },
            })
        }

        async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
            Ok(self
                .references
                .iter()
                .map(|reference| reference.issue_number)
                .collect())
        }

        async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
            let reference = self
                .references
                .iter()
                .find(|reference| reference.issue_number == issue_number)
                .ok_or_else(|| AdapterError::NotFound(format!("issue {issue_number}")))?;
            Ok(IssueData {
                issue_number,
                title: reference.title.to_string(),
                authors: reference.authors.iter().map(ToString::to_string).collect(),
                published_at: Some(reference.published_at),
                part_number: None,
                part_total: None,
                cycle: None,
                cover_artists: Vec::new(),
                keywords: Vec::new(),
                notes: Vec::new(),
                source: SourceReference {
                    source_key: self.descriptor.source_key.to_string(),
                    source_record_id: format!("Issue:{issue_number}"),
                    source_url: format!("https://example.test/issues/{issue_number}"),
                },
            })
        }

        async fn fetch_cover(
            &self,
            _issue_number: u32,
            _known_source_sha1: Option<&str>,
        ) -> Result<CoverFetchResult, AdapterError> {
            Ok(CoverFetchResult::Missing)
        }
    }

    struct BlockingAdapter {
        entered_fetch: Arc<Notify>,
        release_fetch: Arc<Notify>,
    }

    enum ListFailureKind {
        RateLimited,
        NonTransient,
        HttpServer(String),
    }

    struct ListRetryAdapter {
        calls: AtomicUsize,
        failures_before_success: usize,
        failure_kind: ListFailureKind,
    }

    impl ListRetryAdapter {
        fn new(failures_before_success: usize, failure_kind: ListFailureKind) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                failures_before_success,
                failure_kind,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl WikiAdapter for ListRetryAdapter {
        fn name(&self) -> &'static str {
            "list-retry-test"
        }

        fn display_name(&self) -> &'static str {
            "List Retry Test"
        }

        fn version(&self) -> &'static str {
            "1.0"
        }

        fn source_descriptor(&self) -> SourceDescriptor {
            SYNC_DESCRIPTOR
        }

        async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
            Err(AdapterError::Other(
                "unexpected series metadata fetch".to_string(),
            ))
        }

        async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if let ListFailureKind::HttpServer(url) = &self.failure_kind {
                let response = reqwest::get(url).await?.error_for_status()?;
                return Ok(response.json().await?);
            }
            if call >= self.failures_before_success {
                return Ok(vec![3, 1, 2]);
            }

            match &self.failure_kind {
                ListFailureKind::RateLimited => Err(AdapterError::RateLimited),
                ListFailureKind::NonTransient => {
                    Err(AdapterError::Parse("invalid issue list".to_string()))
                }
                ListFailureKind::HttpServer(_) => unreachable!("handled before scripted failures"),
            }
        }

        async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
            Err(AdapterError::Other(format!(
                "unexpected issue fetch for {issue_number}"
            )))
        }

        async fn fetch_cover(
            &self,
            issue_number: u32,
            _known_source_sha1: Option<&str>,
        ) -> Result<CoverFetchResult, AdapterError> {
            Err(AdapterError::Other(format!(
                "unexpected cover fetch for {issue_number}"
            )))
        }
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

        async fn fetch_cover(
            &self,
            _issue_number: u32,
            _known_source_sha1: Option<&str>,
        ) -> Result<CoverFetchResult, AdapterError> {
            Ok(CoverFetchResult::Missing)
        }
    }

    type SyncCover = Result<(Vec<u8>, String, String), String>;

    #[derive(Clone)]
    struct SyncScenario {
        issue_list_error: Option<String>,
        issues: BTreeMap<u32, Result<IssueData, String>>,
        covers: BTreeMap<u32, SyncCover>,
        cover_not_found: BTreeSet<u32>,
    }

    struct SyncAdapter {
        scenario: Arc<RwLock<SyncScenario>>,
        cover_downloads: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WikiAdapter for SyncAdapter {
        fn name(&self) -> &'static str {
            "sync-test"
        }

        fn display_name(&self) -> &'static str {
            "Synchronization Test"
        }

        fn version(&self) -> &'static str {
            "1.0"
        }

        fn source_descriptor(&self) -> SourceDescriptor {
            SYNC_DESCRIPTOR
        }

        async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
            Ok(SeriesData {
                name: SYNC_DESCRIPTOR.series_name.to_string(),
                slug: SYNC_DESCRIPTOR.series_slug.to_string(),
                publisher: Some("Test Publisher".to_string()),
                genre: Some("Test Genre".to_string()),
                frequency: Some("weekly".to_string()),
                total_issues: Some(3),
                status: SeriesStatus::Running,
                source: SourceReference {
                    source_key: SYNC_DESCRIPTOR.source_key.to_string(),
                    source_record_id: SYNC_DESCRIPTOR.series_record_id.to_string(),
                    source_url: SYNC_DESCRIPTOR.series_url.to_string(),
                },
            })
        }

        async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
            let scenario = self.scenario.read().expect("sync scenario lock poisoned");
            if let Some(error) = &scenario.issue_list_error {
                return Err(AdapterError::Other(error.clone()));
            }
            Ok(scenario.issues.keys().copied().collect())
        }

        async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
            let scenario = self.scenario.read().expect("sync scenario lock poisoned");
            match scenario.issues.get(&issue_number) {
                Some(Ok(issue)) => Ok(issue.clone()),
                Some(Err(error)) => Err(AdapterError::Other(error.clone())),
                None => Err(AdapterError::NotFound(format!("issue {issue_number}"))),
            }
        }

        async fn fetch_cover(
            &self,
            issue_number: u32,
            known_source_sha1: Option<&str>,
        ) -> Result<CoverFetchResult, AdapterError> {
            let scenario = self.scenario.read().expect("sync scenario lock poisoned");
            if scenario.cover_not_found.contains(&issue_number) {
                return Err(AdapterError::NotFound(format!(
                    "issue index unavailable for {issue_number}"
                )));
            }
            let Some(cover) = scenario.covers.get(&issue_number) else {
                return Ok(CoverFetchResult::Missing);
            };
            let (bytes, content_type, source_sha1) = cover
                .as_ref()
                .map_err(|error| AdapterError::Other(error.clone()))?;
            let identity = CoverIdentity {
                file_name: format!("{issue_number:03}tibi.png"),
                source_sha1: source_sha1.clone(),
                source_updated_at: "2026-08-14T12:00:00Z".parse().unwrap(),
            };
            if known_source_sha1 == Some(source_sha1.as_str()) {
                Ok(CoverFetchResult::Unchanged(identity))
            } else {
                self.cover_downloads.fetch_add(1, Ordering::SeqCst);
                Ok(CoverFetchResult::Downloaded {
                    data: CoverData {
                        bytes: bytes.clone(),
                        content_type: content_type.clone(),
                    },
                    identity,
                })
            }
        }
    }

    fn test_state(
        pool: sqlx::MySqlPool,
        adapter_registry: AdapterRegistry,
        media_path: &str,
    ) -> Arc<AppStateInner> {
        Arc::new(AppStateInner {
            pool,
            jwt_secret: "test-secret".to_string(),
            jwt_access_expiry: 900,
            jwt_refresh_expiry: 2_592_000,
            password_reset_ttl_seconds: 3_600,
            email_service: EmailService::Log {
                from: "test@example.test".to_string(),
            },
            app_base_url: "http://localhost".to_string(),
            cookie_secure: false,
            oauth_service: crate::services::oauth::OAuthService::disabled(),
            privacy_policy_version: "test-v1".to_string(),
            adapter_registry,
            media_path: PathBuf::from(media_path),
            media_url_prefix: "/media".to_string(),
            photo_upload_config: crate::config::PhotoUploadConfig::default(),
            media_storage: crate::services::media::MediaStorage::new(std::path::Path::new(
                media_path,
            )),
            erasure_ledger: crate::services::account_erasure::ErasureLedger::new(
                "/tmp/lilly-import-test-erasure-ledger",
            ),
            import_scheduler_config: ImportSchedulerConfig {
                enabled: false,
                schedule: "0 10 6 * * Sat *".to_string(),
                timezone: "Europe/Berlin".to_string(),
                adapters: Vec::new(),
            },
            request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
        })
    }

    fn sync_issue(
        issue_number: u32,
        title: &str,
        authors: Vec<String>,
        published_at: NaiveDate,
    ) -> IssueData {
        IssueData {
            issue_number,
            title: title.to_string(),
            authors,
            published_at: Some(published_at),
            part_number: None,
            part_total: None,
            cycle: Some("Test Cycle".to_string()),
            cover_artists: vec!["Test Artist".to_string()],
            keywords: vec!["Test Keyword".to_string()],
            notes: vec!["Test Note".to_string()],
            source: SourceReference {
                source_key: SYNC_DESCRIPTOR.source_key.to_string(),
                source_record_id: format!("Issue:{issue_number}"),
                source_url: format!("https://example.test/issues/{issue_number}"),
            },
        }
    }

    async fn wait_for_terminal_job(
        pool: &sqlx::MySqlPool,
        job_id: u32,
    ) -> crate::models::series::ImportJob {
        for _ in 0..200 {
            let job = import_jobs::find_import_job_by_id(pool, job_id)
                .await
                .expect("job lookup must succeed")
                .expect("job must exist");
            if matches!(
                job.status.as_str(),
                "completed" | "completed_with_errors" | "failed" | "cancelled" | "interrupted"
            ) {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("import job {job_id} did not reach a terminal status");
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

    async fn spawn_list_http_responses(
        statuses: Vec<u16>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture server must bind");
        let address = listener
            .local_addr()
            .expect("fixture server address must be available");
        let server = tokio::spawn(async move {
            for status in statuses {
                let (mut stream, _) = listener.accept().await.expect("request must connect");
                let mut request = vec![0_u8; 1024];
                let _length = stream
                    .read(&mut request)
                    .await
                    .expect("request must be readable");

                let body = if status == 200 {
                    "[3,1,2]"
                } else {
                    "temporary failure"
                };
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response must be writable");
            }
        });
        (format!("http://{address}/issues"), server)
    }

    #[test]
    fn source_numbers_are_sorted_and_reject_invalid_or_duplicate_values() {
        assert_eq!(normalize_source_numbers(&[3, 1, 2]).unwrap(), vec![1, 2, 3]);
        assert!(normalize_source_numbers(&[]).is_err());
        assert!(normalize_source_numbers(&[0, 1]).is_err());
        assert!(normalize_source_numbers(&[1, 1]).is_err());
    }

    #[test]
    fn comparison_distinguishes_created_updated_and_unchanged() {
        let original = issue_response("Original");
        assert_eq!(
            classify_issue(&issue_data("Original"), None),
            IssueOutcome::Created
        );
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
    fn future_issue_is_skipped_before_bibliographic_validation() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        let future = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
        let issue = sync_issue(695, "Future stub", Vec::new(), future);

        assert!(matches!(
            prepare_issue_for_import(SYNC_DESCRIPTOR, 695, issue, today),
            Ok(PreparedIssue::Future(_))
        ));
    }

    #[test]
    fn future_issue_still_requires_valid_identity_and_provenance() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        let future = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();

        let mismatched_number = sync_issue(696, "Wrong future issue", Vec::new(), future);
        let number_error =
            prepare_issue_for_import(SYNC_DESCRIPTOR, 695, mismatched_number, today).unwrap_err();
        assert_eq!(
            number_error.to_string(),
            "Parse error: Returned issue number 696 does not match requested issue 695"
        );

        let mut invalid_source = sync_issue(695, "Untrusted future issue", Vec::new(), future);
        invalid_source.source.source_url = "https://untrusted.example/issues/695".to_string();
        let source_error =
            prepare_issue_for_import(SYNC_DESCRIPTOR, 695, invalid_source, today).unwrap_err();
        assert_eq!(
            source_error.to_string(),
            "Parse error: Source host 'untrusted.example' is not allowed for 'sync-test-wiki'"
        );
    }

    #[test]
    fn published_or_undated_issue_still_requires_complete_metadata() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        let published = sync_issue(695, "Published stub", Vec::new(), today);
        let published_error =
            prepare_issue_for_import(SYNC_DESCRIPTOR, 695, published, today).unwrap_err();
        assert_eq!(
            published_error.to_string(),
            "Parse error: Issue 695 has no author"
        );

        let mut undated = sync_issue(
            695,
            "Undated issue",
            vec!["Known Author".to_string()],
            today,
        );
        undated.published_at = None;
        let undated_error =
            prepare_issue_for_import(SYNC_DESCRIPTOR, 695, undated, today).unwrap_err();
        assert_eq!(
            undated_error.to_string(),
            "Parse error: Issue 695 has no first publication date"
        );
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
        assert!(
            validate_progress(ImportProgress {
                total: 6,
                ..complete
            })
            .is_err()
        );
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
    async fn issue_list_retry_recovers_within_the_attempt_limit() {
        for transient_failures in 1..usize::from(MAX_FETCH_ATTEMPTS) {
            let adapter = ListRetryAdapter::new(transient_failures, ListFailureKind::RateLimited);

            let issue_numbers = fetch_issue_list_with_retry(&adapter)
                .await
                .expect("a transient list error must recover within the attempt limit");

            assert_eq!(issue_numbers, vec![3, 1, 2]);
            assert_eq!(adapter.calls(), transient_failures + 1);
        }
    }

    #[tokio::test]
    async fn issue_list_retry_returns_the_last_transient_error_after_three_attempts() {
        let adapter = ListRetryAdapter::new(usize::MAX, ListFailureKind::RateLimited);

        let error = fetch_issue_list_with_retry(&adapter)
            .await
            .expect_err("an unavailable source must fail after the attempt limit");

        assert!(matches!(error, AdapterError::RateLimited));
        assert_eq!(adapter.calls(), usize::from(MAX_FETCH_ATTEMPTS));
    }

    #[tokio::test]
    async fn issue_list_retry_recovers_from_real_http_server_errors() {
        let (url, server) = spawn_list_http_responses(vec![500, 599, 200]).await;
        let adapter = ListRetryAdapter::new(usize::MAX, ListFailureKind::HttpServer(url));

        let issue_numbers = fetch_issue_list_with_retry(&adapter)
            .await
            .expect("HTTP 500 responses must be retried as network errors");

        assert_eq!(issue_numbers, vec![3, 1, 2]);
        assert_eq!(adapter.calls(), usize::from(MAX_FETCH_ATTEMPTS));
        server.await.expect("fixture server must stop cleanly");
    }

    #[tokio::test]
    async fn issue_list_retry_repeats_only_retryable_http_client_errors() {
        for status in [408, 429] {
            let (url, server) = spawn_list_http_responses(vec![status, 200]).await;
            let adapter = ListRetryAdapter::new(usize::MAX, ListFailureKind::HttpServer(url));

            let issue_numbers = fetch_issue_list_with_retry(&adapter)
                .await
                .expect("HTTP 408 and 429 must be retried");

            assert_eq!(issue_numbers, vec![3, 1, 2]);
            assert_eq!(adapter.calls(), 2, "unexpected attempt count for {status}");
            server.await.expect("fixture server must stop cleanly");
        }
    }

    #[tokio::test]
    async fn issue_list_retry_does_not_repeat_permanent_http_client_errors() {
        for status in [400, 401, 403, 404] {
            let (url, server) = spawn_list_http_responses(vec![status]).await;
            let adapter = ListRetryAdapter::new(usize::MAX, ListFailureKind::HttpServer(url));

            let error = fetch_issue_list_with_retry(&adapter)
                .await
                .expect_err("permanent HTTP client errors must fail immediately");

            assert!(matches!(
                error,
                AdapterError::Network(error)
                    if error.status().is_some_and(|actual| actual.as_u16() == status)
            ));
            assert_eq!(adapter.calls(), 1, "unexpected attempt count for {status}");
            server.await.expect("fixture server must stop cleanly");
        }
    }

    #[tokio::test]
    async fn issue_list_retry_does_not_repeat_non_transient_errors() {
        let adapter = ListRetryAdapter::new(usize::MAX, ListFailureKind::NonTransient);

        let error = fetch_issue_list_with_retry(&adapter)
            .await
            .expect_err("a parse error must fail immediately");

        assert!(matches!(
            error,
            AdapterError::Parse(message) if message == "invalid issue list"
        ));
        assert_eq!(adapter.calls(), 1);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
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
        adapter_registry
            .register(Box::new(BlockingAdapter {
                entered_fetch: entered_fetch.clone(),
                release_fetch: release_fetch.clone(),
            }))
            .unwrap();
        let state = test_state(
            pool.clone(),
            adapter_registry,
            "/tmp/lilly-blocking-import-test",
        );

        let job = tokio::time::timeout(
            Duration::from_secs(1),
            start_import(state, "blocking-test", ImportTrigger::Manual { user_id }),
        )
        .await
        .expect("job start must not wait for the blocked adapter fetch")
        .expect("job start must succeed");
        tokio::time::timeout(Duration::from_secs(1), entered_fetch.notified())
            .await
            .expect("background worker must enter the adapter fetch");

        assert!(
            import_jobs::request_import_cancellation(&pool, job.id, user_id)
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
        assert_eq!(
            issues::count_issues_by_series(&pool, job.series_id)
                .await
                .unwrap(),
            0
        );

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

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn built_in_reference_records_round_trip_through_persistence_idempotently() {
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

        for descriptor in [MADDRAX_REFERENCE_DESCRIPTOR, JOHN_REFERENCE_DESCRIPTOR] {
            sqlx::query("DELETE FROM series WHERE source_key = ? AND source_record_id = ?")
                .bind(descriptor.source_key)
                .bind(descriptor.series_record_id)
                .execute(&pool)
                .await
                .expect("old reference persistence fixture must be removable");
        }

        let built_ins = lilly_importer_adapters::builtin_registry()
            .expect("built-in adapter registry must be constructible");
        let maddrax_references = built_ins
            .get("maddrax")
            .expect("Maddrax adapter must be registered")
            .reference_records();
        let john_references = built_ins
            .get("john-sinclair")
            .expect("John Sinclair adapter must be registered")
            .reference_records();
        assert_eq!(maddrax_references.len(), 3);
        assert_eq!(john_references.len(), 3);

        let expectations = vec![
            (
                "reference-maddrax",
                MADDRAX_REFERENCE_DESCRIPTOR,
                maddrax_references.clone(),
            ),
            (
                "reference-john-sinclair",
                JOHN_REFERENCE_DESCRIPTOR,
                john_references.clone(),
            ),
        ];
        let mut adapter_registry = AdapterRegistry::new();
        adapter_registry
            .register(Box::new(ReferenceSnapshotAdapter {
                name: "reference-maddrax",
                descriptor: MADDRAX_REFERENCE_DESCRIPTOR,
                references: maddrax_references,
            }))
            .unwrap();
        adapter_registry
            .register(Box::new(ReferenceSnapshotAdapter {
                name: "reference-john-sinclair",
                descriptor: JOHN_REFERENCE_DESCRIPTOR,
                references: john_references,
            }))
            .unwrap();

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role) VALUES (?, 'Reference Tester', 'admin')",
        )
        .bind(format!("reference-import-{suffix}@example.test"))
        .execute(&pool)
        .await
        .expect("user fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("user fixture ID must fit u32");
        let state = test_state(
            pool.clone(),
            adapter_registry,
            "/tmp/lilly-reference-persistence-test",
        );

        for (adapter_name, descriptor, references) in expectations {
            let first = start_import(
                state.clone(),
                adapter_name,
                ImportTrigger::Manual { user_id },
            )
            .await
            .expect("reference import must start");
            let first = wait_for_terminal_job(&pool, first.id).await;
            assert_eq!(first.status, "completed_with_errors");
            assert_eq!(first.created_issues, 3);
            assert_eq!(first.updated_issues, 0);
            assert_eq!(first.unchanged_issues, 0);
            assert_eq!(first.skipped_issues, 0);
            assert_eq!(first.failed_issues, 0);
            let cover_findings = import_jobs::find_import_errors(&pool, first.id, 1, 50)
                .await
                .expect("non-blocking cover findings must be queryable");
            assert_eq!(cover_findings.len(), 3);
            assert!(cover_findings.iter().all(|finding| {
                finding.stage == "cover"
                    && finding.severity == "warning"
                    && finding.code == "missing_at_source"
            }));

            let issue_rows = issues::find_all_issues_by_series(&pool, first.series_id)
                .await
                .expect("persisted reference issues must be queryable");
            let responses = issues::build_issue_responses(&pool, &issue_rows)
                .await
                .expect("reference issue relations must be queryable");
            assert_eq!(responses.len(), references.len());
            for reference in references {
                let response = responses
                    .iter()
                    .find(|issue| issue.issue_number == reference.issue_number)
                    .expect("every reference record must be persisted");
                let mut expected_authors = reference
                    .authors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                expected_authors.sort();
                assert_eq!(response.title, reference.title);
                assert_eq!(response.authors, expected_authors);
                assert_eq!(response.published_at, Some(reference.published_at));
                assert_eq!(response.source_key.as_deref(), Some(descriptor.source_key));
                assert_eq!(
                    response.source_record_id.as_deref(),
                    Some(format!("Issue:{}", reference.issue_number).as_str())
                );
                assert_eq!(
                    response.source_wiki_url.as_deref(),
                    Some(
                        format!("https://example.test/issues/{}", reference.issue_number).as_str()
                    )
                );
            }

            let second = start_import(
                state.clone(),
                adapter_name,
                ImportTrigger::Manual { user_id },
            )
            .await
            .expect("idempotence reference import must start");
            let second = wait_for_terminal_job(&pool, second.id).await;
            assert_eq!(second.status, "completed_with_errors");
            assert_eq!(second.created_issues, 0);
            assert_eq!(second.updated_issues, 0);
            assert_eq!(second.unchanged_issues, 3);
            assert_eq!(second.skipped_issues, 0);
            assert_eq!(second.failed_issues, 0);
            assert_eq!(
                issues::count_issues_by_series(&pool, second.series_id)
                    .await
                    .unwrap(),
                3
            );
        }

        for descriptor in [MADDRAX_REFERENCE_DESCRIPTOR, JOHN_REFERENCE_DESCRIPTOR] {
            sqlx::query("DELETE FROM series WHERE source_key = ? AND source_record_id = ?")
                .bind(descriptor.source_key)
                .bind(descriptor.series_record_id)
                .execute(&pool)
                .await
                .expect("reference persistence fixture must be deleted");
        }
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("user fixture must be deleted");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn full_sync_is_idempotent_updates_old_records_and_preserves_valid_data() {
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
        sqlx::query("DELETE FROM series WHERE slug = ?")
            .bind(SYNC_DESCRIPTOR.series_slug)
            .execute(&pool)
            .await
            .expect("old synchronization fixture must be removable");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role) VALUES (?, 'Sync Tester', 'admin')",
        )
        .bind(format!("sync-import-{suffix}@example.test"))
        .execute(&pool)
        .await
        .expect("user fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("user fixture ID must fit u32");

        let past = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let future = NaiveDate::from_ymd_opt(2999, 1, 1).unwrap();
        let mut source_issues: BTreeMap<u32, Result<IssueData, String>> = BTreeMap::new();
        source_issues.insert(
            1,
            Ok(sync_issue(
                1,
                "Original Title",
                vec!["Original Author".to_string()],
                past,
            )),
        );
        source_issues.insert(2, Ok(sync_issue(2, "Future Title", Vec::new(), future)));
        source_issues.insert(3, Ok(sync_issue(3, "Invalid Title", Vec::new(), past)));
        let scenario = Arc::new(RwLock::new(SyncScenario {
            issue_list_error: None,
            issues: source_issues,
            covers: BTreeMap::new(),
            cover_not_found: BTreeSet::new(),
        }));
        let cover_downloads = Arc::new(AtomicUsize::new(0));
        let mut adapter_registry = AdapterRegistry::new();
        adapter_registry
            .register(Box::new(SyncAdapter {
                scenario: scenario.clone(),
                cover_downloads: cover_downloads.clone(),
            }))
            .unwrap();
        let media_path = format!("/tmp/lilly-synchronization-import-test-{suffix}");
        let state = test_state(pool.clone(), adapter_registry, &media_path);

        let first = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("first synchronization must start");
        let first = wait_for_terminal_job(&pool, first.id).await;
        assert_eq!(first.status, "completed_with_errors");
        assert_eq!(first.total_issues, 3);
        assert_eq!(first.created_issues, 1);
        assert_eq!(first.updated_issues, 0);
        assert_eq!(first.unchanged_issues, 0);
        assert_eq!(first.skipped_issues, 1);
        assert_eq!(first.failed_issues, 1);
        assert_eq!(
            issues::count_issues_by_series(&pool, first.series_id)
                .await
                .unwrap(),
            1
        );
        let first_errors = import_jobs::find_import_errors(&pool, first.id, 1, 50)
            .await
            .unwrap();
        assert_eq!(first_errors.len(), 2);
        let validation_error = first_errors
            .iter()
            .find(|error| error.stage == "validate")
            .expect("validation finding must be persisted");
        assert_eq!(
            validation_error.source_record_id.as_deref(),
            Some("Issue:3")
        );
        assert_eq!(validation_error.severity, "blocking");
        let first_review = import_review::review_outcome_counts(&pool, first.id)
            .await
            .unwrap();
        assert_eq!(first_review.total, 3);
        assert_eq!(first_review.created, 1);
        assert_eq!(first_review.skipped, 1);
        assert_eq!(first_review.failed, 1);

        let original_issue_id: u32 =
            sqlx::query_scalar("SELECT id FROM issues WHERE series_id = ? AND issue_number = 1")
                .bind(first.series_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let collection_entry_id: u32 = sqlx::query(
            "INSERT INTO collection_entries \
             (user_id, issue_id, copy_number, condition_grade, status, notes) \
             VALUES (?, ?, 1, 'Z1', 'owned', 'must survive cover backfill')",
        )
        .bind(user_id)
        .bind(original_issue_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();

        series::set_series_active(&pool, first.series_id, true)
            .await
            .unwrap();
        {
            let mut scenario = scenario.write().expect("sync scenario lock poisoned");
            scenario.issues.insert(
                1,
                Ok(sync_issue(
                    1,
                    "Changed Old Title",
                    vec!["Original Author".to_string()],
                    past,
                )),
            );
            scenario.issues.insert(
                3,
                Ok(sync_issue(
                    3,
                    "Newly Valid Title",
                    vec!["New Author".to_string()],
                    past,
                )),
            );
        }

        let second = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("second synchronization must start");
        let second = wait_for_terminal_job(&pool, second.id).await;
        assert_eq!(second.status, "completed_with_errors");
        assert_eq!(second.created_issues, 1);
        assert_eq!(second.updated_issues, 1);
        assert_eq!(second.unchanged_issues, 0);
        assert_eq!(second.skipped_issues, 1);
        assert_eq!(second.failed_issues, 0);
        assert_eq!(
            issues::count_issues_by_series(&pool, second.series_id)
                .await
                .unwrap(),
            2
        );
        assert!(
            series::find_series_by_slug(&pool, SYNC_DESCRIPTOR.series_slug)
                .await
                .unwrap()
                .unwrap()
                .active,
            "series metadata refresh must preserve the active flag"
        );

        scenario
            .write()
            .expect("sync scenario lock poisoned")
            .covers
            .insert(
                1,
                Ok((
                    vec![0x89, b'P', b'N', b'G'],
                    "image/png".to_string(),
                    "1111111111111111111111111111111111111111".to_string(),
                )),
            );
        let third = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("idempotence synchronization must start");
        let third = wait_for_terminal_job(&pool, third.id).await;
        assert_eq!(third.status, "completed_with_errors");
        assert_eq!(third.created_issues, 0);
        assert_eq!(third.updated_issues, 0);
        assert_eq!(third.unchanged_issues, 2);
        assert_eq!(third.skipped_issues, 1);
        assert_eq!(third.failed_issues, 0);
        let recovered_cover: (Option<String>,) = sqlx::query_as(
            "SELECT cover_local_path FROM issues WHERE series_id = ? AND issue_number = 1",
        )
        .bind(third.series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            recovered_cover.0.as_deref(),
            Some(
                format!(
                    "/media/covers/series-{}/1-1111111111111111111111111111111111111111.png",
                    third.series_id
                )
                .as_str()
            )
        );
        assert_eq!(cover_downloads.load(Ordering::SeqCst), 1);
        let stored_identity: (u32, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT id, cover_source_file, cover_source_sha1 FROM issues \
             WHERE series_id = ? AND issue_number = 1",
        )
        .bind(third.series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_identity.0, original_issue_id);
        assert_eq!(stored_identity.1.as_deref(), Some("001tibi.png"));
        assert_eq!(
            stored_identity.2.as_deref(),
            Some("1111111111111111111111111111111111111111")
        );
        assert_eq!(
            sqlx::query_scalar::<_, u32>("SELECT issue_id FROM collection_entries WHERE id = ?")
                .bind(collection_entry_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            original_issue_id
        );
        assert_eq!(
            issues::count_issues_by_series(&pool, third.series_id)
                .await
                .unwrap(),
            2
        );

        let idempotent_cover = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("cover identity synchronization must start");
        let idempotent_cover = wait_for_terminal_job(&pool, idempotent_cover.id).await;
        assert_eq!(idempotent_cover.unchanged_issues, 2);
        assert_eq!(cover_downloads.load(Ordering::SeqCst), 1);

        scenario
            .write()
            .expect("sync scenario lock poisoned")
            .covers
            .insert(
                1,
                Ok((
                    vec![b'R', b'I', b'F', b'F'],
                    "image/webp".to_string(),
                    "2222222222222222222222222222222222222222".to_string(),
                )),
            );
        let changed_cover = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("changed cover synchronization must start");
        let changed_cover = wait_for_terminal_job(&pool, changed_cover.id).await;
        assert_eq!(changed_cover.unchanged_issues, 2);
        assert_eq!(cover_downloads.load(Ordering::SeqCst), 2);
        let old_cover_file = Path::new(&media_path)
            .join("covers")
            .join(format!("series-{}", changed_cover.series_id))
            .join("1-1111111111111111111111111111111111111111.png");
        let changed_cover_file = Path::new(&media_path)
            .join("covers")
            .join(format!("series-{}", changed_cover.series_id))
            .join("1-2222222222222222222222222222222222222222.webp");
        assert!(!tokio::fs::try_exists(old_cover_file).await.unwrap());
        assert!(tokio::fs::try_exists(&changed_cover_file).await.unwrap());

        scenario
            .write()
            .expect("sync scenario lock poisoned")
            .covers
            .insert(1, Err("temporary cover transport failure".to_string()));
        let failed_cover = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("cover failure synchronization must start");
        let failed_cover = wait_for_terminal_job(&pool, failed_cover.id).await;
        assert_eq!(failed_cover.unchanged_issues, 2);
        let retained_cover: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT cover_local_path, cover_source_sha1 FROM issues \
             WHERE series_id = ? AND issue_number = 1",
        )
        .bind(failed_cover.series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(retained_cover.0.as_deref().is_some_and(|path| {
            path.ends_with("/1-2222222222222222222222222222222222222222.webp")
        }));
        assert_eq!(
            retained_cover.1.as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
        assert!(tokio::fs::try_exists(&changed_cover_file).await.unwrap());

        scenario
            .write()
            .expect("sync scenario lock poisoned")
            .cover_not_found
            .insert(1);
        let not_found_cover = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("not-found cover failure synchronization must start");
        let not_found_cover = wait_for_terminal_job(&pool, not_found_cover.id).await;
        assert_eq!(not_found_cover.unchanged_issues, 2);
        let retained_after_not_found: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT cover_local_path, cover_source_sha1 FROM issues \
             WHERE series_id = ? AND issue_number = 1",
        )
        .bind(not_found_cover.series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(retained_after_not_found, retained_cover);
        assert!(tokio::fs::try_exists(&changed_cover_file).await.unwrap());
        let not_found_findings = import_jobs::find_import_errors(&pool, not_found_cover.id, 1, 50)
            .await
            .unwrap();
        assert!(not_found_findings.iter().any(|finding| {
            finding.stage == "cover"
                && finding.code == "fetch_failed"
                && finding.message.contains("issue index unavailable")
        }));

        {
            let mut scenario = scenario.write().expect("sync scenario lock poisoned");
            scenario.cover_not_found.remove(&1);
            scenario.covers.remove(&1);
        }
        let missing_cover = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("missing cover synchronization must start");
        let missing_cover = wait_for_terminal_job(&pool, missing_cover.id).await;
        assert_eq!(missing_cover.unchanged_issues, 2);
        let cleared_cover: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT cover_local_path, cover_source_sha1 FROM issues \
             WHERE series_id = ? AND issue_number = 1",
        )
        .bind(missing_cover.series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cleared_cover, (None, None));
        assert!(!tokio::fs::try_exists(&changed_cover_file).await.unwrap());
        assert_eq!(
            sqlx::query_scalar::<_, u32>("SELECT issue_id FROM collection_entries WHERE id = ?")
                .bind(collection_entry_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            original_issue_id
        );

        {
            let mut scenario = scenario.write().expect("sync scenario lock poisoned");
            scenario
                .issues
                .insert(1, Ok(sync_issue(1, "Must Not Be Stored", Vec::new(), past)));
        }
        let fourth = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("failure-safety synchronization must start");
        let fourth = wait_for_terminal_job(&pool, fourth.id).await;
        assert_eq!(fourth.status, "completed_with_errors");
        assert_eq!(fourth.unchanged_issues, 1);
        assert_eq!(fourth.skipped_issues, 1);
        assert_eq!(fourth.failed_issues, 1);
        let stored_title: (String,) =
            sqlx::query_as("SELECT title FROM issues WHERE series_id = ? AND issue_number = 1")
                .bind(fourth.series_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored_title.0, "Changed Old Title");

        scenario
            .write()
            .expect("sync scenario lock poisoned")
            .issue_list_error = Some("source unavailable".to_string());
        let failed = start_import(
            state.clone(),
            "sync-test",
            ImportTrigger::Manual { user_id },
        )
        .await
        .expect("fatal synchronization must start");
        let failed = wait_for_terminal_job(&pool, failed.id).await;
        assert_eq!(failed.status, "failed");
        assert_eq!(
            issues::count_issues_by_series(&pool, failed.series_id)
                .await
                .unwrap(),
            2
        );
        let failed_errors = import_jobs::find_import_errors(&pool, failed.id, 1, 50)
            .await
            .unwrap();
        assert_eq!(failed_errors.len(), 1);
        assert_eq!(failed_errors[0].stage, "list");
        assert_eq!(failed_errors[0].source_key, SYNC_DESCRIPTOR.source_key);

        scenario
            .write()
            .expect("sync scenario lock poisoned")
            .issue_list_error = None;
        let retry = retry_import(state, failed.id, user_id)
            .await
            .expect("failed synchronization must be retryable");
        let retry = wait_for_terminal_job(&pool, retry.id).await;
        assert_eq!(retry.retry_of_job_id, Some(failed.id));
        assert_eq!(retry.status, "completed_with_errors");
        assert_eq!(
            issues::count_issues_by_series(&pool, retry.series_id)
                .await
                .unwrap(),
            2
        );

        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(first.series_id)
            .execute(&pool)
            .await
            .expect("series fixture must be deleted");
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("user fixture must be deleted");
        let _ = tokio::fs::remove_dir_all(media_path).await;
    }
}
