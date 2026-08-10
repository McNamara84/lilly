use async_trait::async_trait;
use chrono::NaiveDate;
use lilly_importer_core::adapter::{AdapterError, ReferenceRecord, SourceDescriptor, WikiAdapter};
use lilly_importer_core::types::{CoverData, IssueData, SeriesData, SeriesStatus, SourceReference};

const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    source_key: "e2e-fixture",
    display_name: "E2E Fixture",
    allowed_host: "example.test",
    series_name: "ZZZ E2E Fixture Series",
    series_slug: "e2e-fixture-series",
    series_record_id: "Series:E2E",
    series_url: "https://example.test/series/e2e",
};

pub struct E2eFixtureAdapter;

#[async_trait]
impl WikiAdapter for E2eFixtureAdapter {
    fn name(&self) -> &'static str {
        "e2e-fixture"
    }

    fn display_name(&self) -> &'static str {
        DESCRIPTOR.display_name
    }

    fn version(&self) -> &'static str {
        "1.0"
    }

    fn source_descriptor(&self) -> SourceDescriptor {
        DESCRIPTOR
    }

    fn reference_records(&self) -> Vec<ReferenceRecord> {
        vec![ReferenceRecord {
            issue_number: 1,
            title: "Deterministic E2E Issue",
            authors: &["LILLY Test Suite"],
            published_at: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        }]
    }

    async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
        Ok(SeriesData {
            name: DESCRIPTOR.series_name.to_string(),
            slug: DESCRIPTOR.series_slug.to_string(),
            publisher: Some("LILLY Tests".to_string()),
            genre: Some("Test Fixture".to_string()),
            frequency: None,
            total_issues: Some(1),
            status: SeriesStatus::Completed,
            source: SourceReference {
                source_key: DESCRIPTOR.source_key.to_string(),
                source_record_id: DESCRIPTOR.series_record_id.to_string(),
                source_url: DESCRIPTOR.series_url.to_string(),
            },
        })
    }

    async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
        Ok(vec![1])
    }

    async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
        if issue_number != 1 {
            return Err(AdapterError::NotFound(format!("issue {issue_number}")));
        }

        Ok(IssueData {
            issue_number,
            title: "Deterministic E2E Issue".to_string(),
            authors: vec!["LILLY Test Suite".to_string()],
            published_at: NaiveDate::from_ymd_opt(2026, 1, 1),
            part_number: None,
            part_total: None,
            cycle: None,
            cover_artists: Vec::new(),
            keywords: vec!["e2e".to_string()],
            notes: Vec::new(),
            source: SourceReference {
                source_key: DESCRIPTOR.source_key.to_string(),
                source_record_id: "Issue:E2E-1".to_string(),
                source_url: "https://example.test/issues/e2e-1".to_string(),
            },
        })
    }

    async fn fetch_cover(&self, _issue_number: u32) -> Result<Option<CoverData>, AdapterError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fixture_adapter_returns_deterministic_data() {
        let adapter = E2eFixtureAdapter;

        assert_eq!(adapter.name(), "e2e-fixture");
        assert_eq!(adapter.fetch_issue_list().await.unwrap(), vec![1]);
        let series = adapter.fetch_series_metadata().await.unwrap();
        assert_eq!(series.slug, "e2e-fixture-series");
        assert_eq!(series.name, "ZZZ E2E Fixture Series");
        assert_eq!(
            adapter.fetch_issue_details(1).await.unwrap().title,
            "Deterministic E2E Issue"
        );
        assert_eq!(adapter.reference_records().len(), 1);
        assert_eq!(adapter.reference_records()[0].issue_number, 1);
        assert!(adapter.fetch_cover(1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_fixture_adapter_rejects_unknown_issue() {
        assert!(matches!(
            E2eFixtureAdapter.fetch_issue_details(2).await,
            Err(AdapterError::NotFound(_))
        ));
    }
}
