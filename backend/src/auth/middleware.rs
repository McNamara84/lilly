use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use chrono::Utc;

use crate::auth::jwt;
use crate::error::AppError;
use crate::routes::AppState;

async fn validate_active_claims(state: &AppState, claims: &jwt::Claims) -> Result<(), AppError> {
    #[cfg(test)]
    if claims.test_bypass_account_state {
        return Ok(());
    }

    let auth_state = crate::db::users::find_auth_state(&state.inner.pool, claims.sub)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid or expired token".to_string()))?;
    if auth_state.account_state != "active" {
        return Err(AppError::Forbidden {
            message: "Account deletion is pending".to_string(),
            code: Some("ACCOUNT_DELETION_PENDING".to_string()),
        });
    }
    if auth_state.session_version != claims.session_version {
        return Err(AppError::Unauthorized(
            "Invalid or expired token".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: u32,
    #[allow(dead_code)]
    pub display_name: String,
    #[allow(dead_code)]
    pub role: String,
    pub auth_time: usize,
    #[allow(dead_code)]
    pub session_version: u32,
}

#[derive(Debug, Clone)]
pub struct OptionalAuthUser(pub Option<AuthUser>);

#[derive(Debug, Clone)]
pub struct RecentAuthUser(pub AuthUser);

impl FromRequestParts<AppState> for OptionalAuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::InternalError(anyhow::anyhow!("Cookie extraction failed")))?;
        let Some(access_token) = jar.get("access_token") else {
            return Ok(Self(None));
        };
        let Ok(claims) = jwt::validate_token(access_token.value(), &state.inner.jwt_secret) else {
            return Ok(Self(None));
        };
        if validate_active_claims(state, &claims).await.is_err() {
            return Ok(Self(None));
        }
        Ok(Self(Some(AuthUser {
            user_id: claims.sub,
            display_name: claims.name,
            role: claims.role,
            auth_time: claims.auth_time,
            session_version: claims.session_version,
        })))
    }
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
        validate_active_claims(state, &claims).await?;

        Ok(Self {
            user_id: claims.sub,
            display_name: claims.name,
            role: claims.role,
            auth_time: claims.auth_time,
            session_version: claims.session_version,
        })
    }
}

impl FromRequestParts<AppState> for RecentAuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        let now = Utc::now().timestamp();
        let auth_time = i64::try_from(auth.auth_time).unwrap_or(i64::MAX);
        let age = now.saturating_sub(auth_time);
        if auth_time <= 0
            || !(-60..=crate::models::account_erasure::RECENT_AUTH_SECONDS).contains(&age)
        {
            return Err(AppError::Forbidden {
                message: "Please authenticate again before deleting your account".to_string(),
                code: Some("RECENT_AUTH_REQUIRED".to_string()),
            });
        }
        Ok(Self(auth))
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
            auth_time: 0,
            session_version: 0,
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
            auth_time: 0,
            session_version: 0,
        };
        let admin = AdminUser(auth_user);
        assert_eq!(admin.0.user_id, 42);
        assert_eq!(admin.0.role, "admin");
    }

    #[test]
    fn optional_auth_user_can_represent_anonymous_access() {
        let optional = OptionalAuthUser(None);
        assert!(optional.0.is_none());
    }
}
