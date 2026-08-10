//! Reusable conformance checks for source adapters.

use std::collections::BTreeSet;

use crate::{
    AdapterError, WikiAdapter, normalize_and_validate_issue, normalize_and_validate_series,
    validate_reference_record,
};

/// Exercise the source-independent success and idempotency contract.
///
/// Adapter crates should call this function with deterministic fixture-backed
/// transports. Live sources are deliberately unsuitable for repeatable tests.
pub async fn verify_adapter_contract(adapter: &dyn WikiAdapter) -> Result<(), AdapterError> {
    let descriptor = adapter.source_descriptor();
    let first_series =
        normalize_and_validate_series(descriptor, adapter.fetch_series_metadata().await?)?;
    let second_series =
        normalize_and_validate_series(descriptor, adapter.fetch_series_metadata().await?)?;
    if first_series != second_series {
        return Err(contract_error("series metadata is not idempotent"));
    }

    let first_numbers = validate_issue_numbers(adapter.fetch_issue_list().await?)?;
    let second_numbers = validate_issue_numbers(adapter.fetch_issue_list().await?)?;
    if first_numbers != second_numbers {
        return Err(contract_error("issue discovery is not idempotent"));
    }

    let references = adapter.reference_records();
    for reference in &references {
        if !first_numbers.contains(&reference.issue_number) {
            return Err(contract_error(&format!(
                "reference issue {} is absent from discovery",
                reference.issue_number
            )));
        }

        let first_issue = normalize_and_validate_issue(
            descriptor,
            reference.issue_number,
            adapter.fetch_issue_details(reference.issue_number).await?,
        )?;
        validate_reference_record(&references, &first_issue)?;
        let second_issue = normalize_and_validate_issue(
            descriptor,
            reference.issue_number,
            adapter.fetch_issue_details(reference.issue_number).await?,
        )?;
        validate_reference_record(&references, &second_issue)?;
        if first_issue != second_issue {
            return Err(contract_error(&format!(
                "issue {} is not idempotent",
                reference.issue_number
            )));
        }

        let first_cover = adapter.fetch_cover(reference.issue_number).await?;
        let second_cover = adapter.fetch_cover(reference.issue_number).await?;
        if first_cover != second_cover {
            return Err(contract_error(&format!(
                "cover {} is not idempotent",
                reference.issue_number
            )));
        }
    }

    Ok(())
}

fn validate_issue_numbers(numbers: Vec<u32>) -> Result<BTreeSet<u32>, AdapterError> {
    if numbers.contains(&0) {
        return Err(contract_error("issue discovery returned zero"));
    }
    let count = numbers.len();
    let unique = numbers.into_iter().collect::<BTreeSet<_>>();
    if unique.len() != count {
        return Err(contract_error("issue discovery returned duplicates"));
    }
    Ok(unique)
}

