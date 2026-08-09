pub mod adapter;
pub mod adapters;
mod cover_image;
pub mod progress;
pub mod types;

pub use adapter::{
    AdapterError, AdapterRegistry, SourceDescriptor, WikiAdapter, normalize_and_validate_issue,
    normalize_and_validate_series, validate_source_reference,
};
pub use progress::{LogProgressReporter, ProgressReporter};
pub use types::{CoverData, IssueData, SeriesData, SeriesStatus, SourceReference};
