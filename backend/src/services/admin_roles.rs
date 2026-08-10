use sqlx::MySqlPool;
use thiserror::Error;

use crate::models::user::normalize_email;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleChangeMethod {
    AdminEmailBootstrap,
    Cli,
}

impl RoleChangeMethod {
    const fn as_database_value(self) -> &'static str {
        match self {
            Self::AdminEmailBootstrap => "admin_email_bootstrap",
            Self::Cli => "cli",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionResult {
    Promoted { user_id: u32 },
    AlreadyAdmin { user_id: u32 },
    UserNotFound,
}

#[derive(Debug, Error)]
pub enum AdminRoleError {
    #[error("{0}")]
    InvalidEmail(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Promote exactly one existing account and audit a real role transition atomically.
pub async fn promote_user_to_admin(
    pool: &MySqlPool,
    email: &str,
    method: RoleChangeMethod,
) -> Result<PromotionResult, AdminRoleError> {
    let email = normalize_email(email).map_err(AdminRoleError::InvalidEmail)?;
    let mut transaction = pool.begin().await?;
    let user: Option<(u32, String)> =
        sqlx::query_as("SELECT id, role FROM users WHERE email = ? FOR UPDATE")
            .bind(&email)
            .fetch_optional(&mut *transaction)
            .await?;
    let Some((user_id, previous_role)) = user else {
        transaction.rollback().await?;
        return Ok(PromotionResult::UserNotFound);
    };
    if previous_role == "admin" {
        transaction.rollback().await?;
        return Ok(PromotionResult::AlreadyAdmin { user_id });
    }

    sqlx::query("UPDATE users SET role = 'admin' WHERE id = ? AND role = 'user'")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "INSERT INTO role_change_events \
         (target_user_id, previous_role, new_role, method) VALUES (?, ?, 'admin', ?)",
    )
    .bind(user_id)
    .bind(&previous_role)
    .bind(method.as_database_value())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(PromotionResult::Promoted { user_id })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use sqlx::mysql::MySqlPoolOptions;

    use super::*;

    #[test]
    fn role_change_methods_have_stable_database_values() {
        assert_eq!(
            RoleChangeMethod::AdminEmailBootstrap.as_database_value(),
            "admin_email_bootstrap"
        );
        assert_eq!(RoleChangeMethod::Cli.as_database_value(), "cli");
    }

    #[tokio::test]
    async fn promotion_is_normalized_atomic_idempotent_and_audited() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _database_guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        crate::db::migrate_test_database(&pool)
            .await
            .expect("test migrations must succeed");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let email = format!("role-{suffix}@example.test");
        let user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role) VALUES (?, 'Role Tester', 'user')",
        )
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();

        assert_eq!(
            promote_user_to_admin(
                &pool,
                &format!("  {}  ", email.to_uppercase()),
                RoleChangeMethod::AdminEmailBootstrap,
            )
            .await
            .unwrap(),
            PromotionResult::Promoted { user_id }
        );
        let role: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(role.0, "admin");
        assert_eq!(
            promote_user_to_admin(&pool, &email, RoleChangeMethod::Cli)
                .await
                .unwrap(),
            PromotionResult::AlreadyAdmin { user_id }
        );
        let audit_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM role_change_events WHERE target_user_id = ?")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(audit_count.0, 1);
        let audit_event: (u32, String) =
            sqlx::query_as("SELECT id, method FROM role_change_events WHERE target_user_id = ?")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(audit_event.1, "admin_email_bootstrap");
        assert_eq!(
            promote_user_to_admin(
                &pool,
                "unknown-role-user@example.test",
                RoleChangeMethod::Cli,
            )
            .await
            .unwrap(),
            PromotionResult::UserNotFound
        );
        assert!(matches!(
            promote_user_to_admin(&pool, "invalid", RoleChangeMethod::Cli).await,
            Err(AdminRoleError::InvalidEmail(_))
        ));

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        let retained_target: (Option<u32>,) =
            sqlx::query_as("SELECT target_user_id FROM role_change_events WHERE id = ?")
                .bind(audit_event.0)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(retained_target.0, None);
        sqlx::query("DELETE FROM role_change_events WHERE id = ?")
            .bind(audit_event.0)
            .execute(&pool)
            .await
            .unwrap();
    }
}
