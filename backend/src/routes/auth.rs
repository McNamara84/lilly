use axum::{extract::State, routing::post, Json, Router};
use validator::Validate;

use super::AppState;
use crate::auth::{jwt, password};
use crate::db::users;
use crate::error::AppError;
use crate::models::user::{LoginRequest, LoginResponse};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/auth/login", post(login))
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format!("Validation error: {}", e)))?;

    let user = users::find_user_by_email(&state.pool, &payload.email)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid email or password".to_string()))?;

    let password_hash = user
        .password_hash
        .as_ref()
        .ok_or_else(|| AppError::Unauthorized("This account uses OAuth login".to_string()))?;

    let valid = password::verify_password(&payload.password, password_hash)
        .map_err(|_| AppError::Unauthorized("Invalid email or password".to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized(
            "Invalid email or password".to_string(),
        ));
    }

    let access_token = jwt::create_token(
        user.id,
        &user.display_name,
        &state.jwt_secret,
        state.jwt_access_expiry,
    )?;

    Ok(Json(LoginResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: state.jwt_access_expiry,
    }))
}
