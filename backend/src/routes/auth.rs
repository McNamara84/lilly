use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use validator::Validate;

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::auth::oauth::{hash_secret, random_urlsafe_token};
use crate::auth::{jwt, password};
use crate::db::{password_reset_tokens, refresh_tokens, users};
use crate::error::AppError;
use crate::models::user::{
    LoginRequest, LoginResponse, MeResponse, MessageResponse, PasswordResetConfirmRequest,
    PasswordResetRequest, RegisterRequest, RegisterResponse, ResendVerificationRequest,
    VerifyQuery, normalize_email,
};
use crate::services::rate_limit::{PeerAddress, RateLimitPolicy};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/verify", get(verify_email))
        .route(
            "/api/v1/auth/resend-verification",
            post(resend_verification),
        )
        .route(
            "/api/v1/auth/password-reset/request",
            post(request_password_reset),
        )
        .route(
            "/api/v1/auth/password-reset/confirm",
            post(confirm_password_reset),
        )
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
}

fn generate_random_token() -> String {
    use rand::RngExt;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn build_cookie(
    name: &str,
    value: String,
    path: &str,
    max_age_secs: i64,
    secure: bool,
) -> Cookie<'static> {
    let mut cookie = Cookie::build((name.to_string(), value))
        .path(path.to_string())
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(max_age_secs))
        .build();
    cookie.set_name(name.to_string());
    cookie
}

pub(super) fn clear_cookie(name: &str, path: &str) -> Cookie<'static> {
    Cookie::build((name.to_string(), String::new()))
        .path(path.to_string())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build()
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    Json(mut payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::Register, &client)
        .await?;
    payload.email = normalize_email(&payload.email)
        .map_err(|message| field_validation_error("email", message))?;
    payload
        .validate()
        .map_err(|errors| register_validation_error(&errors))?;

    if !payload.privacy_consent {
        return Err(field_validation_error(
            "privacy_consent",
            "Privacy consent is required",
        ));
    }

    if payload.privacy_policy_version != state.inner.privacy_policy_version {
        return Err(AppError::ConflictWithCode {
            message: "Privacy policy changed; please review it again".to_string(),
            code: "PRIVACY_POLICY_CHANGED".to_string(),
        });
    }

    if payload.password != payload.password_confirmation {
        return Err(field_validation_error(
            "password_confirmation",
            "Passwords do not match",
        ));
    }

    password::validate_password_strength(&payload.password, &payload.email, &payload.display_name)
        .map_err(|message| field_validation_error("password", message))?;

    // Check if email already exists — return same success message to prevent user enumeration
    if users::find_user_by_email(&state.inner.pool, &payload.email)
        .await?
        .is_some()
    {
        return Ok((
            StatusCode::CREATED,
            Json(RegisterResponse {
                message: "Registration successful. Please check your email to verify your account."
                    .to_string(),
            }),
        ));
    }

    let password_hash = password::hash_password(&payload.password)
        .map_err(|e| AppError::InternalError(anyhow::anyhow!("Password hashing failed: {e}")))?;

    let verification_token = generate_random_token();
    let verification_token_hash = hash_token(&verification_token);

    #[allow(clippy::cast_possible_truncation)]
    let now = Utc::now().naive_utc();
    let expires_at = now + chrono::Duration::hours(24);

    let user_created = match users::create_user(
        &state.inner.pool,
        &payload.email,
        &password_hash,
        &payload.display_name,
        &verification_token_hash,
        expires_at,
        now,
        &payload.privacy_policy_version,
    )
    .await
    {
        Ok(_) => true,
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            // Race condition: concurrent registration with the same email.
            // Return generic success to prevent user enumeration.
            false
        }
        Err(e) => {
            tracing::error!("Failed to create user: {e}");
            return Err(AppError::InternalError(anyhow::anyhow!(
                "Failed to create user"
            )));
        }
    };

    // Send verification email only if user was actually created
    if user_created
        && let Err(e) = state
            .inner
            .email_service
            .send_verification_email(
                &payload.email,
                &payload.display_name,
                &verification_token,
                &state.inner.app_base_url,
            )
            .await
    {
        tracing::error!("Failed to send verification email: {e}");
    }

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            message: "Registration successful. Please check your email to verify your account."
                .to_string(),
        }),
    ))
}

