use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Validate)]
struct EmailAddress {
    #[validate(email(message = "Invalid email format"))]
    email: String,
}

/// Normalize an email address consistently across authentication and admin tooling.
pub fn normalize_email(value: &str) -> Result<String, String> {
    let email = value.trim().to_lowercase();
    EmailAddress {
        email: email.clone(),
    }
    .validate()
    .map_err(|_| "Invalid email format".to_string())?;
    Ok(email)
}

#[derive(Debug, sqlx::FromRow)]
pub struct User {
    pub id: u32,
    pub email: String,
    pub password_hash: Option<String>,
    pub display_name: String,
    pub role: String,
    pub email_verified: bool,
    pub account_state: String,
    pub session_version: u32,
    #[allow(dead_code)]
    pub erasure_subject: String,
}

impl User {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.account_state == "active"
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
    #[validate(length(min = 1, max = 128, message = "Password must be 1–128 characters"))]
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_for: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 2, max = 100, message = "Display name must be 2–100 characters"))]
    pub display_name: String,
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
    #[validate(length(min = 8, max = 128, message = "Password must be 8–128 characters"))]
    pub password: String,
    pub password_confirmation: String,
    pub privacy_consent: bool,
    #[validate(length(min = 1, max = 64, message = "Privacy policy version is required"))]
    pub privacy_policy_version: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: u32,
    pub email: String,
    pub display_name: String,
    pub email_verified: bool,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResendVerificationRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PasswordResetRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PasswordResetConfirmRequest {
    #[validate(length(min = 43, max = 128, message = "Reset token is invalid"))]
    pub token: String,
    #[validate(length(min = 8, max = 128, message = "Password must be 8–128 characters"))]
    pub password: String,
    pub password_confirmation: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_login_request_valid() {
        let req = LoginRequest {
            email: "user@example.com".to_string(),
            password: "secret123".to_string(),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn email_normalization_trims_lowercases_and_validates() {
        assert_eq!(
            normalize_email("  Admin.User@Example.COM ").unwrap(),
            "admin.user@example.com"
        );
        assert_eq!(
            normalize_email("invalid"),
            Err("Invalid email format".to_string())
        );
        assert_eq!(
            normalize_email("   "),
            Err("Invalid email format".to_string())
        );
    }

    #[test]
    fn test_login_request_invalid_email() {
        let req = LoginRequest {
            email: "not-an-email".to_string(),
            password: "secret123".to_string(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_login_request_empty_password() {
        let req = LoginRequest {
            email: "user@example.com".to_string(),
            password: String::new(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_register_request_valid() {
        let req = RegisterRequest {
            display_name: "Max Mustermann".to_string(),
            email: "max@example.com".to_string(),
            password: "strongpass123".to_string(),
            password_confirmation: "strongpass123".to_string(),
            privacy_consent: true,
            privacy_policy_version: "test-v1".to_string(),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn password_reset_requests_validate_email_token_and_password_bounds() {
        assert!(
            PasswordResetRequest {
                email: "collector@example.com".to_string()
            }
            .validate()
            .is_ok()
        );
        assert!(
            PasswordResetRequest {
                email: "invalid".to_string()
            }
            .validate()
            .is_err()
        );
        let valid = PasswordResetConfirmRequest {
            token: "a".repeat(43),
            password: "strong-password".to_string(),
            password_confirmation: "strong-password".to_string(),
        };
        assert!(valid.validate().is_ok());
        assert!(
            PasswordResetConfirmRequest {
                token: "short".to_string(),
                password: "short".to_string(),
                password_confirmation: "short".to_string(),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn test_register_request_short_display_name() {
        let req = RegisterRequest {
            display_name: "M".to_string(),
            email: "max@example.com".to_string(),
            password: "strongpass123".to_string(),
            password_confirmation: "strongpass123".to_string(),
            privacy_consent: true,
            privacy_policy_version: "test-v1".to_string(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_register_request_invalid_email() {
        let req = RegisterRequest {
            display_name: "Max Mustermann".to_string(),
            email: "invalid".to_string(),
            password: "strongpass123".to_string(),
            password_confirmation: "strongpass123".to_string(),
            privacy_consent: true,
            privacy_policy_version: "test-v1".to_string(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_register_request_short_password() {
        let req = RegisterRequest {
            display_name: "Max Mustermann".to_string(),
            email: "max@example.com".to_string(),
            password: "short".to_string(),
            password_confirmation: "short".to_string(),
            privacy_consent: true,
            privacy_policy_version: "test-v1".to_string(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_resend_verification_valid() {
        let req = ResendVerificationRequest {
            email: "user@example.com".to_string(),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_resend_verification_invalid_email() {
        let req = ResendVerificationRequest {
            email: "invalid".to_string(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_error_response_serialization_without_code() {
        let resp = ErrorResponse {
            error: "Something went wrong".to_string(),
            code: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"], "Something went wrong");
        assert!(json.get("code").is_none());
    }

    #[test]
    fn test_error_response_serialization_with_code() {
        let resp = ErrorResponse {
            error: "Email not verified".to_string(),
            code: Some("EMAIL_NOT_VERIFIED".to_string()),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"], "Email not verified");
        assert_eq!(json["code"], "EMAIL_NOT_VERIFIED");
    }

    #[test]
    fn test_me_response_serialization() {
        let resp = MeResponse {
            id: 1,
            email: "user@example.com".to_string(),
            display_name: "Max".to_string(),
            email_verified: true,
            role: "user".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["email"], "user@example.com");
        assert_eq!(json["display_name"], "Max");
        assert_eq!(json["email_verified"], true);
        assert_eq!(json["role"], "user");
    }
}
