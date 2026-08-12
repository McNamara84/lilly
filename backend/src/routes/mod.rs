use std::path::PathBuf;
use std::sync::Arc;

use lilly_importer_core::adapter::AdapterRegistry;

use crate::config::PhotoUploadConfig;
use crate::services::email::EmailService;
use crate::services::import_scheduler::ImportSchedulerConfig;
use crate::services::media::MediaStorage;
use crate::services::oauth::OAuthService;

pub mod admin;
pub mod auth;
pub mod collection;
pub mod health;
pub mod media;
pub mod messages;
pub mod notifications;
pub mod oauth;
pub mod profiles;
pub mod series;
pub mod trades;

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
    pub oauth_service: OAuthService,
    pub privacy_policy_version: String,
    pub adapter_registry: AdapterRegistry,
    pub media_path: PathBuf,
    pub media_url_prefix: String,
    pub photo_upload_config: PhotoUploadConfig,
    pub media_storage: MediaStorage,
    pub import_scheduler_config: ImportSchedulerConfig,
}