fn field_validation_error(field: &str, message: impl Into<String>) -> AppError {
    AppError::Validation {
        fields: BTreeMap::from([(field.to_string(), message.into())]),
    }
}

fn register_validation_error(errors: &validator::ValidationErrors) -> AppError {
    let fields = errors
        .field_errors()
        .iter()
        .filter_map(|(field, errors)| {
            errors.first().map(|error| {
                (
                    (*field).to_string(),
                    error
                        .message
                        .as_deref()
                        .unwrap_or("Invalid value")
                        .to_string(),
                )
            })
        })
        .collect();
    AppError::Validation { fields }
}

async fn verify_email(State(state): State<AppState>, Query(query): Query<VerifyQuery>) -> Response {
    let redirect_ok = format!("{}/login?verified=true", state.inner.app_base_url);
    let redirect_err = format!("{}/login?verify_error=invalid", state.inner.app_base_url);

    let token_hash = hash_token(&query.token);
    let user = match users::find_user_by_verification_token(&state.inner.pool, &token_hash).await {
        Ok(Some(user)) => user,
        Ok(None) => return Redirect::to(&redirect_err).into_response(),
        Err(e) => {
            tracing::error!("DB error during email verification: {e}");
            return Redirect::to(&redirect_err).into_response();
        }
    };

    // Check token expiry
    match users::get_verification_token_expiry(&state.inner.pool, user.id).await {
        Ok(Some(expires_at)) if expires_at > Utc::now().naive_utc() => {}
        _ => return Redirect::to(&redirect_err).into_response(),
    }

    if let Err(e) = users::verify_user_email(&state.inner.pool, user.id).await {
        tracing::error!("Failed to verify user email: {e}");
        return Redirect::to(&redirect_err).into_response();
    }

    tracing::info!(user_id = user.id, "Email address verified");
    Redirect::to(&redirect_ok).into_response()
}

async fn resend_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    Json(mut payload): Json<ResendVerificationRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::ResendVerification, &client)
        .await?;
    payload.email = normalize_email(&payload.email).map_err(AppError::BadRequest)?;
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format!("Validation error: {e}")))?;
    state
        .inner
        .request_security
        .enforce_account(RateLimitPolicy::ResendVerification, &payload.email)
        .await?;

    // Always return success to prevent user enumeration
    if let Ok(Some(user)) = users::find_user_by_email(&state.inner.pool, &payload.email).await
        && !user.email_verified
    {
        let token = generate_random_token();
        let token_hash = hash_token(&token);
        let expires_at = Utc::now().naive_utc() + chrono::Duration::hours(24);

        if let Err(e) =
            users::update_verification_token(&state.inner.pool, user.id, &token_hash, expires_at)
                .await
        {
            tracing::error!("Failed to update verification token: {e}");
        } else if let Err(e) = state
            .inner
            .email_service
            .send_verification_email(
                &user.email,
                &user.display_name,
                &token,
                &state.inner.app_base_url,
            )
            .await
        {
            tracing::error!("Failed to resend verification email: {e}");
        }
    }

    Ok(Json(MessageResponse {
        message: "If an account with this email exists and is not yet verified, a new verification email has been sent.".to_string(),
    }))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    jar: CookieJar,
    Json(mut payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::LoginClient, &client)
        .await?;
    payload.email = normalize_email(&payload.email).map_err(AppError::BadRequest)?;
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format!("Validation error: {e}")))?;
    state
        .inner
        .request_security
        .enforce_account(RateLimitPolicy::LoginAccount, &payload.email)
        .await?;

    let user = users::find_user_by_email(&state.inner.pool, &payload.email)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid email or password".to_string()))?;

    let password_hash = user.password_hash.as_ref().ok_or_else(|| {
        tracing::warn!("Password login attempted for an OAuth-only account");
        AppError::Unauthorized("Invalid email or password".to_string())
    })?;

    let valid = password::verify_password(&payload.password, password_hash)
        .map_err(|_| AppError::Unauthorized("Invalid email or password".to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized(
            "Invalid email or password".to_string(),
        ));
    }

    // Check email verification
    if !user.email_verified {
        return Err(AppError::Forbidden {
            message: "Email not verified".to_string(),
            code: Some("EMAIL_NOT_VERIFIED".to_string()),
        });
    }

    if !user.is_active() {
        let (recovery_token, scheduled_for) =
            crate::services::account_erasure::issue_recovery_token(
                &state.inner.pool,
                user.id,
                Utc::now().naive_utc(),
            )
            .await?
            .ok_or_else(|| AppError::ConflictWithCode {
                message: "The account deletion can no longer be cancelled".to_string(),
                code: "ACCOUNT_DELETION_WINDOW_EXPIRED".to_string(),
            })?;
        let jar = crate::services::account_erasure::recovery_jar(
            jar.add(clear_cookie("access_token", "/api"))
                .add(clear_cookie("refresh_token", "/api/v1/auth")),
            recovery_token,
            scheduled_for,
            state.inner.cookie_secure,
        );
        return Ok((
            jar,
            Json(LoginResponse {
                message: "Account deletion is pending".to_string(),
                account_state: Some("pending_deletion".to_string()),
                scheduled_for: Some(scheduled_for),
            }),
        ));
    }

    let jar = authenticated_jar(&state, jar, &user).await?;

    Ok((
        jar,
        Json(LoginResponse {
            message: "Login successful".to_string(),
            account_state: None,
            scheduled_for: None,
        }),
    ))
}

