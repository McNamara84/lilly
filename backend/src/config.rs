pub struct E2eConfig {
    pub demo_seed_enabled: bool,
    pub worker_count: u16,
    pub fixture_adapter_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitRule {
    pub max_requests: usize,
    pub window_seconds: u64,
}

impl RateLimitRule {
    const fn new(max_requests: usize, window_seconds: u64) -> Self {
        Self {
            max_requests,
            window_seconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    pub register: RateLimitRule,
    pub login_client: RateLimitRule,
    pub login_account: RateLimitRule,
    pub resend_verification: RateLimitRule,
    pub password_reset_request: RateLimitRule,
    pub password_reset_confirm: RateLimitRule,
    pub oauth_start: RateLimitRule,
    pub oauth_callback: RateLimitRule,
    pub refresh: RateLimitRule,
    pub public_api: RateLimitRule,
    pub authenticated_api: RateLimitRule,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            register: RateLimitRule::new(5, 15 * 60),
            login_client: RateLimitRule::new(30, 15 * 60),
            login_account: RateLimitRule::new(10, 15 * 60),
            resend_verification: RateLimitRule::new(5, 15 * 60),
            password_reset_request: RateLimitRule::new(5, 15 * 60),
            password_reset_confirm: RateLimitRule::new(10, 15 * 60),
            oauth_start: RateLimitRule::new(10, 60),
            oauth_callback: RateLimitRule::new(30, 5 * 60),
            refresh: RateLimitRule::new(60, 60),
            public_api: RateLimitRule::new(120, 60),
            authenticated_api: RateLimitRule::new(600, 60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTlsMode {
    StartTls,
    Tls,
    None,
}

impl SmtpTlsMode {
    fn from_env(value: Option<&str>) -> Self {
        match value
            .unwrap_or("starttls")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "starttls" => Self::StartTls,
            "tls" => Self::Tls,
            "none" => Self::None,
            _ => panic!("SMTP_TLS_MODE must be starttls, tls, or none"),
        }
    }
}

#[derive(Clone)]
pub struct OAuthCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone)]
pub struct PhotoUploadConfig {
    pub max_upload_bytes: usize,
    pub max_count: u8,
    pub max_edge: u32,
    pub max_source_dimension: u32,
    pub max_source_pixels: u64,
    pub jpeg_quality: u8,
}

impl Default for PhotoUploadConfig {
    fn default() -> Self {
        Self {
            max_upload_bytes: 5 * 1024 * 1024,
            max_count: 4,
            max_edge: 2_048,
            max_source_dimension: 10_000,
            max_source_pixels: 40_000_000,
            jpeg_quality: 85,
        }
    }
}

pub struct AppConfig {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_access_token_expiry: u64,
    pub jwt_refresh_token_expiry: u64,
    pub password_reset_ttl_seconds: u64,
    pub backend_port: u16,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_tls_mode: SmtpTlsMode,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from: String,
    pub app_base_url: String,
    pub cookie_secure: bool,
    pub google_oauth: Option<OAuthCredentials>,
    pub github_oauth: Option<OAuthCredentials>,
    pub privacy_policy_version: String,
    pub admin_email: Option<String>,
    pub media_path: String,
    pub media_url_prefix: String,
    pub photo_upload: PhotoUploadConfig,
    pub e2e: E2eConfig,
    pub import_scheduler_enabled: bool,
    pub import_schedule: String,
    pub import_timezone: String,
    pub import_scheduled_adapters: Vec<String>,
    pub trusted_proxy_cidrs: Vec<ipnet::IpNet>,
    pub rate_limits: RateLimitConfig,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    #[allow(clippy::too_many_lines)]
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

        let photo_upload = photo_upload_config(&get);
        let google_oauth = oauth_credentials(&get, "GOOGLE");
        let github_oauth = oauth_credentials(&get, "GITHUB");
        let privacy_policy_version = get("PRIVACY_POLICY_VERSION")
            .unwrap_or_else(|| "2026-03-06".to_string())
            .trim()
            .to_string();
        assert!(
            !privacy_policy_version.is_empty() && privacy_policy_version.len() <= 64,
            "PRIVACY_POLICY_VERSION must contain 1 to 64 bytes"
        );
        let app_base_url = get("APP_BASE_URL").unwrap_or_else(|| "http://localhost".to_string());
        let app_base_url = validate_app_base_url(&app_base_url);
        let rate_limits = rate_limit_config(&get);
        let trusted_proxy_cidrs = trusted_proxy_cidrs(&get);
        let password_reset_ttl_seconds = get("PASSWORD_RESET_TTL_SECONDS")
            .unwrap_or_else(|| "3600".to_string())
            .parse()
            .expect("PASSWORD_RESET_TTL_SECONDS must be a number");
        assert!(
            (300..=86_400).contains(&password_reset_ttl_seconds),
            "PASSWORD_RESET_TTL_SECONDS must be between 300 and 86400"
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
            password_reset_ttl_seconds,
            backend_port: get("BACKEND_PORT")
                .unwrap_or_else(|| "8080".to_string())
                .parse()
                .expect("BACKEND_PORT must be a number"),
            smtp_host: get("SMTP_HOST").filter(|s| !s.is_empty()),
            smtp_port: get("SMTP_PORT")
                .unwrap_or_else(|| "587".to_string())
                .parse()
                .expect("SMTP_PORT must be a number"),
            smtp_tls_mode: SmtpTlsMode::from_env(get("SMTP_TLS_MODE").as_deref()),
            smtp_user: get("SMTP_USER").filter(|s| !s.is_empty()),
            smtp_password: get("SMTP_PASSWORD").filter(|s| !s.is_empty()),
            smtp_from: get("SMTP_FROM").unwrap_or_else(|| "noreply@lilly.app".to_string()),
            app_base_url,
            cookie_secure: get("COOKIE_SECURE")
                .unwrap_or_else(|| "false".to_string())
                .parse()
                .unwrap_or(false),
            google_oauth,
            github_oauth,
            privacy_policy_version,
            admin_email: get("ADMIN_EMAIL")
                .filter(|value| !value.trim().is_empty())
                .map(|value| {
                    crate::models::user::normalize_email(&value)
                        .expect("ADMIN_EMAIL must be a valid email address")
                }),
            media_path: get("MEDIA_PATH").unwrap_or_else(|| "/media".to_string()),
            media_url_prefix: get("MEDIA_URL_PREFIX").unwrap_or_else(|| "/media".to_string()),
            photo_upload,
            e2e: E2eConfig {
                demo_seed_enabled,
                worker_count: e2e_worker_count,
                fixture_adapter_enabled: e2e_fixture_adapter_enabled,
            },
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
            trusted_proxy_cidrs,
            rate_limits,
        }
    }
}

fn rate_limit_config(get: &impl Fn(&str) -> Option<String>) -> RateLimitConfig {
    let defaults = RateLimitConfig::default();
    RateLimitConfig {
        register: rate_limit_rule(get, "RATE_LIMIT_REGISTER", defaults.register),
        login_client: rate_limit_rule(get, "RATE_LIMIT_LOGIN_CLIENT", defaults.login_client),
        login_account: rate_limit_rule(get, "RATE_LIMIT_LOGIN_ACCOUNT", defaults.login_account),
        resend_verification: rate_limit_rule(
            get,
            "RATE_LIMIT_RESEND_VERIFICATION",
            defaults.resend_verification,
        ),
        password_reset_request: rate_limit_rule(
            get,
            "RATE_LIMIT_PASSWORD_RESET_REQUEST",
            defaults.password_reset_request,
        ),
        password_reset_confirm: rate_limit_rule(
            get,
            "RATE_LIMIT_PASSWORD_RESET_CONFIRM",
            defaults.password_reset_confirm,
        ),
        oauth_start: rate_limit_rule(get, "RATE_LIMIT_OAUTH_START", defaults.oauth_start),
        oauth_callback: rate_limit_rule(get, "RATE_LIMIT_OAUTH_CALLBACK", defaults.oauth_callback),
        refresh: rate_limit_rule(get, "RATE_LIMIT_REFRESH", defaults.refresh),
        public_api: rate_limit_rule(get, "RATE_LIMIT_PUBLIC_API", defaults.public_api),
        authenticated_api: rate_limit_rule(
            get,
            "RATE_LIMIT_AUTHENTICATED_API",
            defaults.authenticated_api,
        ),
    }
}

fn rate_limit_rule(
    get: &impl Fn(&str) -> Option<String>,
    key: &str,
    default: RateLimitRule,
) -> RateLimitRule {
    let Some(value) = get(key) else {
        return default;
    };
    let (max_requests, window_seconds) = value
        .trim()
        .split_once('/')
        .unwrap_or_else(|| panic!("{key} must use MAX_REQUESTS/WINDOW_SECONDS"));
    let rule = RateLimitRule {
        max_requests: max_requests
            .parse()
            .unwrap_or_else(|_| panic!("{key} max requests must be a number")),
        window_seconds: window_seconds
            .parse()
            .unwrap_or_else(|_| panic!("{key} window seconds must be a number")),
    };
    assert!(
        rule.max_requests > 0 && rule.window_seconds > 0,
        "{key} values must be greater than zero"
    );
    rule
}

fn trusted_proxy_cidrs(get: &impl Fn(&str) -> Option<String>) -> Vec<ipnet::IpNet> {
    get("TRUSTED_PROXY_CIDRS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse()
                .unwrap_or_else(|_| panic!("TRUSTED_PROXY_CIDRS contains invalid CIDR: {value}"))
        })
        .collect()
}

