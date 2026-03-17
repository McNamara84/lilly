pub struct AppConfig {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_access_token_expiry: u64,
    pub jwt_refresh_token_expiry: u64,
    pub backend_port: u16,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from: String,
    pub app_base_url: String,
    pub cookie_secure: bool,
    pub admin_email: Option<String>,
    pub media_path: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set"),
            jwt_access_token_expiry: std::env::var("JWT_ACCESS_TOKEN_EXPIRY")
                .unwrap_or_else(|_| "900".to_string())
                .parse()
                .expect("JWT_ACCESS_TOKEN_EXPIRY must be a number"),
            jwt_refresh_token_expiry: std::env::var("JWT_REFRESH_TOKEN_EXPIRY")
                .unwrap_or_else(|_| "2592000".to_string())
                .parse()
                .expect("JWT_REFRESH_TOKEN_EXPIRY must be a number"),
            backend_port: std::env::var("BACKEND_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("BACKEND_PORT must be a number"),
            smtp_host: std::env::var("SMTP_HOST").ok().filter(|s| !s.is_empty()),
            smtp_port: std::env::var("SMTP_PORT")
                .unwrap_or_else(|_| "587".to_string())
                .parse()
                .expect("SMTP_PORT must be a number"),
            smtp_user: std::env::var("SMTP_USER").ok().filter(|s| !s.is_empty()),
            smtp_password: std::env::var("SMTP_PASSWORD")
                .ok()
                .filter(|s| !s.is_empty()),
            smtp_from: std::env::var("SMTP_FROM")
                .unwrap_or_else(|_| "noreply@lilly.app".to_string()),
            app_base_url: std::env::var("APP_BASE_URL")
                .unwrap_or_else(|_| "http://localhost".to_string()),
            cookie_secure: std::env::var("COOKIE_SECURE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            admin_email: std::env::var("ADMIN_EMAIL").ok().filter(|s| !s.is_empty()),
            media_path: std::env::var("MEDIA_PATH").unwrap_or_else(|_| "/media".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_env_with_defaults() {
        std::env::set_var("DATABASE_URL", "mysql://test:test@localhost/test");
        std::env::set_var("JWT_SECRET", "test-secret");
        std::env::remove_var("JWT_ACCESS_TOKEN_EXPIRY");
        std::env::remove_var("JWT_REFRESH_TOKEN_EXPIRY");
        std::env::remove_var("BACKEND_PORT");
        std::env::remove_var("SMTP_HOST");
        std::env::remove_var("SMTP_PORT");
        std::env::remove_var("SMTP_USER");
        std::env::remove_var("SMTP_PASSWORD");
        std::env::remove_var("SMTP_FROM");
        std::env::remove_var("APP_BASE_URL");
        std::env::remove_var("COOKIE_SECURE");
        std::env::remove_var("ADMIN_EMAIL");
        std::env::remove_var("MEDIA_PATH");

        let config = AppConfig::from_env();
        assert_eq!(config.jwt_access_token_expiry, 900);
        assert_eq!(config.jwt_refresh_token_expiry, 2_592_000);
        assert_eq!(config.backend_port, 8080);
        assert!(config.smtp_host.is_none());
        assert_eq!(config.smtp_port, 587);
        assert!(config.smtp_user.is_none());
        assert!(config.smtp_password.is_none());
        assert_eq!(config.smtp_from, "noreply@lilly.app");
        assert_eq!(config.app_base_url, "http://localhost");
        assert!(!config.cookie_secure);
        assert!(config.admin_email.is_none());
        assert_eq!(config.media_path, "/media");
    }
}
