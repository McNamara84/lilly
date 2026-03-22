use async_trait::async_trait;

#[async_trait]
pub trait ProgressReporter: Send + Sync {
    /// Report import progress (imported/total)
    async fn report_progress(&self, imported: u32, total: u32) -> Result<(), anyhow::Error>;

    /// Report a non-fatal error during import
    async fn report_error(&self, message: &str) -> Result<(), anyhow::Error>;

    /// Report that the import is complete
    async fn report_complete(&self) -> Result<(), anyhow::Error>;
}

/// A simple progress reporter that logs to tracing
pub struct LogProgressReporter;

#[async_trait]
impl ProgressReporter for LogProgressReporter {
    async fn report_progress(&self, imported: u32, total: u32) -> Result<(), anyhow::Error> {
        tracing::info!("Import progress: {imported}/{total}");
        Ok(())
    }

    async fn report_error(&self, message: &str) -> Result<(), anyhow::Error> {
        tracing::warn!("Import error: {message}");
        Ok(())
    }

    async fn report_complete(&self) -> Result<(), anyhow::Error> {
        tracing::info!("Import complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_reporter_progress() {
        let reporter = LogProgressReporter;
        assert!(reporter.report_progress(10, 100).await.is_ok());
    }

    #[tokio::test]
    async fn test_log_reporter_error() {
        let reporter = LogProgressReporter;
        assert!(reporter.report_error("test error").await.is_ok());
    }

    #[tokio::test]
    async fn test_log_reporter_complete() {
        let reporter = LogProgressReporter;
        assert!(reporter.report_complete().await.is_ok());
    }
}