fn validate_app_base_url(value: &str) -> String {
    let value = value.trim().trim_end_matches('/');
    let url = url::Url::parse(value).expect("APP_BASE_URL must be an absolute HTTP(S) origin");
    assert!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && matches!(url.path(), "" | "/")
            && url.query().is_none()
            && url.fragment().is_none(),
        "APP_BASE_URL must be an absolute HTTP(S) origin without credentials, path, query, or fragment"
    );
    value.to_string()
}

fn oauth_credentials(
    get: &impl Fn(&str) -> Option<String>,
    provider: &str,
) -> Option<OAuthCredentials> {
    let client_id_key = format!("{provider}_OAUTH_CLIENT_ID");
    let client_secret_key = format!("{provider}_OAUTH_CLIENT_SECRET");
    let client_id = get(&client_id_key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let client_secret = get(&client_secret_key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    match (client_id, client_secret) {
        (None, None) => None,
        (Some(client_id), Some(client_secret)) => Some(OAuthCredentials {
            client_id,
            client_secret,
        }),
        _ => panic!("{provider} OAuth client ID and secret must be configured together"),
    }
}

fn photo_upload_config(get: &impl Fn(&str) -> Option<String>) -> PhotoUploadConfig {
    let config = PhotoUploadConfig {
        max_upload_bytes: get("PHOTO_MAX_UPLOAD_BYTES")
            .unwrap_or_else(|| (5 * 1024 * 1024).to_string())
            .parse()
            .expect("PHOTO_MAX_UPLOAD_BYTES must be a number"),
        max_count: get("PHOTO_MAX_COUNT")
            .unwrap_or_else(|| "4".to_string())
            .parse()
            .expect("PHOTO_MAX_COUNT must be a number"),
        max_edge: get("PHOTO_MAX_EDGE")
            .unwrap_or_else(|| "2048".to_string())
            .parse()
            .expect("PHOTO_MAX_EDGE must be a number"),
        max_source_dimension: get("PHOTO_MAX_SOURCE_DIMENSION")
            .unwrap_or_else(|| "10000".to_string())
            .parse()
            .expect("PHOTO_MAX_SOURCE_DIMENSION must be a number"),
        max_source_pixels: get("PHOTO_MAX_SOURCE_PIXELS")
            .unwrap_or_else(|| "40000000".to_string())
            .parse()
            .expect("PHOTO_MAX_SOURCE_PIXELS must be a number"),
        jpeg_quality: get("PHOTO_JPEG_QUALITY")
            .unwrap_or_else(|| "85".to_string())
            .parse()
            .expect("PHOTO_JPEG_QUALITY must be a number"),
    };
    assert!(
        (1..=5 * 1024 * 1024).contains(&config.max_upload_bytes),
        "PHOTO_MAX_UPLOAD_BYTES must be between 1 and 5242880"
    );
    assert!(
        config.max_count == 4,
        "PHOTO_MAX_COUNT must be 4 for the MVP database constraint"
    );
    assert!(
        (1..=4_096).contains(&config.max_edge),
        "PHOTO_MAX_EDGE must be between 1 and 4096"
    );
    assert!(
        config.max_source_dimension >= config.max_edge && config.max_source_dimension <= 20_000,
        "PHOTO_MAX_SOURCE_DIMENSION must be between PHOTO_MAX_EDGE and 20000"
    );
    assert!(
        config.max_source_pixels >= u64::from(config.max_edge).pow(2)
            && config.max_source_pixels <= 100_000_000,
        "PHOTO_MAX_SOURCE_PIXELS must safely cover PHOTO_MAX_EDGE and not exceed 100000000"
    );
    assert!(
        (1..=100).contains(&config.jpeg_quality),
        "PHOTO_JPEG_QUALITY must be between 1 and 100"
    );
    config
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
        assert_eq!(config.password_reset_ttl_seconds, 3_600);
        assert_eq!(config.backend_port, 8080);
        assert!(config.smtp_host.is_none());
        assert_eq!(config.smtp_port, 587);
        assert_eq!(config.smtp_tls_mode, SmtpTlsMode::StartTls);
        assert!(config.smtp_user.is_none());
        assert!(config.smtp_password.is_none());
        assert_eq!(config.smtp_from, "noreply@lilly.app");
        assert_eq!(config.app_base_url, "http://localhost");
        assert!(!config.cookie_secure);
        assert!(config.google_oauth.is_none());
        assert!(config.github_oauth.is_none());
        assert_eq!(config.privacy_policy_version, "2026-03-06");
        assert!(config.admin_email.is_none());
        assert_eq!(config.media_path, "/media");
        assert_eq!(config.media_url_prefix, "/media");
        assert_eq!(config.photo_upload.max_upload_bytes, 5 * 1024 * 1024);
        assert_eq!(config.photo_upload.max_count, 4);
        assert_eq!(config.photo_upload.max_edge, 2_048);
        assert_eq!(config.photo_upload.max_source_dimension, 10_000);
        assert_eq!(config.photo_upload.max_source_pixels, 40_000_000);
        assert_eq!(config.photo_upload.jpeg_quality, 85);
        assert!(!config.e2e.demo_seed_enabled);
        assert_eq!(config.e2e.worker_count, 0);
        assert!(!config.e2e.fixture_adapter_enabled);
        assert!(!config.import_scheduler_enabled);
        assert_eq!(config.import_schedule, "0 10 6 * * Sat *");
        assert_eq!(config.import_timezone, "Europe/Berlin");
        assert_eq!(
            config.import_scheduled_adapters,
            vec!["maddrax", "john-sinclair"]
        );
        assert!(config.trusted_proxy_cidrs.is_empty());
        assert_eq!(config.rate_limits, RateLimitConfig::default());
    }

    #[test]
    fn security_limits_and_trusted_proxies_are_configurable() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "PASSWORD_RESET_TTL_SECONDS" => Some("1800".to_string()),
            "RATE_LIMIT_LOGIN_CLIENT" => Some("7/120".to_string()),
            "TRUSTED_PROXY_CIDRS" => Some("10.0.0.0/8, 2001:db8::/32".to_string()),
            _ => None,
        });

        assert_eq!(config.password_reset_ttl_seconds, 1_800);
        assert_eq!(config.rate_limits.login_client, RateLimitRule::new(7, 120));
        assert_eq!(config.trusted_proxy_cidrs.len(), 2);
        assert!(
            config.trusted_proxy_cidrs[0]
                .contains(&"10.2.3.4".parse::<std::net::IpAddr>().unwrap())
        );
        assert!(
            config.trusted_proxy_cidrs[1]
                .contains(&"2001:db8::42".parse::<std::net::IpAddr>().unwrap())
        );
    }

    #[test]
    #[should_panic(expected = "RATE_LIMIT_LOGIN_CLIENT must use MAX_REQUESTS/WINDOW_SECONDS")]
    fn malformed_rate_limit_fails_configuration() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "RATE_LIMIT_LOGIN_CLIENT" => Some("7".to_string()),
            _ => None,
        });
    }

    #[test]
    #[should_panic(expected = "TRUSTED_PROXY_CIDRS contains invalid CIDR")]
    fn malformed_trusted_proxy_fails_configuration() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "TRUSTED_PROXY_CIDRS" => Some("not-a-network".to_string()),
            _ => None,
        });
    }

    #[test]
    fn smtp_tls_mode_supports_implicit_tls() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "SMTP_TLS_MODE" => Some(" TLS ".to_string()),
            _ => None,
        });

        assert_eq!(config.smtp_tls_mode, SmtpTlsMode::Tls);
    }

    #[test]
    fn smtp_tls_mode_supports_explicit_test_only_plaintext() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "SMTP_TLS_MODE" => Some("none".to_string()),
            _ => None,
        });

        assert_eq!(config.smtp_tls_mode, SmtpTlsMode::None);
    }

    #[test]
    #[should_panic(expected = "SMTP_TLS_MODE must be starttls, tls, or none")]
    fn invalid_smtp_tls_mode_fails_configuration() {
        AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "SMTP_TLS_MODE" => Some("opportunistic".to_string()),
            _ => None,
        });
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
    fn admin_email_is_normalized() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "ADMIN_EMAIL" => Some("  First.Admin@Example.COM ".to_string()),
            _ => None,
        });
        assert_eq!(
            config.admin_email.as_deref(),
            Some("first.admin@example.com")
        );
    }

    #[test]
    #[should_panic(expected = "ADMIN_EMAIL must be a valid email address")]
    fn invalid_admin_email_fails_configuration() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "ADMIN_EMAIL" => Some("not-an-email".to_string()),
            _ => None,
        });
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
        assert!(config.e2e.demo_seed_enabled);
        assert_eq!(config.e2e.worker_count, 4);
        assert!(config.e2e.fixture_adapter_enabled);
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

    #[test]
    fn photo_upload_configuration_can_be_restricted() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "PHOTO_MAX_UPLOAD_BYTES" => Some("1048576".to_string()),
            "PHOTO_MAX_EDGE" => Some("1024".to_string()),
            "PHOTO_MAX_SOURCE_DIMENSION" => Some("8000".to_string()),
            "PHOTO_MAX_SOURCE_PIXELS" => Some("20000000".to_string()),
            "PHOTO_JPEG_QUALITY" => Some("80".to_string()),
            _ => None,
        });

        assert_eq!(config.photo_upload.max_upload_bytes, 1_048_576);
        assert_eq!(config.photo_upload.max_edge, 1_024);
        assert_eq!(config.photo_upload.max_source_dimension, 8_000);
        assert_eq!(config.photo_upload.max_source_pixels, 20_000_000);
        assert_eq!(config.photo_upload.jpeg_quality, 80);
    }

    #[test]
    #[should_panic(expected = "PHOTO_MAX_COUNT must be 4")]
    fn photo_count_outside_mvp_constraint_fails_configuration() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "PHOTO_MAX_COUNT" => Some("5".to_string()),
            _ => None,
        });
    }

    #[test]
    fn oauth_credentials_and_policy_version_are_loaded() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "GOOGLE_OAUTH_CLIENT_ID" => Some(" google-client ".to_string()),
            "GOOGLE_OAUTH_CLIENT_SECRET" => Some("google-secret".to_string()),
            "GITHUB_OAUTH_CLIENT_ID" => Some("github-client".to_string()),
            "GITHUB_OAUTH_CLIENT_SECRET" => Some("github-secret".to_string()),
            "PRIVACY_POLICY_VERSION" => Some(" policy-v2 ".to_string()),
            _ => None,
        });

        let google = config.google_oauth.unwrap();
        assert_eq!(google.client_id, "google-client");
        assert_eq!(google.client_secret, "google-secret");
        assert!(config.github_oauth.is_some());
        assert_eq!(config.privacy_policy_version, "policy-v2");
    }

    #[test]
    #[should_panic(expected = "GOOGLE OAuth client ID and secret must be configured together")]
    fn incomplete_oauth_credentials_fail_configuration() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "GOOGLE_OAUTH_CLIENT_ID" => Some("client-only".to_string()),
            _ => None,
        });
    }

    #[test]
    #[should_panic(expected = "PRIVACY_POLICY_VERSION must contain 1 to 64 bytes")]
    fn empty_policy_version_fails_configuration() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "PRIVACY_POLICY_VERSION" => Some("   ".to_string()),
            _ => None,
        });
    }

    #[test]
    fn app_base_url_is_trimmed_and_restricted_to_an_origin() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "APP_BASE_URL" => Some("  https://lilly.example/  ".to_string()),
            _ => None,
        });
        assert_eq!(config.app_base_url, "https://lilly.example");
    }

    #[test]
    #[should_panic(expected = "without credentials, path, query, or fragment")]
    fn app_base_url_with_path_fails_configuration() {
        let _ = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("mysql://test:test@localhost/test".to_string()),
            "JWT_SECRET" => Some("test-secret".to_string()),
            "APP_BASE_URL" => Some("https://lilly.example/untrusted".to_string()),
            _ => None,
        });
    }
}