fn contract_error(message: &str) -> AdapterError {
    AdapterError::Other(format!("Adapter contract violation: {message}"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::{
        CoverData, IssueData, ReferenceRecord, SeriesData, SeriesStatus, SourceDescriptor,
        SourceReference,
    };

    const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
        source_key: "contract-test",
        display_name: "Contract Test",
        allowed_host: "example.test",
        series_name: "Contract Series",
        series_slug: "contract-series",
        series_record_id: "Series:Contract",
        series_url: "https://example.test/series/contract",
    };

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum FailureMode {
        ChangingSeriesMetadata,
        ChangingIssueDiscovery,
        ChangingIssueDetails,
        ChangingCover,
        ZeroIssueNumber,
        DuplicateIssueNumber,
        MissingReferenceIssue,
    }

    struct InvalidAdapter {
        mode: FailureMode,
        series_calls: AtomicUsize,
        discovery_calls: AtomicUsize,
        detail_calls: AtomicUsize,
        cover_calls: AtomicUsize,
    }

    impl InvalidAdapter {
        const fn new(mode: FailureMode) -> Self {
            Self {
                mode,
                series_calls: AtomicUsize::new(0),
                discovery_calls: AtomicUsize::new(0),
                detail_calls: AtomicUsize::new(0),
                cover_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl WikiAdapter for InvalidAdapter {
        fn name(&self) -> &'static str {
            "invalid-contract"
        }

        fn display_name(&self) -> &'static str {
            "Invalid Contract Adapter"
        }

        fn version(&self) -> &'static str {
            "test"
        }

        fn source_descriptor(&self) -> SourceDescriptor {
            DESCRIPTOR
        }

        fn reference_records(&self) -> Vec<ReferenceRecord> {
            vec![ReferenceRecord {
                issue_number: 1,
                title: "Contract issue",
                authors: &["Contract Author"],
                published_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            }]
        }

        async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
            let call = self.series_calls.fetch_add(1, Ordering::SeqCst);
            Ok(SeriesData {
                name: DESCRIPTOR.series_name.to_string(),
                slug: DESCRIPTOR.series_slug.to_string(),
                publisher: (self.mode == FailureMode::ChangingSeriesMetadata && call > 0)
                    .then(|| "Changed publisher".to_string()),
                genre: None,
                frequency: None,
                total_issues: Some(1),
                status: SeriesStatus::Running,
                source: SourceReference {
                    source_key: DESCRIPTOR.source_key.to_string(),
                    source_record_id: DESCRIPTOR.series_record_id.to_string(),
                    source_url: DESCRIPTOR.series_url.to_string(),
                },
            })
        }

        async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
            let call = self.discovery_calls.fetch_add(1, Ordering::SeqCst);
            Ok(match self.mode {
                FailureMode::ZeroIssueNumber => vec![0, 1],
                FailureMode::DuplicateIssueNumber => vec![1, 1],
                FailureMode::MissingReferenceIssue => vec![2],
                FailureMode::ChangingIssueDiscovery if call > 0 => vec![1, 2],
                _ => vec![1],
            })
        }

        async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
            let call = self.detail_calls.fetch_add(1, Ordering::SeqCst);
            Ok(IssueData {
                issue_number,
                title: "Contract issue".to_string(),
                authors: vec!["Contract Author".to_string()],
                published_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 1),
                part_number: None,
                part_total: None,
                cycle: None,
                cover_artists: Vec::new(),
                keywords: if self.mode == FailureMode::ChangingIssueDetails && call > 0 {
                    vec!["changed".to_string()]
                } else {
                    Vec::new()
                },
                notes: Vec::new(),
                source: SourceReference {
                    source_key: DESCRIPTOR.source_key.to_string(),
                    source_record_id: format!("Issue:{issue_number}"),
                    source_url: format!("https://example.test/issues/{issue_number}"),
                },
            })
        }

        async fn fetch_cover(&self, _issue_number: u32) -> Result<Option<CoverData>, AdapterError> {
            let call = self.cover_calls.fetch_add(1, Ordering::SeqCst);
            if self.mode == FailureMode::ChangingCover && call > 0 {
                Ok(Some(CoverData {
                    bytes: vec![1, 2, 3],
                    content_type: "image/png".to_string(),
                }))
            } else {
                Ok(None)
            }
        }
    }

    async fn assert_contract_rejected(mode: FailureMode, expected: &str) {
        let error = verify_adapter_contract(&InvalidAdapter::new(mode))
            .await
            .unwrap_err();
        assert!(
            matches!(error, AdapterError::Other(_)),
            "expected a contract error, got {error}"
        );
        assert!(
            error.to_string().contains(expected),
            "expected '{expected}' in '{error}'"
        );
    }

    #[tokio::test]
    async fn rejects_changing_series_metadata() {
        assert_contract_rejected(
            FailureMode::ChangingSeriesMetadata,
            "series metadata is not idempotent",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_changing_issue_discovery() {
        assert_contract_rejected(
            FailureMode::ChangingIssueDiscovery,
            "issue discovery is not idempotent",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_changing_issue_details() {
        assert_contract_rejected(
            FailureMode::ChangingIssueDetails,
            "issue 1 is not idempotent",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_changing_covers() {
        assert_contract_rejected(FailureMode::ChangingCover, "cover 1 is not idempotent").await;
    }

    #[tokio::test]
    async fn rejects_zero_issue_numbers() {
        assert_contract_rejected(
            FailureMode::ZeroIssueNumber,
            "issue discovery returned zero",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_duplicate_issue_numbers() {
        assert_contract_rejected(
            FailureMode::DuplicateIssueNumber,
            "issue discovery returned duplicates",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_missing_reference_issues() {
        assert_contract_rejected(
            FailureMode::MissingReferenceIssue,
            "reference issue 1 is absent from discovery",
        )
        .await;
    }
}