pub(super) async fn authenticated_jar(
    state: &AppState,
    jar: CookieJar,
    user: &crate::models::user::User,
) -> Result<CookieJar, AppError> {
    if !user.is_active() {
        return Err(AppError::Forbidden {
            message: "Account deletion is pending".to_string(),
            code: Some("ACCOUNT_DELETION_PENDING".to_string()),
        });
    }
    let authenticated_at = Utc::now().naive_utc();
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let auth_time = authenticated_at.and_utc().timestamp() as usize;
    let access_token = jwt::create_token_with_auth(
        user.id,
        &user.display_name,
        &user.role,
        &state.inner.jwt_secret,
        state.inner.jwt_access_expiry,
        auth_time,
        user.session_version,
        false,
    )?;
    let raw_refresh_token = generate_random_token();
    let refresh_token_hash = hash_token(&raw_refresh_token);
    #[allow(clippy::cast_possible_truncation)]
    let refresh_expires_at = Utc::now().naive_utc()
        + chrono::Duration::seconds(state.inner.jwt_refresh_expiry.cast_signed());
    refresh_tokens::store_refresh_token(
        &state.inner.pool,
        user.id,
        &refresh_token_hash,
        refresh_expires_at,
        authenticated_at,
    )
    .await?;
    Ok(jar
        .add(build_cookie(
            "access_token",
            access_token,
            "/api",
            state.inner.jwt_access_expiry.cast_signed(),
            state.inner.cookie_secure,
        ))
        .add(build_cookie(
            "refresh_token",
            raw_refresh_token,
            "/api/v1/auth",
            state.inner.jwt_refresh_expiry.cast_signed(),
            state.inner.cookie_secure,
        )))
}

async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    jar: CookieJar,
) -> Result<(CookieJar, Json<MessageResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::Refresh, &client)
        .await?;
    let raw_refresh_token = jar
        .get("refresh_token")
        .map(|c| c.value().to_string())
        .ok_or_else(|| AppError::Unauthorized("Missing refresh token".to_string()))?;

    let token_hash = hash_token(&raw_refresh_token);

    let token_row = refresh_tokens::find_valid_refresh_token(&state.inner.pool, &token_hash)
        .await?
        .ok_or_else(|| {
            tracing::warn!("Invalid refresh token used — possible token reuse attack");
            AppError::Unauthorized("Invalid refresh token".to_string())
        })?;

    // Load user to get current display_name
    let user = users::find_user_by_id(&state.inner.pool, token_row.user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;
    if !user.is_active() {
        return Err(AppError::Forbidden {
            message: "Account deletion is pending".to_string(),
            code: Some("ACCOUNT_DELETION_PENDING".to_string()),
        });
    }

    // Issue new access token
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let auth_time = token_row.authenticated_at.and_utc().timestamp() as usize;
    let new_access_token = jwt::create_token_with_auth(
        user.id,
        &user.display_name,
        &user.role,
        &state.inner.jwt_secret,
        state.inner.jwt_access_expiry,
        auth_time,
        user.session_version,
        false,
    )?;

    // Issue new refresh token
    let new_raw_refresh = generate_random_token();
    let new_refresh_hash = hash_token(&new_raw_refresh);

    #[allow(clippy::cast_possible_truncation)]
    let refresh_expires_at = Utc::now().naive_utc()
        + chrono::Duration::seconds(state.inner.jwt_refresh_expiry.cast_signed());

    // Atomically revoke old token and store new one
    refresh_tokens::rotate_refresh_token(
        &state.inner.pool,
        &token_hash,
        user.id,
        &new_refresh_hash,
        refresh_expires_at,
        token_row.authenticated_at,
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => {
            tracing::warn!("Refresh token already revoked — possible replay attack");
            AppError::Unauthorized("Invalid refresh token".to_string())
        }
        other => AppError::from(other),
    })?;

    let access_cookie = build_cookie(
        "access_token",
        new_access_token,
        "/api",
        state.inner.jwt_access_expiry.cast_signed(),
        state.inner.cookie_secure,
    );

    let refresh_cookie = build_cookie(
        "refresh_token",
        new_raw_refresh,
        "/api/v1/auth",
        state.inner.jwt_refresh_expiry.cast_signed(),
        state.inner.cookie_secure,
    );

    let jar = jar.add(access_cookie).add(refresh_cookie);

    Ok((
        jar,
        Json(MessageResponse {
            message: "Token refreshed".to_string(),
        }),
    ))
}

