use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::AppState;
use crate::auth::middleware::AuthUser;
use crate::auth::{jwt, password};
use crate::db::{refresh_tokens, users};
use crate::error::AppError;
use crate::models::user::{
    LoginRequest, LoginResponse, MeResponse, MessageResponse, RegisterRequest, RegisterResponse,
    ResendVerificationRequest, VerifyQuery,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/verify", get(verify_email))
        .route(
            "/api/v1/auth/resend-verification",
            post(resend_verification),
        )
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
}

fn generate_random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
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

fn clear_cookie(name: &str, path: &str) -> Cookie<'static> {
    Cookie::build((name.to_string(), String::new()))
        .path(path.to_string())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build()
}

async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format!("Validation error: {e}")))?;

    if !payload.privacy_consent {
        return Err(AppError::BadRequest(
            "Privacy consent is required".to_string(),
        ));
    }

    if payload.password != payload.password_confirmation {
        return Err(AppError::BadRequest("Passwords do not match".to_string()));
    }

    // Check password strength with zxcvbn
    let entropy = zxcvbn::zxcvbn(&payload.password, &[&payload.email, &payload.display_name]);
    if entropy.score() < zxcvbn::Score::Two {
        return Err(AppError::BadRequest(
            "Password is too weak. Please choose a stronger password.".to_string(),
        ));
    }

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
    if user_created {
        if let Err(e) = state
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
    }

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            message: "Registration successful. Please check your email to verify your account."
                .to_string(),
        }),
    ))
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

    tracing::info!("Email verified for user: {}", user.email);
    Redirect::to(&redirect_ok).into_response()
}

async fn resend_verification(
    State(state): State<AppState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format!("Validation error: {e}")))?;

    // Always return success to prevent user enumeration
    if let Ok(Some(user)) = users::find_user_by_email(&state.inner.pool, &payload.email).await {
        if !user.email_verified {
            let token = generate_random_token();
            let token_hash = hash_token(&token);
            let expires_at = Utc::now().naive_utc() + chrono::Duration::hours(24);

            if let Err(e) = users::update_verification_token(
                &state.inner.pool,
                user.id,
                &token_hash,
                expires_at,
            )
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
    }

    Ok(Json(MessageResponse {
        message: "If an account with this email exists and is not yet verified, a new verification email has been sent.".to_string(),
    }))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format!("Validation error: {e}")))?;

    let user = users::find_user_by_email(&state.inner.pool, &payload.email)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid email or password".to_string()))?;

    let password_hash = user.password_hash.as_ref().ok_or_else(|| {
        tracing::warn!("Login attempt for OAuth-only account: {}", payload.email);
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

    // Create access token
    let access_token = jwt::create_token(
        user.id,
        &user.display_name,
        &state.inner.jwt_secret,
        state.inner.jwt_access_expiry,
    )?;

    // Create refresh token
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
    )
    .await?;

    // Set cookies
    let access_cookie = build_cookie(
        "access_token",
        access_token,
        "/api",
        state.inner.jwt_access_expiry.cast_signed(),
        state.inner.cookie_secure,
    );

    let refresh_cookie = build_cookie(
        "refresh_token",
        raw_refresh_token,
        "/api/v1/auth",
        state.inner.jwt_refresh_expiry.cast_signed(),
        state.inner.cookie_secure,
    );

    let jar = jar.add(access_cookie).add(refresh_cookie);

    Ok((
        jar,
        Json(LoginResponse {
            message: "Login successful".to_string(),
        }),
    ))
}

async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<MessageResponse>), AppError> {
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

    // Issue new access token
    let new_access_token = jwt::create_token(
        user.id,
        &user.display_name,
        &state.inner.jwt_secret,
        state.inner.jwt_access_expiry,
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
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
