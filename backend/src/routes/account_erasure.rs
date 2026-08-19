use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Utc};

use super::AppState;
use crate::auth::middleware::{AdminUser, AuthUser, OptionalAuthUser, RecentAuthUser};
use crate::auth::oauth::hash_secret;
use crate::auth::password;
use crate::db::{account_erasure, users};
use crate::error::AppError;
use crate::models::account_erasure::{
    ACCOUNT_DELETION_CONFIRMATION, ACCOUNT_DELETION_GRACE_DAYS, AccountDeletionOptionsResponse,
    AccountDeletionStatusResponse, AdminAccountErasureJobResponse, PasswordReauthenticationRequest,
    RECENT_AUTH_SECONDS, RequestAccountDeletion,
};
use crate::models::user::{LoginResponse, MessageResponse};
use crate::services::account_erasure::{
    RECOVERY_COOKIE, cancel, clear_recovery_cookie, recovery_jar, schedule,
};
use crate::services::rate_limit::{PeerAddress, RateLimitPolicy};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/me/account-deletion",
            get(status).post(request_deletion).delete(cancel_deletion),
        )
        .route("/api/v1/me/account-deletion/options", get(deletion_options))
        .route(
            "/api/v1/auth/reauth/password",
            post(reauthenticate_password),
        )
        .route("/api/v1/admin/account-erasure-jobs", get(admin_jobs))
}

async fn status(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    jar: CookieJar,
) -> Result<Json<AccountDeletionStatusResponse>, AppError> {
    let now = Utc::now().naive_utc();
    let result = if let Some(auth) = auth.0 {
        account_erasure::find_status(&state.inner.pool, auth.user_id, now).await?
    } else {
        let raw_token = jar
            .get(RECOVERY_COOKIE)
            .map(axum_extra::extract::cookie::Cookie::value)
            .ok_or_else(recovery_required)?;
        account_erasure::find_status_by_recovery_token(
            &state.inner.pool,
            &hash_secret(raw_token),
            now,
        )
        .await?
    };
    result.map(Json).ok_or_else(recovery_required)
}

async fn deletion_options(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<AccountDeletionOptionsResponse>, AppError> {
    let methods = account_erasure::find_auth_methods(&state.inner.pool, auth.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Account not found".to_string()))?;
    Ok(Json(AccountDeletionOptionsResponse {
        recent_authentication: authentication_is_recent(auth.auth_time, Utc::now().timestamp()),
        password: methods.password,
        google: methods.google,
        github: methods.github,
        confirmation_phrase: ACCOUNT_DELETION_CONFIRMATION,
        grace_days: ACCOUNT_DELETION_GRACE_DAYS,
    }))
}

async fn request_deletion(
    State(state): State<AppState>,
    auth: RecentAuthUser,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    jar: CookieJar,
    Json(payload): Json<RequestAccountDeletion>,
) -> Result<(StatusCode, CookieJar, Json<AccountDeletionStatusResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::AccountDeletion, &client)
        .await?;
    state
        .inner
        .request_security
        .enforce_user(RateLimitPolicy::AccountDeletion, auth.0.user_id)
        .await?;
    payload
        .validate()
        .map_err(|message| AppError::BadRequestWithCode {
            message: message.to_string(),
            code: "ACCOUNT_DELETION_CONFIRMATION_INVALID".to_string(),
        })?;

    let scheduled = schedule(&state.inner.pool, auth.0.user_id, Utc::now().naive_utc()).await?;
    let jar = recovery_jar(
        jar.add(super::auth::clear_cookie("access_token", "/api"))
            .add(super::auth::clear_cookie("refresh_token", "/api/v1/auth")),
        scheduled.recovery_token,
        scheduled.status.scheduled_for,
        state.inner.cookie_secure,
    );
    Ok((StatusCode::ACCEPTED, jar, Json(scheduled.status)))
}

async fn cancel_deletion(
    State(state): State<AppState>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    jar: CookieJar,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::AccountDeletion, &client)
        .await?;
    let raw_token = jar
        .get(RECOVERY_COOKIE)
        .map(axum_extra::extract::cookie::Cookie::value)
        .ok_or_else(recovery_required)?
        .to_string();
    state
        .inner
        .request_security
        .enforce_token(RateLimitPolicy::AccountDeletion, &raw_token)
        .await?;
    let user = cancel(&state.inner.pool, &raw_token, Utc::now().naive_utc()).await?;
    // Possession of the recovery cookie is not a credential check. Keep the
    // restored session deliberately stale so another deletion requires reauth.
    let jar = super::auth::authenticated_jar_at(
        &state,
        jar.add(clear_recovery_cookie()),
        &user,
        DateTime::<Utc>::UNIX_EPOCH.naive_utc(),
    )
    .await?;
    Ok((
        jar,
        Json(LoginResponse {
            message: "Account deletion cancelled".to_string(),
            account_state: None,
            scheduled_for: None,
        }),
    ))
}