const PASSWORD_RESET_REQUEST_MESSAGE: &str =
    "If an eligible account exists, a password reset email has been sent.";

async fn request_password_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    Json(mut payload): Json<PasswordResetRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::PasswordResetRequest, &client)
        .await?;
    payload.email = normalize_email(&payload.email)
        .map_err(|message| field_validation_error("email", message))?;
    payload
        .validate()
        .map_err(|errors| register_validation_error(&errors))?;
    state
        .inner
        .request_security
        .enforce_account(RateLimitPolicy::PasswordResetRequest, &payload.email)
        .await?;

    let now = Utc::now().naive_utc();
    if let Err(error) = password_reset_tokens::delete_expired(&state.inner.pool, now).await {
        tracing::warn!(error = %error, "Failed to clean expired password reset tokens");
    }
    let user = match users::find_user_by_email(&state.inner.pool, &payload.email).await {
        Ok(Some(user))
            if user.is_active() && user.email_verified && user.password_hash.is_some() =>
        {
            Some(user)
        }
        Ok(_) => None,
        Err(error) => {
            tracing::error!(error = %error, "Password reset account lookup failed");
            None
        }
    };
    if let Some(user) = user {
        let raw_token = random_urlsafe_token();
        let token_hash = hash_secret(&raw_token);
        let expires_at =
            now + chrono::Duration::seconds(state.inner.password_reset_ttl_seconds.cast_signed());
        match password_reset_tokens::replace_active_token(
            &state.inner.pool,
            user.id,
            &token_hash,
            now,
            expires_at,
        )
        .await
        {
            Ok(()) => {
                if let Err(error) = state
                    .inner
                    .email_service
                    .send_password_reset_email(
                        &user.email,
                        &user.display_name,
                        &raw_token,
                        &state.inner.app_base_url,
                        state.inner.password_reset_ttl_seconds,
                    )
                    .await
                {
                    tracing::error!(error = %error, "Failed to send password reset email");
                }
            }
            Err(error) => {
                tracing::error!(error = %error, "Failed to persist password reset token");
            }
        }
    }

    Ok(Json(MessageResponse {
        message: PASSWORD_RESET_REQUEST_MESSAGE.to_string(),
    }))
}

