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
    pub media_url_prefix: String,
    pub demo_seed_enabled: bool,
    pub e2e_worker_count: u16,
    pub e2e_fixture_adapter_enabled: bool,
    pub import_scheduler_enabled: bool,
    pub import_schedule: String,
    pub import_timezone: String,
    pub import_scheduled_adapters: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let demo_seed_enabled: bool = get("ENABLE_DEMO_SEED")
            .unwrap_or_else(|| "false".to_string())
            .parse()
            .expect("ENABLE_DEMO_SEED must be true or false");
        let e2e_worker_count: u16 = get("E2E_WORKER_COUNT")
            .unwrap_or_else(|| "0".to_string())
            .parse()
            .expect("E2E_WORKER_COUNT must be a number");
        assert!(
            e2e_worker_count <= 16,
            "E2E_WORKER_COUNT must not exceed 16"
        );
        let e2e_fixture_adapter_enabled: bool = get("ENABLE_E2E_FIXTURE_ADAPTER")
            .unwrap_or_else(|| "false".to_string())
            .parse()
            .expect("ENABLE_E2E_FIXTURE_ADAPTER must be true or false");
        assert!(
            !e2e_fixture_adapter_enabled || demo_seed_enabled,
            "ENABLE_E2E_FIXTURE_ADAPTER requires ENABLE_DEMO_SEED=true"
        );

        Self {
            database_url: get("DATABASE_URL").expect("DATABASE_URL must be set"),
            jwt_secret: get("JWT_SECRET").expect("JWT_SECRET must be set"),
            jwt_access_token_expiry: get("JWT_ACCESS_TOKEN_EXPIRY")
                .unwrap_or_else(|| "900".to_string())
                .parse()
                .expect("JWT_ACCESS_TOKEN_EXPIRY must be a number"),
            jwt_refresh_token_expiry: get("JWT_REFRESH_TOKEN_EXPIRY")
                .unwrap_or_else(|| "2592000".to_string())
                .parse()
                .expect("JWT_REFRESH_TOKEN_EXPIRY must be a number"),
            backend_port: get("BACKEND_PORT")
                .unwrap_or_else(|| "8080".to_string())
                .parse()
                .expect("BACKEND_PORT must be a number"),
            smtp_host: get("SMTP_HOST").filter(|s| !s.is_empty()),
            smtp_port: get("SMTP_PORT")
                .unwrap_or_else(|| "587".to_string())
                .parse()
                .expect("SMTP_PORT must be a number"),
            smtp_user: get("SMTP_USER").filter(|s| !s.is_empty()),
            smtp_password: get("SMTP_PASSWORD").filter(|s| !s.is_empty()),
            smtp_from: get("SMTP_FROM").unwrap_or_else(|| "noreply@lilly.app".to_string()),
            app_base_url: get("APP_BASE_URL").unwrap_or_else(|| "http://localhost".to_string()),
            cookie_secure: get("COOKIE_SECURE")
                .unwrap_or_else(|| "false".to_string())
                .parse()
                .unwrap_or(false),
            admin_email: get("ADMIN_EMAIL").filter(|s| !s.is_empty()),
            media_path: get("MEDIA_PATH").unwrap_or_else(|| "/media".to_string()),
            media_url_prefix: get("MEDIA_URL_PREFIX").unwrap_or_else(|| "/media".to_string()),
            demo_seed_enabled,
            e2e_worker_count,
            e2e_fixture_adapter_enabled,
            import_scheduler_enabled: get("IMPORT_SCHEDULER_ENABLED")
                .unwrap_or_else(|| "false".to_string())
                .parse()
                .expect("IMPORT_SCHEDULER_ENABLED must be true or false"),
            import_schedule: get("IMPORT_SCHEDULE")
                .unwrap_or_else(|| "0 10 6 * * Sat *".to_string()),
            import_timezone: get("IMPORT_TIMEZONE").unwrap_or_else(|| "Europe/Berlin".to_string()),
            import_scheduled_adapters: get("IMPORT_SCHEDULED_ADAPTERS")
                .unwrap_or_else(|| "maddrax,john-sinclair".to_string())
                .split(',')
                .map(str::trim)
                .filter(|adapter| !adapter.is_empty())
                .map(ToString::to_string)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_env_with_defaults() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            _ => None,
        });
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
        assert_eq!(config.media_url_prefix, "/media");
        assert!(!config.demo_seed_enabled);
        assert_eq!(config.e2e_worker_count, 0);
        assert!(!config.e2e_fixture_adapter_enabled);
        assert!(!config.import_scheduler_enabled);
        assert_eq!(config.import_schedule, "0 10 6 * * Sat *");
        assert_eq!(config.import_timezone, "Europe/Berlin");
        assert_eq!(
            config.import_scheduled_adapters,
            vec!["maddrax", "john-sinclair"]
        );
    }

    #[test]
    fn test_import_scheduler_configuration() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "IMPORT_SCHEDULER_ENABLED" => Some("true".to_string()),
            "IMPORT_SCHEDULE" => Some("0 10 6 * * Sat *".to_string()),
            "IMPORT_TIMEZONE" => Some("Europe/Berlin".to_string()),
            "IMPORT_SCHEDULED_ADAPTERS" => Some("maddrax, john-sinclair, ".to_string()),
            _ => None,
        });
        assert!(config.import_scheduler_enabled);
        assert_eq!(
            config.import_scheduled_adapters,
            vec!["maddrax", "john-sinclair"]
        );
    }

    #[test]
    fn test_e2e_configuration() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "ENABLE_DEMO_SEED" | "ENABLE_E2E_FIXTURE_ADAPTER" => Some("true".to_string()),
            "E2E_WORKER_COUNT" => Some("4".to_string()),
            _ => None,
        });
        assert!(config.demo_seed_enabled);
        assert_eq!(config.e2e_worker_count, 4);
        assert!(config.e2e_fixture_adapter_enabled);
    }

    #[test]
    #[should_panic(expected = "requires ENABLE_DEMO_SEED=true")]
    fn test_e2e_fixture_adapter_requires_demo_seed() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "ENABLE_E2E_FIXTURE_ADAPTER" => Some("true".to_string()),
            _ => None,
        });
    }
}
