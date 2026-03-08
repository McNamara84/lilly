pub mod auth;
pub mod health;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::MySqlPool,
    pub jwt_secret: String,
    pub jwt_access_expiry: u64,
    #[allow(dead_code)]
    pub jwt_refresh_expiry: u64,
}
