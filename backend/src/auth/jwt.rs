use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: u32,
    pub name: String,
    pub role: String,
    pub exp: usize,
    pub iat: usize,
}

pub fn create_token(
    user_id: u32,
    display_name: &str,
    role: &str,
    secret: &str,
    expiry_seconds: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id,
        name: display_name.to_string(),
        role: role.to_string(),
        #[allow(clippy::cast_possible_truncation)]
        exp: now + expiry_seconds as usize,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

#[allow(dead_code)]
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_validate_token() {
        let secret = "test-secret-key";
        let token =
            create_token(1, "TestUser", "user", secret, 3600).expect("Failed to create token");

        let claims = validate_token(&token, secret).expect("Failed to validate token");
        assert_eq!(claims.sub, 1);
        assert_eq!(claims.name, "TestUser");
        assert_eq!(claims.role, "user");
    }

    #[test]
    fn test_invalid_secret_fails_validation() {
        let token =
            create_token(1, "TestUser", "admin", "secret1", 3600).expect("Failed to create token");
        let result = validate_token(&token, "wrong-secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_expired_token_fails_validation() {
        let secret = "test-secret-key";
        // Manually create a token that expired 120 seconds ago (beyond default leeway)
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = Claims {
            sub: 1,
            name: "TestUser".to_string(),
            role: "user".to_string(),
            exp: now - 120,
            iat: now - 300,
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("Failed to create token");

        let result = validate_token(&token, secret);
        assert!(result.is_err());
    }
}
