use std::path::PathBuf;
use std::sync::Arc;

use lilly_importer_core::adapter::AdapterRegistry;

use crate::services::email::EmailService;

pub mod admin;
pub mod auth;
pub mod collection;
pub mod health;
pub mod series;

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
    pub adapter_registry: AdapterRegistry,
    pub media_path: PathBuf,
    pub media_url_prefix: String,
}
