pub struct AppConfig {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_access_token_expiry: u64,
    pub jwt_refresh_token_expiry: u64,
    pub backend_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            jwt_secret: std::env::var("JWT_SECRET")
                .expect("JWT_SECRET must be set"),
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

        let config = AppConfig::from_env();
        assert_eq!(config.jwt_access_token_expiry, 900);
        assert_eq!(config.jwt_refresh_token_expiry, 2592000);
        assert_eq!(config.backend_port, 8080);
    }
}
