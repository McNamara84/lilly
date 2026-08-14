pub mod adapter;
pub mod contract;
pub mod progress;
pub mod types;

pub use adapter::{
    AdapterError, AdapterRegistry, ReferenceRecord, SourceDescriptor, WikiAdapter,
    normalize_and_validate_issue, normalize_and_validate_series, validate_reference_record,
    validate_source_reference,
};
pub use contract::verify_adapter_contract;
pub use progress::{LogProgressReporter, ProgressReporter};
pub use types::{
    CoverData, CoverFetchResult, CoverIdentity, IssueData, SeriesData, SeriesStatus,
    SourceReference,
};