async fn reauthenticate_password(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    jar: CookieJar,
    Json(payload): Json<PasswordReauthenticationRequest>,
) -> Result<(CookieJar, Json<MessageResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::Reauth, &client)
        .await?;
    state
        .inner
        .request_security
        .enforce_user(RateLimitPolicy::Reauth, auth.user_id)
        .await?;
    if payload.password.is_empty() || payload.password.len() > 128 {
        return Err(invalid_credentials());
    }
    let user = users::find_user_by_id(&state.inner.pool, auth.user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;
    let password_hash =
        user.password_hash
            .as_deref()
            .ok_or_else(|| AppError::ConflictWithCode {
                message: "Password reauthentication is not available for this account".to_string(),
                code: "REAUTH_METHOD_UNAVAILABLE".to_string(),
            })?;
    if !password::verify_password(&payload.password, password_hash)
        .map_err(|_| invalid_credentials())?
    {
        return Err(invalid_credentials());
    }
    let jar = super::auth::authenticated_jar(&state, jar, &user).await?;
    Ok((
        jar,
        Json(MessageResponse {
            message: "Reauthentication successful".to_string(),
        }),
    ))
}

async fn admin_jobs(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<AdminAccountErasureJobResponse>>, AppError> {
    Ok(Json(
        account_erasure::list_admin_jobs(&state.inner.pool).await?,
    ))
}

fn authentication_is_recent(auth_time: usize, now: i64) -> bool {
    let auth_time = i64::try_from(auth_time).unwrap_or(i64::MAX);
    let age = now.saturating_sub(auth_time);
    auth_time > 0 && (-60..=RECENT_AUTH_SECONDS).contains(&age)
}

fn recovery_required() -> AppError {
    AppError::Forbidden {
        message: "Account deletion recovery is missing or expired".to_string(),
        code: Some("ACCOUNT_DELETION_RECOVERY_REQUIRED".to_string()),
    }
}

fn invalid_credentials() -> AppError {
    AppError::Unauthorized("Invalid credentials".to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use lilly_importer_core::AdapterRegistry;
    use sqlx::mysql::MySqlPoolOptions;

    use super::*;
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;

    async fn database_pool() -> Option<sqlx::MySqlPool> {
        let database_url = std::env::var("DATABASE_URL").ok()?;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .ok()?;
        crate::db::migrate_test_database(&pool).await.ok()?;
        Some(pool)
    }

    fn test_state(pool: sqlx::MySqlPool) -> AppState {
        let media_path = PathBuf::from("/tmp/lilly-account-erasure-route-tests");
        AppState {
            inner: Arc::new(AppStateInner {
                pool,
                jwt_secret: "account-erasure-route-test-secret".to_string(),
                jwt_access_expiry: 900,
                jwt_refresh_expiry: 2_592_000,
                password_reset_ttl_seconds: 3_600,
                email_service: EmailService::Log {
                    from: "test@example.test".to_string(),
                },
                app_base_url: "http://localhost".to_string(),
                cookie_secure: false,
                oauth_service: crate::services::oauth::OAuthService::disabled(),
                privacy_policy_version: "test-v1".to_string(),
                adapter_registry: AdapterRegistry::new(),
                media_path: media_path.clone(),
                media_url_prefix: "/media".to_string(),
                photo_upload_config: crate::config::PhotoUploadConfig::default(),
                media_storage: crate::services::media::MediaStorage::new(&media_path),
                erasure_ledger: crate::services::account_erasure::ErasureLedger::new(
                    media_path.join("erasure-ledger"),
                ),
                import_scheduler_config: ImportSchedulerConfig {
                    enabled: false,
                    schedule: "0 10 6 * * Sat *".to_string(),
                    timezone: "Europe/Berlin".to_string(),
                    adapters: Vec::new(),
                },
                request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
            }),
        }
    }

    #[test]
    fn recent_authentication_has_strict_time_bounds() {
        assert!(authentication_is_recent(1_000, 1_000));
        assert!(authentication_is_recent(1_000, 1_600));
        assert!(authentication_is_recent(1_060, 1_000));
        assert!(!authentication_is_recent(999, 1_600));
        assert!(!authentication_is_recent(1_061, 1_000));
        assert!(!authentication_is_recent(0, 1_000));
    }

    #[test]
    fn stable_recovery_error_code_is_exposed() {
        let AppError::Forbidden { code, .. } = recovery_required() else {
            panic!("expected forbidden error");
        };
        assert_eq!(code.as_deref(), Some("ACCOUNT_DELETION_RECOVERY_REQUIRED"));
    }

    #[tokio::test]
    async fn cancellation_session_requires_fresh_reauthentication_before_another_deletion() {
        let Some(pool) = database_pool().await else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, email_verified) VALUES (?, ?, TRUE)",
        )
        .bind(format!("recovery-session-{suffix}@example.test"))
        .bind("Recovery Session")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let now = Utc::now().naive_utc();
        let recovery_token = crate::auth::oauth::random_urlsafe_token();
        sqlx::query(
            "INSERT INTO account_erasure_jobs \
             (user_id, previous_profile_public, previous_collection_public, requested_at, scheduled_for) \
             VALUES (?, TRUE, TRUE, ?, ?)",
        )
        .bind(user_id)
        .bind(now)
        .bind(now + chrono::Duration::days(7))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO account_erasure_recovery_tokens \
             (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(hash_secret(&recovery_token))
        .bind(user_id)
        .bind(now)
        .bind(now + chrono::Duration::days(7))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE users SET account_state = 'pending_deletion', \
             profile_public = FALSE, collection_public = FALSE, \
             session_version = session_version + 1 WHERE id = ?",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        let recovery_cookie =
            crate::services::account_erasure::recovery_cookie(recovery_token, 600, false);

        let (jar, _) = cancel_deletion(
            State(test_state(pool.clone())),
            HeaderMap::new(),
            PeerAddress(None),
            CookieJar::new().add(recovery_cookie),
        )
        .await
        .unwrap();

        let access_token = jar.get("access_token").unwrap().value();
        let claims =
            crate::auth::jwt::validate_token(access_token, "account-erasure-route-test-secret")
                .unwrap();
        assert_eq!(claims.auth_time, 0);
        assert!(!authentication_is_recent(
            claims.auth_time,
            Utc::now().timestamp()
        ));
        let refresh_authenticated_at: chrono::NaiveDateTime = sqlx::query_scalar(
            "SELECT authenticated_at FROM refresh_tokens \
             WHERE user_id = ? AND revoked = FALSE ORDER BY id DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(refresh_authenticated_at.and_utc().timestamp(), 0);

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
