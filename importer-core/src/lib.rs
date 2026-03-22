pub mod adapter;
pub mod adapters;
pub mod progress;
pub mod types;

pub use adapter::{AdapterError, AdapterRegistry, WikiAdapter};
pub use progress::{LogProgressReporter, ProgressReporter};
pub use types::{CoverData, IssueData, SeriesData, SeriesStatus};
