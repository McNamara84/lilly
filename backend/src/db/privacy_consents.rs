use chrono::NaiveDateTime;
use sqlx::{MySql, MySqlPool, Transaction};

use crate::models::oauth::PrivacyConsentResponse;

pub async fn insert_on_transaction(
    transaction: &mut Transaction<'_, MySql>,
    user_id: u32,
    policy_version: &str,
    consented_at: NaiveDateTime,
    registration_method: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO privacy_consents \
         (user_id, policy_version, consented_at, registration_method) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(policy_version)
    .bind(consented_at)
    .bind(registration_method)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn find_for_user(
    pool: &MySqlPool,
    user_id: u32,
) -> Result<Vec<PrivacyConsentResponse>, sqlx::Error> {
    sqlx::query_as::<_, PrivacyConsentResponse>(
        "SELECT policy_version, consented_at, registration_method \
         FROM privacy_consents WHERE user_id = ? \
         ORDER BY consented_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
