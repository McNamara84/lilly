use chrono::NaiveDateTime;
use sqlx::{MySqlPool, Row};

use crate::db::privacy_consents;
use crate::models::oauth::{OAuthFlowRow, OAuthIdentityProfile, PendingOAuthLinkRow};
use crate::models::user::User;

pub struct NewOAuthFlow<'a> {
    pub state_hash: &'a str,
    pub browser_binding_hash: &'a str,
    pub provider: &'a str,
    pub intent: &'a str,
    pub reauth_user_id: Option<u32>,
    pub pkce_verifier: &'a str,
    pub privacy_policy_version: Option<&'a str>,
    pub consented_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub expires_at: NaiveDateTime,
}

pub async fn insert_flow(pool: &MySqlPool, flow: &NewOAuthFlow<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO oauth_authorization_flows \
         (state_hash, browser_binding_hash, provider, intent, reauth_user_id, pkce_verifier, \
          privacy_policy_version, consented_at, created_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(flow.state_hash)
    .bind(flow.browser_binding_hash)
    .bind(flow.provider)
    .bind(flow.intent)
    .bind(flow.reauth_user_id)
    .bind(flow.pkce_verifier)
    .bind(flow.privacy_policy_version)
    .bind(flow.consented_at)
    .bind(flow.created_at)
    .bind(flow.expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn consume_flow(
    pool: &MySqlPool,
    state_hash: &str,
    browser_binding_hash: &str,
    provider: &str,
    now: NaiveDateTime,
) -> Result<Option<OAuthFlowRow>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let flow = sqlx::query_as::<_, OAuthFlowRow>(
        "SELECT browser_binding_hash, provider, intent, reauth_user_id, pkce_verifier, \
                privacy_policy_version, consented_at, expires_at, consumed_at \
         FROM oauth_authorization_flows WHERE state_hash = ? FOR UPDATE",
    )
    .bind(state_hash)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(flow) = flow else {
        transaction.commit().await?;
        return Ok(None);
    };
    let valid = flow.browser_binding_hash == browser_binding_hash
        && flow.provider == provider
        && flow.expires_at > now
        && flow.consumed_at.is_none();

    if valid {
        sqlx::query(
            "UPDATE oauth_authorization_flows SET consumed_at = ? \
             WHERE state_hash = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(state_hash)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(valid.then_some(flow))
}

pub async fn cleanup_expired(pool: &MySqlPool, now: NaiveDateTime) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM oauth_authorization_flows \
         WHERE expires_at <= ? OR (consumed_at IS NOT NULL AND consumed_at <= DATE_SUB(?, INTERVAL 1 HOUR))",
    )
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM pending_oauth_links WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_user_by_identity(
    pool: &MySqlPool,
    provider: &str,
    subject: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT users.id, users.email, users.password_hash, users.display_name, users.role, \
                users.email_verified, users.account_state, users.session_version, \
                users.erasure_subject \
         FROM oauth_identities \
         JOIN users ON users.id = oauth_identities.user_id \
         WHERE oauth_identities.provider = ? AND oauth_identities.provider_subject = ?",
    )
    .bind(provider)
    .bind(subject)
    .fetch_optional(pool)
    .await
}

pub async fn touch_identity(
    pool: &MySqlPool,
    provider: &str,
    subject: &str,
    now: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE oauth_identities SET last_login_at = ? \
         WHERE provider = ? AND provider_subject = ?",
    )
    .bind(now)
    .bind(provider)
    .bind(subject)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_oauth_user(
    pool: &MySqlPool,
    profile: &OAuthIdentityProfile,
    policy_version: &str,
    consented_at: NaiveDateTime,
) -> Result<User, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO users \
         (email, password_hash, display_name, role, email_verified, privacy_consent_at) \
         VALUES (?, NULL, ?, 'user', TRUE, ?)",
    )
    .bind(&profile.email)
    .bind(&profile.display_name)
    .bind(consented_at)
    .execute(&mut *transaction)
    .await?;
    #[allow(clippy::cast_possible_truncation)]
    let user_id = result.last_insert_id() as u32;

    sqlx::query(
        "INSERT INTO oauth_identities \
         (user_id, provider, provider_subject, last_login_at) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(profile.provider.as_str())
    .bind(&profile.subject)
    .bind(consented_at)
    .execute(&mut *transaction)
    .await?;
    privacy_consents::insert_on_transaction(
        &mut transaction,
        user_id,
        policy_version,
        consented_at,
        profile.provider.as_str(),
    )
    .await?;
    transaction.commit().await?;

    Ok(User {
        id: user_id,
        email: profile.email.clone(),
        password_hash: None,
        display_name: profile.display_name.clone(),
        role: "user".to_string(),
        email_verified: true,
        account_state: "active".to_string(),
        session_version: 0,
        erasure_subject: sqlx::query_scalar("SELECT erasure_subject FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?,
    })
}

pub async fn insert_pending_link(
    pool: &MySqlPool,
    token_hash: &str,
    profile: &OAuthIdentityProfile,
    now: NaiveDateTime,
    expires_at: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pending_oauth_links \
         (token_hash, provider, provider_subject, verified_email, display_name, created_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(token_hash)
    .bind(profile.provider.as_str())
    .bind(&profile.subject)
    .bind(&profile.email)
    .bind(&profile.display_name)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_pending_link(
    pool: &MySqlPool,
    token_hash: &str,
    now: NaiveDateTime,
) -> Result<Option<PendingOAuthLinkRow>, sqlx::Error> {
    sqlx::query_as::<_, PendingOAuthLinkRow>(
        "SELECT provider, provider_subject, verified_email, expires_at \
         FROM pending_oauth_links WHERE token_hash = ? AND expires_at > ?",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPendingResult {
    Linked,
    Missing,
    EmailMismatch,
    Conflict,
}

pub async fn confirm_pending_link(
    pool: &MySqlPool,
    token_hash: &str,
    user_id: u32,
    now: NaiveDateTime,
) -> Result<LinkPendingResult, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let pending = sqlx::query_as::<_, PendingOAuthLinkRow>(
        "SELECT provider, provider_subject, verified_email, expires_at \
         FROM pending_oauth_links WHERE token_hash = ? FOR UPDATE",
    )
    .bind(token_hash)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(pending) = pending else {
        transaction.commit().await?;
        return Ok(LinkPendingResult::Missing);
    };
    if pending.expires_at <= now {
        sqlx::query("DELETE FROM pending_oauth_links WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        return Ok(LinkPendingResult::Missing);
    }

    let user_email = sqlx::query("SELECT email FROM users WHERE id = ? FOR UPDATE")
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await?
        .map(|row| row.get::<String, _>("email"));
    if user_email.as_deref() != Some(pending.verified_email.as_str()) {
        transaction.commit().await?;
        return Ok(LinkPendingResult::EmailMismatch);
    }

    let identity_insert = sqlx::query(
        "INSERT INTO oauth_identities \
         (user_id, provider, provider_subject, last_login_at) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(&pending.provider)
    .bind(&pending.provider_subject)
    .bind(now)
    .execute(&mut *transaction)
    .await;
    if matches!(
        &identity_insert,
        Err(sqlx::Error::Database(error)) if error.is_unique_violation()
    ) {
        transaction.rollback().await?;
        return Ok(LinkPendingResult::Conflict);
    }
    identity_insert?;
    sqlx::query("DELETE FROM pending_oauth_links WHERE token_hash = ?")
        .bind(token_hash)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(LinkPendingResult::Linked)
}

pub async fn delete_pending_link(pool: &MySqlPool, token_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM pending_oauth_links WHERE token_hash = ?")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::oauth::{OAuthIdentityProfile, OAuthProvider};
    use sqlx::mysql::MySqlPoolOptions;

    async fn test_pool() -> Option<MySqlPool> {
        let database_url = std::env::var("DATABASE_URL").ok()?;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        Some(pool)
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    async fn insert_user(pool: &MySqlPool, email: &str) -> u32 {
        let id = sqlx::query(
            "INSERT INTO users (email, display_name, role, email_verified) \
             VALUES (?, 'OAuth DB Tester', 'user', TRUE)",
        )
        .bind(email)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_id();
        id.try_into().unwrap()
    }

    fn profile(suffix: u128) -> OAuthIdentityProfile {
        OAuthIdentityProfile {
            provider: OAuthProvider::Google,
            subject: format!("google-subject-{suffix}"),
            email: format!("oauth-{suffix}@example.test"),
            display_name: "OAuth Collector".to_string(),
        }
    }

    #[tokio::test]
    async fn flow_validates_browser_provider_expiry_and_one_time_use() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = unique_suffix();
        let state_hash = crate::auth::oauth::hash_secret(&format!("state-{suffix}"));
        let browser_hash = crate::auth::oauth::hash_secret(&format!("browser-{suffix}"));
        let now = chrono::Utc::now().naive_utc();
        insert_flow(
            &pool,
            &NewOAuthFlow {
                state_hash: &state_hash,
                browser_binding_hash: &browser_hash,
                provider: "google",
                intent: "login",
                reauth_user_id: None,
                pkce_verifier: "test-pkce-verifier",
                privacy_policy_version: None,
                consented_at: None,
                created_at: now,
                expires_at: now + chrono::Duration::minutes(10),
            },
        )
        .await
        .unwrap();

        assert!(
            consume_flow(&pool, &state_hash, "wrong-browser", "google", now)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            consume_flow(&pool, &state_hash, &browser_hash, "github", now)
                .await
                .unwrap()
                .is_none()
        );
        let flow = consume_flow(&pool, &state_hash, &browser_hash, "google", now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(flow.pkce_verifier, "test-pkce-verifier");
        assert!(
            consume_flow(&pool, &state_hash, &browser_hash, "google", now)
                .await
                .unwrap()
                .is_none()
        );

        sqlx::query("DELETE FROM oauth_authorization_flows WHERE state_hash = ?")
            .bind(&state_hash)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_flow_is_rejected_and_cleanup_removes_ephemeral_rows() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = unique_suffix();
        let state_hash = crate::auth::oauth::hash_secret(&format!("expired-state-{suffix}"));
        let browser_hash = crate::auth::oauth::hash_secret(&format!("expired-browser-{suffix}"));
        let now = chrono::Utc::now().naive_utc();
        insert_flow(
            &pool,
            &NewOAuthFlow {
                state_hash: &state_hash,
                browser_binding_hash: &browser_hash,
                provider: "github",
                intent: "login",
                reauth_user_id: None,
                pkce_verifier: "test-pkce-verifier",
                privacy_policy_version: None,
                consented_at: None,
                created_at: now - chrono::Duration::minutes(20),
                expires_at: now - chrono::Duration::minutes(10),
            },
        )
        .await
        .unwrap();
        let pending_hash = crate::auth::oauth::hash_secret(&format!("pending-{suffix}"));
        let mut pending_profile = profile(suffix);
        pending_profile.provider = OAuthProvider::GitHub;
        insert_pending_link(
            &pool,
            &pending_hash,
            &pending_profile,
            now - chrono::Duration::minutes(20),
            now - chrono::Duration::minutes(10),
        )
        .await
        .unwrap();

        assert!(
            consume_flow(&pool, &state_hash, &browser_hash, "github", now)
                .await
                .unwrap()
                .is_none()
        );
        cleanup_expired(&pool, now).await.unwrap();
        let flow_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM oauth_authorization_flows WHERE state_hash = ?")
                .bind(&state_hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        let pending_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM pending_oauth_links WHERE token_hash = ?")
                .bind(&pending_hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((flow_count.0, pending_count.0), (0, 0));
    }

    #[tokio::test]
    async fn oauth_account_identity_and_consent_are_atomic_and_cascade() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = unique_suffix();
        let profile = profile(suffix);
        let consented_at = chrono::Utc::now().naive_utc();

        let user = create_oauth_user(&pool, &profile, "policy-test-v1", consented_at)
            .await
            .unwrap();
        assert!(user.password_hash.is_none());
        assert!(user.email_verified);
        let found = find_user_by_identity(&pool, "google", &profile.subject)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, user.id);
        let consents = privacy_consents::find_for_user(&pool, user.id)
            .await
            .unwrap();
        assert_eq!(consents.len(), 1);
        assert_eq!(consents[0].policy_version, "policy-test-v1");
        assert_eq!(consents[0].registration_method, "google");

        assert!(
            create_oauth_user(&pool, &profile, "policy-test-v2", consented_at)
                .await
                .is_err()
        );
        let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = ?")
            .bind(&profile.email)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(user_count.0, 1);

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();
        let identity_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM oauth_identities WHERE user_id = ?")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let consent_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM privacy_consents WHERE user_id = ?")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((identity_count.0, consent_count.0), (0, 0));
    }

    #[tokio::test]
    async fn pending_link_requires_the_matching_authenticated_account_and_is_one_time() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = unique_suffix();
        let profile = profile(suffix);
        let matching_user = insert_user(&pool, &profile.email).await;
        let other_user = insert_user(&pool, &format!("other-{suffix}@example.test")).await;
        let token_hash = crate::auth::oauth::hash_secret(&format!("link-{suffix}"));
        let now = chrono::Utc::now().naive_utc();
        insert_pending_link(
            &pool,
            &token_hash,
            &profile,
            now,
            now + chrono::Duration::minutes(10),
        )
        .await
        .unwrap();

        assert!(
            find_pending_link(&pool, &token_hash, now)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            confirm_pending_link(&pool, &token_hash, other_user, now)
                .await
                .unwrap(),
            LinkPendingResult::EmailMismatch
        );
        assert_eq!(
            confirm_pending_link(&pool, &token_hash, matching_user, now)
                .await
                .unwrap(),
            LinkPendingResult::Linked
        );
        assert_eq!(
            confirm_pending_link(&pool, &token_hash, matching_user, now)
                .await
                .unwrap(),
            LinkPendingResult::Missing
        );
        assert!(
            find_user_by_identity(&pool, "google", &profile.subject)
                .await
                .unwrap()
                .is_some()
        );

        let conflicting_token = crate::auth::oauth::hash_secret(&format!("conflict-{suffix}"));
        let conflicting_profile = OAuthIdentityProfile {
            provider: OAuthProvider::Google,
            subject: format!("other-google-subject-{suffix}"),
            email: profile.email.clone(),
            display_name: profile.display_name.clone(),
        };
        insert_pending_link(
            &pool,
            &conflicting_token,
            &conflicting_profile,
            now,
            now + chrono::Duration::minutes(10),
        )
        .await
        .unwrap();
        assert_eq!(
            confirm_pending_link(&pool, &conflicting_token, matching_user, now)
                .await
                .unwrap(),
            LinkPendingResult::Conflict
        );
        assert!(
            find_pending_link(&pool, &conflicting_token, now)
                .await
                .unwrap()
                .is_some()
        );

        sqlx::query("DELETE FROM users WHERE id IN (?, ?)")
            .bind(matching_user)
            .bind(other_user)
            .execute(&pool)
            .await
            .unwrap();
    }
}
