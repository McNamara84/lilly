use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;

use crate::auth::jwt;
use crate::error::AppError;
use crate::routes::AppState;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: u32,
    #[allow(dead_code)]
    pub display_name: String,
    #[allow(dead_code)]
    pub role: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized("Missing authentication".to_string()))?;

        let access_token = jar
            .get("access_token")
            .map(|c| c.value().to_string())
            .ok_or_else(|| AppError::Unauthorized("Missing authentication".to_string()))?;

        let claims = jwt::validate_token(&access_token, &state.inner.jwt_secret)
            .map_err(|_| AppError::Unauthorized("Invalid or expired token".to_string()))?;

        Ok(Self {
            user_id: claims.sub,
            display_name: claims.name,
            role: claims.role,
        })
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminUser(pub AuthUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_user = AuthUser::from_request_parts(parts, state).await?;

        if auth_user.role != "admin" {
            return Err(AppError::Forbidden {
                message: "Admin access required".to_string(),
                code: Some("ADMIN_REQUIRED".to_string()),
            });
        }

        Ok(Self(auth_user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_user_debug() {
        let user = AuthUser {
            user_id: 1,
            display_name: "Test".to_string(),
            role: "user".to_string(),
        };
        assert_eq!(user.user_id, 1);
        assert_eq!(user.display_name, "Test");
        assert_eq!(user.role, "user");
    }

    #[test]
    fn test_admin_user_wraps_auth_user() {
        let auth_user = AuthUser {
            user_id: 42,
            display_name: "Admin".to_string(),
            role: "admin".to_string(),
        };
        let admin = AdminUser(auth_user);
        assert_eq!(admin.0.user_id, 42);
        assert_eq!(admin.0.role, "admin");
    }
}
