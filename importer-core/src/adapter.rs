use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;

use crate::types::{CoverData, IssueData, SeriesData};

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
    pub fn list(&self) -> Vec<(&str, &str, &str)> {
        self.adapters
            .values()
            .map(|a| (a.name(), a.display_name(), a.version()))
            .collect()
    }
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
        let (name, display, version) = list[0];
        assert_eq!(name, "mock");
        assert_eq!(display, "Mock Adapter");
        assert_eq!(version, "1.0");
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
}
