use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use chrono::Utc;

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
    let jar =
        super::auth::authenticated_jar(&state, jar.add(clear_recovery_cookie()), &user).await?;
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
    use super::*;

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
}
