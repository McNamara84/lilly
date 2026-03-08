use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, sqlx::FromRow)]
pub struct User {
    pub id: u32,
    pub email: String,
    pub password_hash: Option<String>,
    pub display_name: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
    #[validate(length(min = 1, message = "Password is required"))]
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}
