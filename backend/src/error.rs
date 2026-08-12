use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("Validation failed")]
    Validation { fields: BTreeMap<String, String> },

    #[error("{0}")]
    PayloadTooLarge(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{message}")]
    ConflictWithCode { message: String, code: String },

    #[error("{0}")]
    Unauthorized(String),

    #[error("{message}")]
    Forbidden {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    TooManyRequests { message: String, code: String },

    #[error("{0}")]
    #[allow(dead_code)]
    NotFound(String),

    #[error("Internal server error")]
    InternalError(#[source] anyhow::Error),
}

// Manual From implementation to wrap any error via anyhow
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!("Database error: {:?}", err);
        AppError::InternalError(err.into())
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        use jsonwebtoken::errors::ErrorKind;
        match err.kind() {
            ErrorKind::InvalidToken
            | ErrorKind::InvalidSignature
            | ErrorKind::ExpiredSignature
            | ErrorKind::InvalidAudience
            | ErrorKind::InvalidIssuer
            | ErrorKind::ImmatureSignature => {
                tracing::warn!("JWT validation error: {:?}", err);
                AppError::Unauthorized("Invalid token".to_string())
            }
            _ => {
                tracing::error!("JWT error: {:?}", err);
                AppError::InternalError(err.into())
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message, code, fields) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone(), None, None),
            AppError::Validation { fields } => (
                StatusCode::BAD_REQUEST,
                "Validation failed".to_string(),
                None,
                Some(fields.clone()),
            ),
            AppError::PayloadTooLarge(msg) => {
                (StatusCode::PAYLOAD_TOO_LARGE, msg.clone(), None, None)
            }
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone(), None, None),
            AppError::ConflictWithCode { message, code } => (
                StatusCode::CONFLICT,
                message.clone(),
                Some(code.clone()),
                None,
            ),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone(), None, None),
            AppError::Forbidden { message, code } => {
                (StatusCode::FORBIDDEN, message.clone(), code.clone(), None)
            }
            AppError::TooManyRequests { message, code } => (
                StatusCode::TOO_MANY_REQUESTS,
                message.clone(),
                Some(code.clone()),
                None,
            ),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone(), None, None),
            AppError::InternalError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
                None,
                None,
            ),
        };

        let body = if let Some(fields) = fields {
            json!({ "error": message, "fields": fields })
        } else if let Some(code) = code {
            json!({ "error": message, "code": code })
        } else {
            json!({ "error": message })
        };
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_bad_request_status() {
        let error = AppError::BadRequest("invalid input".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validation_error_contains_field_messages() {
        let error = AppError::Validation {
            fields: BTreeMap::from([("email".to_string(), "Invalid email".to_string())]),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_conflict_status() {
        let error = AppError::Conflict("invalid state transition".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_payload_too_large_status() {
        let error = AppError::PayloadTooLarge("too large".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn test_conflict_with_code_status() {
        let error = AppError::ConflictWithCode {
            message: "review required".to_string(),
            code: "review_required".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_unauthorized_status() {
        let error = AppError::Unauthorized("not logged in".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_forbidden_status() {
        let error = AppError::Forbidden {
            message: "access denied".to_string(),
            code: None,
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_forbidden_with_code_includes_code_in_response() {
        let error = AppError::Forbidden {
            message: "Email not verified".to_string(),
            code: Some("EMAIL_NOT_VERIFIED".to_string()),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn rate_limit_error_uses_too_many_requests_status() {
        let error = AppError::TooManyRequests {
            message: "Too many attempts".to_string(),
            code: "RATE_LIMITED".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_not_found_status() {
        let error = AppError::NotFound("resource missing".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_internal_error_status() {
        let error = AppError::InternalError(anyhow::anyhow!("something broke"));
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
