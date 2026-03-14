use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{message}")]
    Forbidden {
        message: String,
        code: Option<String>,
    },

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
        let (status, message, code) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone(), None),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone(), None),
            AppError::Forbidden { message, code } => {
                (StatusCode::FORBIDDEN, message.clone(), code.clone())
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone(), None),
            AppError::InternalError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
                None,
            ),
        };

        let body = if let Some(code) = code {
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
