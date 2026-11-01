//! Cleanup for database fault-injection tests, including panic and task cancellation.

use sqlx::{Connection, MySqlConnection, MySqlPool};

enum Parameter {
    Id(u32),
    Text(String),
}

type Statement = (&'static str, Vec<Parameter>);

pub(crate) struct DatabaseTestCleanup {
    pool: MySqlPool,
    statements: Vec<Statement>,
}

impl DatabaseTestCleanup {
    pub(crate) fn new(pool: MySqlPool) -> Self {
        // Drop may need to wait for a transaction rollback on the test's runtime.
        assert_eq!(
            tokio::runtime::Handle::current().runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread,
            "fault-injection cleanup requires a multi-thread test runtime"
        );
        Self {
            pool,
            statements: Vec::new(),
        }
    }

    /// Register immediately after acquiring a fixture (before installing a trigger).
    /// Statements run in reverse order; all identifiers in SQL must be static.
    pub(crate) fn defer(&mut self, query: &'static str, ids: &[u32]) {
        self.statements
            .push((query, ids.iter().copied().map(Parameter::Id).collect()));
    }

    pub(crate) fn defer_text(&mut self, query: &'static str, value: &str) {
        self.statements
            .push((query, vec![Parameter::Text(value.to_owned())]));
    }

    /// Normal cleanup is awaited and propagates failures to the test.
    pub(crate) async fn finish(mut self) -> Result<(), sqlx::Error> {
        let mut connection = self.pool.acquire().await?;
        execute_cleanup(&mut connection, &mut self.statements).await
    }
}

async fn execute_cleanup(
    connection: &mut MySqlConnection,
    statements: &mut Vec<Statement>,
) -> Result<(), sqlx::Error> {
    while let Some((sql, ids)) = statements.last() {
        let mut query = sqlx::query(*sql);
        for value in ids {
            query = match value {
                Parameter::Id(id) => query.bind(id),
                Parameter::Text(text) => query.bind(text),
            };
        }
        query.execute(&mut *connection).await?;
        statements.pop();
    }
    Ok(())
}

impl Drop for DatabaseTestCleanup {
    fn drop(&mut self) {
        if self.statements.is_empty() {
            return;
        }
        let options = self.pool.connect_options();
        let mut statements = std::mem::take(&mut self.statements);
        // A detached Tokio task could be lost when a panicking test shuts down its runtime.
        // Join an independent runtime before releasing the caller's database test mutex.
        // block_in_place lets pending SQLx rollbacks continue on the original runtime.
        let result = tokio::task::block_in_place(|| {
            std::thread::spawn(move || -> anyhow::Result<()> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(10), async {
                        let mut connection = MySqlConnection::connect_with(&options).await?;
                        execute_cleanup(&mut connection, &mut statements).await?;
                        connection.close().await
                    })
                    .await??;
                    Ok::<(), anyhow::Error>(())
                })?;
                Ok(())
            })
            .join()
        });
        // Preserve the original panic; normal cleanup failures are reported by finish().
        if !matches!(result, Ok(Ok(()))) {
            eprintln!("Database fault-injection cleanup failed: {result:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn check_cleanup(outcome: &'static str) {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let _lock = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let (ready, started) = tokio::sync::oneshot::channel();
        let task_pool = pool.clone();
        let task = tokio::spawn(async move {
            let mut cleanup = DatabaseTestCleanup::new(task_pool.clone());
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let user_id = u32::try_from(
                sqlx::query(
                    "INSERT INTO users (email, display_name) VALUES (?, 'Cleanup fixture')",
                )
                .bind(format!("cleanup-{suffix}@example.test"))
                .execute(&task_pool)
                .await
                .unwrap()
                .last_insert_id(),
            )
            .unwrap();
            cleanup.defer("DELETE FROM users WHERE id = ?", &[user_id]);
            cleanup.defer("DROP TRIGGER IF EXISTS lilly_test_cleanup_guard", &[]);
            sqlx::query("CREATE TRIGGER lilly_test_cleanup_guard BEFORE INSERT ON notifications FOR EACH ROW SET @lilly_cleanup_seen = 1")
                .execute(&task_pool).await.unwrap();
            // Cancellation must also drain the dropped transaction's queued rollback.
            let mut transaction = task_pool.begin().await.unwrap();
            sqlx::query("UPDATE users SET display_name = 'Locked cleanup fixture' WHERE id = ?")
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .unwrap();
            ready.send(user_id).unwrap();
            match outcome {
                "panic" => panic!("Intentional fixture panic"),
                "abort" => std::future::pending::<()>().await,
                _ => {
                    transaction.rollback().await.unwrap();
                    cleanup.finish().await.unwrap();
                }
            }
        });
        let user_id = started.await.unwrap();
        if outcome == "abort" {
            task.abort();
        }
        let result = task.await;
        match outcome {
            "panic" => assert!(result.unwrap_err().is_panic()),
            "abort" => assert!(result.unwrap_err().is_cancelled()),
            _ => result.unwrap(),
        }
        let triggers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() AND TRIGGER_NAME = 'lilly_test_cleanup_guard'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(triggers, 0);
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(users, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cleans_up_after_success() {
        check_cleanup("success").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cleans_up_after_panic() {
        check_cleanup("panic").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cleans_up_after_task_abort() {
        check_cleanup("abort").await;
    }
}
