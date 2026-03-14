use std::sync::Arc;

use crate::services::email::EmailService;

pub mod auth;
pub mod health;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub pool: sqlx::MySqlPool,
    pub jwt_secret: String,
    pub jwt_access_expiry: u64,
    pub jwt_refresh_expiry: u64,
    pub email_service: EmailService,
    pub app_base_url: String,
    pub cookie_secure: bool,
}