async fn confirm_password_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    jar: CookieJar,
    Json(payload): Json<PasswordResetConfirmRequest>,
) -> Result<(CookieJar, Json<MessageResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::PasswordResetConfirm, &client)
        .await?;
    payload
        .validate()
        .map_err(|errors| register_validation_error(&errors))?;
    state
        .inner
        .request_security
        .enforce_token(RateLimitPolicy::PasswordResetConfirm, &payload.token)
        .await?;
    if payload.password != payload.password_confirmation {
        return Err(field_validation_error(
            "password_confirmation",
            "Passwords do not match",
        ));
    }

    let now = Utc::now().naive_utc();
    let token_hash = hash_secret(&payload.token);
    let target = password_reset_tokens::find_valid_target(&state.inner.pool, &token_hash, now)
        .await?
        .ok_or_else(invalid_password_reset_token)?;
    password::validate_password_strength(&payload.password, &target.email, &target.display_name)
        .map_err(|message| field_validation_error("password", message))?;
    let password_hash = password::hash_password(&payload.password).map_err(|error| {
        AppError::InternalError(anyhow::anyhow!("Password hashing failed: {error}"))
    })?;

    match password_reset_tokens::consume_and_update_password(
        &state.inner.pool,
        &token_hash,
        &password_hash,
        now,
    )
    .await?
    {
        password_reset_tokens::ConsumePasswordResetResult::Updated => {
            let jar = jar
                .add(clear_cookie("access_token", "/api"))
                .add(clear_cookie("refresh_token", "/api/v1/auth"));
            Ok((
                jar,
                Json(MessageResponse {
                    message: "Password reset successful. Please sign in again.".to_string(),
                }),
            ))
        }
        password_reset_tokens::ConsumePasswordResetResult::Invalid => {
            Err(invalid_password_reset_token())
        }
    }
}

fn invalid_password_reset_token() -> AppError {
    AppError::BadRequestWithCode {
        message: "Password reset link is invalid or expired.".to_string(),
        code: "PASSWORD_RESET_TOKEN_INVALID".to_string(),
    }
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<MessageResponse>), AppError> {
    // Revoke refresh token if present
    if let Some(cookie) = jar.get("refresh_token") {
        let token_hash = hash_token(cookie.value());
        let _ = refresh_tokens::revoke_refresh_token(&state.inner.pool, &token_hash).await;
    }

    // Clear cookies
    let jar = jar
        .add(clear_cookie("access_token", "/api"))
        .add(clear_cookie("refresh_token", "/api/v1/auth"));

    Ok((
        jar,
        Json(MessageResponse {
            message: "Logged out".to_string(),
        }),
    ))
}

