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
