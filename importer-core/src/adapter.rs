use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;

use crate::types::{CoverData, IssueData, SeriesData, SourceReference};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceDescriptor {
    pub source_key: &'static str,
    pub display_name: &'static str,
    pub allowed_host: &'static str,
    pub series_name: &'static str,
    pub series_slug: &'static str,
    pub series_record_id: &'static str,
    pub series_url: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRecord {
    pub issue_number: u32,
    pub title: &'static str,
    pub authors: &'static [&'static str],
    pub published_at: chrono::NaiveDate,
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait WikiAdapter: Send + Sync {
    /// Unique identifier for this adapter (e.g., "maddrax")
    fn name(&self) -> &str;

    /// Human-readable display name (e.g., "Maddrax – Die dunkle Zukunft der Erde")
    fn display_name(&self) -> &str;

    /// Version of this adapter
    fn version(&self) -> &str;

    /// Stable source and target-series identity used before any network access.
    fn source_descriptor(&self) -> SourceDescriptor;

    /// Stable records that must match before an import can be published.
    fn reference_records(&self) -> Vec<ReferenceRecord> {
        Vec::new()
    }

    /// Fetch series metadata (name, publisher, genre, etc.)
    async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError>;

    /// Fetch the list of all available issue numbers
    async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError>;

    /// Fetch details for a single issue
    async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError>;

    /// Download cover image, returns raw bytes + content type
    async fn fetch_cover(&self, issue_number: u32) -> Result<Option<CoverData>, AdapterError>;
}

pub struct AdapterRegistry {
    adapters: HashMap<String, Box<dyn WikiAdapter>>,
}

impl AdapterRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    pub fn register(&mut self, adapter: Box<dyn WikiAdapter>) {
        let name = adapter.name().to_string();
        self.adapters.insert(name, adapter);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn WikiAdapter> {
        self.adapters.get(name).map(AsRef::as_ref)
    }

    /// Returns a list of (name, `display_name`, version) for all registered adapters
    #[must_use]
    pub fn list(&self) -> Vec<(&str, &str, &str, SourceDescriptor)> {
        self.adapters
            .values()
            .map(|a| {
                (
                    a.name(),
                    a.display_name(),
                    a.version(),
                    a.source_descriptor(),
                )
            })
            .collect()
    }
}

/// Validate that a returned record belongs to the adapter's authoritative source.
pub fn validate_source_reference(
    descriptor: SourceDescriptor,
    source: &SourceReference,
) -> Result<(), AdapterError> {
    if source.source_key.trim() != descriptor.source_key {
        return Err(AdapterError::Parse(format!(
            "Source key '{}' does not match adapter source '{}'",
            source.source_key, descriptor.source_key
        )));
    }
    if source.source_record_id.trim().is_empty() {
        return Err(AdapterError::Parse(
            "Source record identifier must not be empty".to_string(),
        ));
    }
    let url = reqwest::Url::parse(source.source_url.trim())
        .map_err(|error| AdapterError::Parse(format!("Invalid source URL: {error}")))?;
    if url.scheme() != "https" {
        return Err(AdapterError::Parse("Source URL must use HTTPS".to_string()));
    }
    if url.host_str() != Some(descriptor.allowed_host) {
        return Err(AdapterError::Parse(format!(
            "Source host '{}' is not allowed for '{}'",
            url.host_str().unwrap_or_default(),
            descriptor.source_key
        )));
    }
    Ok(())
}

pub fn normalize_and_validate_series(
    descriptor: SourceDescriptor,
    mut series: SeriesData,
) -> Result<SeriesData, AdapterError> {
    series.name = series.name.trim().to_string();
    series.slug = series.slug.trim().to_string();
    series.source.source_key = series.source.source_key.trim().to_string();
    series.source.source_record_id = series.source.source_record_id.trim().to_string();
    series.source.source_url = series.source.source_url.trim().to_string();

    if series.name.is_empty() || series.slug.is_empty() {
        return Err(AdapterError::Parse(
            "Series name and slug are mandatory".to_string(),
        ));
    }
    if series.slug != descriptor.series_slug {
        return Err(AdapterError::Parse(format!(
            "Series slug '{}' does not match adapter target '{}'",
            series.slug, descriptor.series_slug
        )));
    }
    if series.source.source_record_id != descriptor.series_record_id {
        return Err(AdapterError::Parse(format!(
            "Series source identifier '{}' does not match adapter target '{}'",
            series.source.source_record_id, descriptor.series_record_id
        )));
    }
    validate_source_reference(descriptor, &series.source)?;
    Ok(series)
}

pub fn normalize_and_validate_issue(
    descriptor: SourceDescriptor,
    expected_issue_number: u32,
    mut issue: IssueData,
) -> Result<IssueData, AdapterError> {
    if issue.issue_number == 0 || issue.issue_number != expected_issue_number {
        return Err(AdapterError::Parse(format!(
            "Returned issue number {} does not match requested issue {expected_issue_number}",
            issue.issue_number
        )));
    }

    issue.title = issue.title.trim().to_string();
    normalize_values(&mut issue.authors);
    normalize_values(&mut issue.cover_artists);
    normalize_values(&mut issue.keywords);
    normalize_values(&mut issue.notes);
    issue.source.source_key = issue.source.source_key.trim().to_string();
    issue.source.source_record_id = issue.source.source_record_id.trim().to_string();
    issue.source.source_url = issue.source.source_url.trim().to_string();

    if issue.title.is_empty() {
        return Err(AdapterError::Parse(format!(
            "Issue {expected_issue_number} has no title"
        )));
    }
    if issue.authors.is_empty() {
        return Err(AdapterError::Parse(format!(
            "Issue {expected_issue_number} has no author"
        )));
    }
    if issue.published_at.is_none() {
        return Err(AdapterError::Parse(format!(
            "Issue {expected_issue_number} has no first publication date"
        )));
    }
    let valid_multipart = match (issue.part_number, issue.part_total) {
        (None, None) => true,
        (Some(number), Some(total)) => number > 0 && number <= total,
        _ => false,
    };
    if !valid_multipart {
        return Err(AdapterError::Parse(format!(
            "Issue {expected_issue_number} has an invalid multipart position"
        )));
    }
    validate_source_reference(descriptor, &issue.source)?;
    Ok(issue)
}

/// Validate a pinned reference issue when the adapter declares one for this number.
pub fn validate_reference_record(
    references: &[ReferenceRecord],
    issue: &IssueData,
) -> Result<(), AdapterError> {
    let Some(reference) = references
        .iter()
        .find(|reference| reference.issue_number == issue.issue_number)
    else {
        return Ok(());
    };
    let expected_authors: Vec<String> = reference
        .authors
        .iter()
        .map(|author| (*author).to_string())
        .collect();
    if issue.title == reference.title
        && issue.authors == expected_authors
        && issue.published_at == Some(reference.published_at)
    {
        return Ok(());
    }
    Err(AdapterError::Parse(format!(
        "Reference issue {} differs from the pinned title, author or publication date",
        issue.issue_number
    )))
}

fn normalize_values(values: &mut Vec<String>) {
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAdapter;

    #[async_trait]
    impl WikiAdapter for MockAdapter {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn display_name(&self) -> &'static str {
            "Mock Adapter"
        }
        fn version(&self) -> &'static str {
            "1.0"
        }
        fn source_descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_key: "mock-wiki",
                display_name: "Mock Wiki",
                allowed_host: "example.test",
                series_name: "Mock Series",
                series_slug: "mock",
                series_record_id: "Mock_Series",
                series_url: "https://example.test/wiki/Mock_Series",
            }
        }
        async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
            Err(AdapterError::Other("not implemented".to_string()))
        }
        async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
            Ok(vec![1, 2, 3])
        }
        async fn fetch_issue_details(&self, _issue_number: u32) -> Result<IssueData, AdapterError> {
            Err(AdapterError::Other("not implemented".to_string()))
        }
        async fn fetch_cover(&self, _issue_number: u32) -> Result<Option<CoverData>, AdapterError> {
            Ok(None)
        }
    }

    #[test]
    fn test_registry_new_is_empty() {
        let registry = AdapterRegistry::new();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter));

        let adapter = registry.get("mock");
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().name(), "mock");
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = AdapterRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_list() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter));

        let list = registry.list();
        assert_eq!(list.len(), 1);
        let (name, display, version, source) = list[0];
        assert_eq!(name, "mock");
        assert_eq!(display, "Mock Adapter");
        assert_eq!(version, "1.0");
        assert_eq!(source.source_key, "mock-wiki");
    }

    #[test]
    fn test_adapter_error_display() {
        let err = AdapterError::Parse("bad html".to_string());
        assert_eq!(err.to_string(), "Parse error: bad html");
    }

    #[test]
    fn test_registry_default() {
        let registry = AdapterRegistry::default();
        assert!(registry.list().is_empty());
    }

    #[tokio::test]
    async fn test_mock_adapter_fetch_issue_list() {
        let adapter = MockAdapter;
        let issues = adapter.fetch_issue_list().await.unwrap();
        assert_eq!(issues, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_mock_adapter_fetch_cover_returns_none() {
        let adapter = MockAdapter;
        let cover = adapter.fetch_cover(1).await.unwrap();
        assert!(cover.is_none());
    }

    fn valid_issue() -> IssueData {
        IssueData {
            issue_number: 1,
            title: "  Test title ".to_string(),
            authors: vec![" Author ".to_string(), "Author".to_string()],
            published_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 1),
            part_number: None,
            part_total: None,
            cycle: None,
            cover_artists: Vec::new(),
            keywords: vec![" Beta ".to_string(), "Alpha".to_string()],
            notes: Vec::new(),
            source: SourceReference {
                source_key: "mock-wiki".to_string(),
                source_record_id: "Issue_1".to_string(),
                source_url: "https://example.test/wiki/Issue_1".to_string(),
            },
        }
    }

    #[test]
    fn normalization_is_deterministic_and_validates_mandatory_fields() {
        let descriptor = MockAdapter.source_descriptor();
        let issue = normalize_and_validate_issue(descriptor, 1, valid_issue()).unwrap();
        assert_eq!(issue.title, "Test title");
        assert_eq!(issue.authors, vec!["Author"]);
        assert_eq!(issue.keywords, vec!["Alpha", "Beta"]);

        let mut missing_author = valid_issue();
        missing_author.authors.clear();
        assert!(normalize_and_validate_issue(descriptor, 1, missing_author).is_err());

        let mut missing_date = valid_issue();
        missing_date.published_at = None;
        assert!(normalize_and_validate_issue(descriptor, 1, missing_date).is_err());
    }

    #[test]
    fn validation_rejects_wrong_source_and_host() {
        let descriptor = MockAdapter.source_descriptor();
        let mut wrong_source = valid_issue();
        wrong_source.source.source_key = "other".to_string();
        assert!(normalize_and_validate_issue(descriptor, 1, wrong_source).is_err());

        let mut wrong_host = valid_issue();
        wrong_host.source.source_url = "https://other.test/wiki/Issue_1".to_string();
        assert!(normalize_and_validate_issue(descriptor, 1, wrong_host).is_err());
    }

    #[test]
    fn reference_validation_accepts_exact_and_unpinned_records() {
        let reference = ReferenceRecord {
            issue_number: 1,
            title: "Test title",
            authors: &["Author"],
            published_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        };
        let issue = normalize_and_validate_issue(MockAdapter.source_descriptor(), 1, valid_issue())
            .unwrap();
        assert!(validate_reference_record(std::slice::from_ref(&reference), &issue).is_ok());

        let mut unpinned = issue;
        unpinned.issue_number = 2;
        assert!(validate_reference_record(&[reference], &unpinned).is_ok());
    }

    #[test]
    fn reference_validation_rejects_a_metadata_difference() {
        let reference = ReferenceRecord {
            issue_number: 1,
            title: "Different title",
            authors: &["Author"],
            published_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        };
        let issue = normalize_and_validate_issue(MockAdapter.source_descriptor(), 1, valid_issue())
            .unwrap();
        let error = validate_reference_record(&[reference], &issue).unwrap_err();
        assert!(error.to_string().contains("Reference issue 1 differs"));
    }
}