async fn me(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<MeResponse>, AppError> {
    let user = users::find_user_by_id(&state.inner.pool, auth_user.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(Json(MeResponse {
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        email_verified: user.email_verified,
        role: user.role,
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, header};
    use lilly_importer_core::AdapterRegistry;
    use sqlx::mysql::MySqlPoolOptions;
    use tower::ServiceExt;

    use super::*;
    use crate::routes::AppStateInner;
    use crate::services::admin_roles::{PromotionResult, RoleChangeMethod};
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;

    fn test_state(pool: sqlx::MySqlPool) -> AppState {
        let media_path = PathBuf::from("/tmp/lilly-auth-route-tests");
        AppState {
            inner: Arc::new(AppStateInner {
                pool,
                jwt_secret: "auth-route-test-secret".to_string(),
                jwt_access_expiry: 900,
                jwt_refresh_expiry: 2_592_000,
                password_reset_ttl_seconds: 3_600,
                email_service: EmailService::Log {
                    from: "test@example.test".to_string(),
                },
                app_base_url: "http://localhost".to_string(),
                cookie_secure: false,
                oauth_service: crate::services::oauth::OAuthService::disabled(),
                privacy_policy_version: "policy-test-v1".to_string(),
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
    fn test_generate_random_token_length() {
        let token = generate_random_token();
        // 32 random bytes → 64 hex chars
        assert_eq!(token.len(), 64);
    }

    #[test]
    fn test_generate_random_token_uniqueness() {
        let t1 = generate_random_token();
        let t2 = generate_random_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_generate_random_token_is_hex() {
        let token = generate_random_token();
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_token_deterministic() {
        let input = "my_secret_token";
        let h1 = hash_token(input);
        let h2 = hash_token(input);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_token_different_inputs_different_hashes() {
        let h1 = hash_token("token_a");
        let h2 = hash_token("token_b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_token_is_sha256_hex() {
        let h = hash_token("test");
        // SHA-256 produces 32 bytes = 64 hex chars
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_build_cookie_properties() {
        let cookie = build_cookie("access_token", "value123".to_string(), "/api", 900, true);
        assert_eq!(cookie.name(), "access_token");
        assert_eq!(cookie.value(), "value123");
        assert_eq!(cookie.path(), Some("/api"));
        assert!(cookie.http_only().unwrap_or(false));
        assert!(cookie.secure().unwrap_or(false));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.max_age(), Some(time::Duration::seconds(900)));
    }

    #[test]
    fn test_build_cookie_not_secure() {
        let cookie = build_cookie("test", "val".to_string(), "/", 60, false);
        assert!(!cookie.secure().unwrap_or(true));
    }

    #[test]
    fn test_clear_cookie_properties() {
        let cookie = clear_cookie("access_token", "/api");
        assert_eq!(cookie.name(), "access_token");
        assert_eq!(cookie.value(), "");
        assert_eq!(cookie.path(), Some("/api"));
        assert!(cookie.http_only().unwrap_or(false));
        assert_eq!(cookie.max_age(), Some(time::Duration::ZERO));
    }

    #[tokio::test]
    async fn registration_validation_returns_field_specific_errors() {
        let pool = MySqlPoolOptions::new()
            .connect_lazy("mysql://test:test@localhost/test")
            .unwrap();
        let response = Router::new()
            .merge(router())
            .with_state(test_state(pool))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"display_name":"M","email":"invalid","password":"short","password_confirmation":"different","privacy_consent":false,"privacy_policy_version":"policy-test-v1"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "Validation failed");
        assert_eq!(json["fields"]["email"], "Invalid email format");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn password_registration_is_atomic_versioned_and_indistinguishable_when_repeated() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _database_guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let email = format!("registration-route-{suffix}@example.test");
        let stale_email = format!("registration-stale-{suffix}@example.test");
        let state = test_state(pool.clone());
        let credential = crate::auth::oauth::random_urlsafe_token();
        let request_body = serde_json::json!({
            "display_name": "Route Collector",
            "email": email,
            "password": credential,
            "password_confirmation": credential,
            "privacy_consent": true,
            "privacy_policy_version": "policy-test-v1"
        })
        .to_string();

        let first = Router::new()
            .merge(router())
            .with_state(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let second = Router::new()
            .merge(router())
            .with_state(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::CREATED);
        assert_eq!(second.status(), StatusCode::CREATED);
        let first_body = to_bytes(first.into_body(), usize::MAX).await.unwrap();
        let second_body = to_bytes(second.into_body(), usize::MAX).await.unwrap();
        assert_eq!(first_body, second_body);

        let stale_body = serde_json::json!({
            "display_name": "Stale Collector",
            "email": stale_email,
            "password": credential,
            "password_confirmation": credential,
            "privacy_consent": true,
            "privacy_policy_version": "policy-stale"
        })
        .to_string();
        let stale_response = Router::new()
            .merge(router())
            .with_state(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(stale_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale_response.status(), StatusCode::CONFLICT);
        let stale_json: serde_json::Value = serde_json::from_slice(
            &to_bytes(stale_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(stale_json["code"], "PRIVACY_POLICY_CHANGED");

        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT \
               (SELECT COUNT(*) FROM users WHERE email = ?), \
               (SELECT COUNT(*) FROM privacy_consents pc JOIN users u ON u.id = pc.user_id WHERE u.email = ?), \
               (SELECT COUNT(*) FROM users WHERE email = ?)",
        )
        .bind(&email)
        .bind(&email)
        .bind(&stale_email)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 0));

        sqlx::query("DELETE FROM users WHERE email = ?")
            .bind(email)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn password_reset_request_rate_limit_returns_retry_metadata() {
        let pool = MySqlPoolOptions::new()
            .connect_lazy("mysql://test:test@localhost/test")
            .unwrap();
        let mut state = test_state(pool);
        let inner = Arc::get_mut(&mut state.inner).unwrap();
        let limits = crate::config::RateLimitConfig {
            password_reset_request: crate::config::RateLimitRule {
                max_requests: 1,
                window_seconds: 120,
            },
            ..crate::config::RateLimitConfig::default()
        };
        let request_security = crate::services::rate_limit::RequestSecurity::new(
            limits,
            Vec::new(),
            "password-reset-limit-test",
        );
        let client = request_security.client_identity(&HeaderMap::new(), None);
        request_security
            .enforce_client(RateLimitPolicy::PasswordResetRequest, &client)
            .await
            .unwrap();
        inner.request_security = request_security;
        let app = Router::new().merge(router()).with_state(state);
        let body = r#"{"email":"collector@example.test"}"#;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/password-reset/request")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after = response.headers()[header::RETRY_AFTER]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!((1..=120).contains(&retry_after));
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(json["code"], "RATE_LIMITED");
        assert_eq!(json["retry_after_seconds"], retry_after);
    }

    #[tokio::test]
    async fn password_reset_request_is_indistinguishable_for_eligible_and_ineligible_accounts() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _database_guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let suffix = crate::auth::oauth::random_urlsafe_token();
        let eligible_email = format!("reset-eligible-{suffix}@example.test");
        let oauth_email = format!("reset-oauth-{suffix}@example.test");
        let unknown_email = format!("reset-unknown-{suffix}@example.test");
        sqlx::query(
            "INSERT INTO users (email, password_hash, display_name, email_verified) \
             VALUES (?, 'password-hash', 'Eligible', TRUE), \
                    (?, NULL, 'OAuth only', TRUE)",
        )
        .bind(&eligible_email)
        .bind(&oauth_email)
        .execute(&pool)
        .await
        .unwrap();
        let app = Router::new()
            .merge(router())
            .with_state(test_state(pool.clone()));
        let mut bodies = Vec::new();

        for email in [&eligible_email, &oauth_email, &unknown_email] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/auth/password-reset/request")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            serde_json::json!({ "email": email }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            bodies.push(to_bytes(response.into_body(), usize::MAX).await.unwrap());
        }
        assert_eq!(bodies[0], bodies[1]);
        assert_eq!(bodies[1], bodies[2]);
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT \
               (SELECT COUNT(*) FROM password_reset_tokens prt \
                JOIN users u ON u.id = prt.user_id WHERE u.email = ?), \
               (SELECT COUNT(*) FROM password_reset_tokens prt \
                JOIN users u ON u.id = prt.user_id WHERE u.email = ?)",
        )
        .bind(&eligible_email)
        .bind(&oauth_email)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 0));
        sqlx::query("DELETE FROM users WHERE email IN (?, ?)")
            .bind(eligible_email)
            .bind(oauth_email)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn password_reset_confirmation_changes_password_revokes_sessions_and_is_single_use() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _database_guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let suffix = crate::auth::oauth::random_urlsafe_token();
        let email = format!("reset-confirm-{suffix}@example.test");
        let old_password = "old correct horse battery staple 2048";
        let new_password = "new correct horse battery staple 2049";
        let old_hash = password::hash_password(old_password).unwrap();
        let result = sqlx::query(
            "INSERT INTO users (email, password_hash, display_name, email_verified) \
             VALUES (?, ?, 'Reset Confirmation', TRUE)",
        )
        .bind(&email)
        .bind(old_hash)
        .execute(&pool)
        .await
        .unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let user_id = result.last_insert_id() as u32;
        let raw_token = crate::auth::oauth::random_urlsafe_token();
        let now = Utc::now().naive_utc();
        password_reset_tokens::replace_active_token(
            &pool,
            user_id,
            &hash_secret(&raw_token),
            now,
            now + chrono::Duration::hours(1),
        )
        .await
        .unwrap();
        refresh_tokens::store_refresh_token(
            &pool,
            user_id,
            &hash_token(&format!("refresh-{suffix}")),
            now + chrono::Duration::hours(1),
            now,
        )
        .await
        .unwrap();
        let app = Router::new()
            .merge(router())
            .with_state(test_state(pool.clone()));
        let request_body = serde_json::json!({
            "token": raw_token,
            "password": new_password,
            "password_confirmation": new_password
        })
        .to_string();

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/password-reset/confirm")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("cookie", "access_token=old; refresh_token=old")
                    .body(Body::from(request_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let cleared_cookies = first
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>();
        assert_eq!(cleared_cookies.len(), 2);
        assert!(
            cleared_cookies
                .iter()
                .all(|cookie| cookie.contains("Max-Age=0"))
        );

        let row: (String, bool) = sqlx::query_as(
            "SELECT users.password_hash, refresh_tokens.revoked FROM users \
             INNER JOIN refresh_tokens ON refresh_tokens.user_id = users.id \
             WHERE users.id = ?",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(password::verify_password(new_password, &row.0).unwrap());
        assert!(!password::verify_password(old_password, &row.0).unwrap());
        assert!(row.1);

        let repeated = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/password-reset/confirm")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(repeated.status(), StatusCode::BAD_REQUEST);
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(repeated.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(json["code"], "PASSWORD_RESET_TOKEN_INVALID");

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn password_reset_confirmation_rejects_mismatched_passwords_before_database_access() {
        let pool = MySqlPoolOptions::new()
            .connect_lazy("mysql://test:test@localhost/test")
            .unwrap();
        let response = Router::new()
            .merge(router())
            .with_state(test_state(pool))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/password-reset/confirm")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "token": crate::auth::oauth::random_urlsafe_token(),
                            "password": "correct horse battery staple 2049",
                            "password_confirmation": "different horse battery staple 2050"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(
            json["fields"]["password_confirmation"],
            "Passwords do not match"
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn refresh_uses_the_current_database_role_after_promotion() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _database_guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let email = format!("refresh-role-{suffix}@example.test");
        let user_id: u32 = sqlx::query(
            "INSERT INTO users (email, display_name, role, email_verified) \
             VALUES (?, 'Refresh Role Tester', 'user', TRUE)",
        )
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_id()
        .try_into()
        .unwrap();
        let old_access = jwt::create_token(
            user_id,
            "Refresh Role Tester",
            "user",
            "refresh-role-secret",
            900,
        )
        .unwrap();
        let raw_refresh = format!("refresh-token-{suffix}");
        let original_authenticated_at = chrono::DateTime::from_timestamp(
            Utc::now().timestamp() - chrono::Duration::minutes(20).num_seconds(),
            0,
        )
        .unwrap()
        .naive_utc();
        refresh_tokens::store_refresh_token(
            &pool,
            user_id,
            &hash_token(&raw_refresh),
            Utc::now().naive_utc() + chrono::Duration::hours(1),
            original_authenticated_at,
        )
        .await
        .unwrap();
        assert_eq!(
            crate::services::admin_roles::promote_user_to_admin(
                &pool,
                &email,
                RoleChangeMethod::Cli,
            )
            .await
            .unwrap(),
            PromotionResult::Promoted { user_id }
        );
        let role_change_event_id: (u32,) =
            sqlx::query_as("SELECT id FROM role_change_events WHERE target_user_id = ?")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        let state = AppState {
            inner: Arc::new(AppStateInner {
                pool: pool.clone(),
                jwt_secret: "refresh-role-secret".to_string(),
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
                media_path: PathBuf::from("/tmp/lilly-refresh-role-test"),
                media_url_prefix: "/media".to_string(),
                photo_upload_config: crate::config::PhotoUploadConfig::default(),
                media_storage: crate::services::media::MediaStorage::new(std::path::Path::new(
                    "/tmp/lilly-refresh-role-test",
                )),
                erasure_ledger: crate::services::account_erasure::ErasureLedger::new(
                    "/tmp/lilly-refresh-role-test-erasure-ledger",
                ),
                import_scheduler_config: ImportSchedulerConfig {
                    enabled: false,
                    schedule: "0 10 6 * * Sat *".to_string(),
                    timezone: "Europe/Berlin".to_string(),
                    adapters: Vec::new(),
                },
                request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
            }),
        };
        let response = Router::new()
            .merge(router())
            .with_state(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/refresh")
                    .header("cookie", format!("refresh_token={raw_refresh}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let access_cookie = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with("access_token="))
            .unwrap();
        let access_token = access_cookie
            .trim_start_matches("access_token=")
            .split(';')
            .next()
            .unwrap();
        assert_eq!(
            jwt::validate_token(&old_access, "refresh-role-secret")
                .unwrap()
                .role,
            "user",
            "an already-issued access token keeps its role until expiry"
        );
        let refreshed_claims = jwt::validate_token(access_token, "refresh-role-secret").unwrap();
        assert_eq!(
            refreshed_claims.role, "admin",
            "refresh must load the current role from the database"
        );
        assert_eq!(
            refreshed_claims.auth_time,
            usize::try_from(original_authenticated_at.and_utc().timestamp()).unwrap(),
            "refresh must not make authentication recent again"
        );
        assert_eq!(
            sqlx::query_scalar::<_, chrono::NaiveDateTime>(
                "SELECT authenticated_at FROM refresh_tokens \
                 WHERE user_id = ? AND revoked = FALSE",
            )
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            original_authenticated_at,
            "rotated refresh token must retain the credential-check time"
        );

        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM role_change_events WHERE id = ?")
            .bind(role_change_event_id.0)
            .execute(&pool)
            .await
            .unwrap();
    }
}
