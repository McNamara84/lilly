use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

pub const ACCOUNT_DELETION_CONFIRMATION: &str = "KONTO LÖSCHEN";
pub const ACCOUNT_DELETION_GRACE_DAYS: i64 = 7;
pub const RECENT_AUTH_SECONDS: i64 = 10 * 60;

#[derive(Debug, Deserialize)]
pub struct RequestAccountDeletion {
    pub confirmation: String,
}

impl RequestAccountDeletion {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.confirmation == ACCOUNT_DELETION_CONFIRMATION {
            Ok(())
        } else {
            Err("Type KONTO LÖSCHEN exactly to confirm account deletion")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PasswordReauthenticationRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AccountDeletionStatusResponse {
    pub status: String,
    pub requested_at: NaiveDateTime,
    pub scheduled_for: NaiveDateTime,
    pub can_cancel: bool,
}

#[derive(Debug, Clone, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct AccountDeletionOptionsResponse {
    pub recent_authentication: bool,
    pub password: bool,
    pub google: bool,
    pub github: bool,
    pub confirmation_phrase: &'static str,
    pub grace_days: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct AccountErasureJobRow {
    pub id: u64,
    pub user_id: Option<u32>,
    pub status: String,
    pub previous_profile_public: bool,
    pub previous_collection_public: bool,
    pub requested_at: NaiveDateTime,
    pub scheduled_for: NaiveDateTime,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub ledger_recorded_at: Option<NaiveDateTime>,
    pub attempts: u32,
    pub next_retry_at: Option<NaiveDateTime>,
    pub last_error_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AdminAccountErasureJobResponse {
    pub id: u64,
    pub status: String,
    pub requested_at: NaiveDateTime,
    pub scheduled_for: NaiveDateTime,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub attempts: u32,
    pub next_retry_at: Option<NaiveDateTime>,
    pub last_error_category: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_confirmation_is_exact() {
        assert!(
            RequestAccountDeletion {
                confirmation: ACCOUNT_DELETION_CONFIRMATION.to_string()
            }
            .validate()
            .is_ok()
        );
        for invalid in ["konto löschen", " KONTO LÖSCHEN", "KONTO LOESCHEN", ""] {
            assert!(
                RequestAccountDeletion {
                    confirmation: invalid.to_string()
                }
                .validate()
                .is_err()
            );
        }
    }
}
