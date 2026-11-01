use std::future::Future;

use crate::error::AppError;

pub(crate) fn is_retryable_transaction_error(error: &AppError) -> bool {
    let AppError::InternalError(error) = error else {
        return false;
    };
    let Some(sqlx::Error::Database(database_error)) = error.downcast_ref::<sqlx::Error>() else {
        return false;
    };
    // SQLx exposes SQLSTATE through code(); MariaDB's numeric codes are separate.
    database_error
        .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
        .is_some_and(|error| matches!(error.number(), 1020 | 1205 | 1213))
        || database_error.code().is_some_and(|code| code == "40001")
}

/// Each attempt must open a fresh transaction and roll it back on error.
/// Return immediately after commit; response reads and external effects belong outside the retry.
pub(crate) async fn retry_transaction<T, F, Fut>(
    operation: &'static str,
    mut run: F,
) -> Result<T, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    const MAX_ATTEMPTS: u32 = 5;
    for attempt in 1..=MAX_ATTEMPTS {
        match run().await {
            Err(error) if attempt < MAX_ATTEMPTS && is_retryable_transaction_error(&error) => {
                tracing::warn!(operation, attempt, "Transaction was rolled back; retrying");
                tokio::time::sleep(std::time::Duration::from_millis(
                    50 * 2_u64.pow(attempt - 1),
                ))
                .await;
            }
            result => return result,
        }
    }
    unreachable!("The final transaction attempt always returns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn application_and_connection_errors_are_not_retried() {
        for error in [
            AppError::Conflict("entry changed".into()),
            AppError::Forbidden {
                message: "not a participant".into(),
                code: None,
            },
            AppError::InternalError(anyhow::anyhow!("unexpected error")),
            AppError::from(sqlx::Error::PoolTimedOut),
        ] {
            assert!(!is_retryable_transaction_error(&error));
            let mut error = Some(error);
            let result = retry_transaction("test", || {
                std::future::ready(Err::<(), _>(
                    error
                        .take()
                        .expect("non-transient errors must not be retried"),
                ))
            })
            .await;
            assert!(result.is_err());
        }
        assert_eq!(
            retry_transaction("test", || async { Ok(42) })
                .await
                .unwrap(),
            42
        );
    }

    #[tokio::test]
    async fn mysql_codes_are_classified_and_only_transient_failures_are_retried() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::MySqlPool::connect(&url).await.unwrap();
        for (query, retryable) in [
            ("SIGNAL SQLSTATE 'HY000' SET MYSQL_ERRNO = 1020", true),
            ("SIGNAL SQLSTATE 'HY000' SET MYSQL_ERRNO = 1205", true),
            ("SIGNAL SQLSTATE '40001' SET MYSQL_ERRNO = 1213", true),
            ("SIGNAL SQLSTATE '40001' SET MYSQL_ERRNO = 1644", true),
            ("SIGNAL SQLSTATE '23000' SET MYSQL_ERRNO = 1062", false),
            ("SIGNAL SQLSTATE '42000' SET MYSQL_ERRNO = 1064", false),
        ] {
            let calls = std::sync::atomic::AtomicUsize::new(0);
            let result = retry_transaction("test", || async {
                if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    let error =
                        AppError::from(sqlx::query(query).execute(&pool).await.unwrap_err());
                    assert_eq!(is_retryable_transaction_error(&error), retryable);
                    return Err(error);
                }
                Ok(())
            })
            .await;
            assert_eq!(result.is_ok(), retryable);
            assert_eq!(
                calls.load(std::sync::atomic::Ordering::SeqCst),
                if retryable { 2 } else { 1 }
            );
        }
    }
}
