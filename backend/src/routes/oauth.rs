use std::str::FromStr;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use chrono::Utc;

use super::AppState;
use crate::auth::middleware::{AuthUser, OptionalAuthUser};
use crate::auth::oauth::{
    OAUTH_FLOW_COOKIE, OAUTH_LINK_COOKIE, OAUTH_TTL_SECONDS, clear_short_lived_cookie,
    constant_time_secret_eq, generate_flow_secrets, hash_secret, link_confirmation_token,
    random_urlsafe_token, short_lived_cookie,
};
use crate::db::{oauth, privacy_consents, users};
use crate::error::AppError;
use crate::models::oauth::{
    AuthOptionsResponse, OAuthAvailability, OAuthCallbackQuery, OAuthIntent, OAuthProvider,
    OAuthStartRequest, OAuthStartResponse, PendingOAuthLinkResponse, PrivacyConsentResponse,
    PrivacyPolicyOption,
};
use crate::models::user::MessageResponse;
use crate::services::oauth::OAuthServiceError;
use crate::services::rate_limit::{PeerAddress, RateLimitPolicy};

const LINK_CSRF_HEADER: &str = "x-csrf-token";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/options", get(options))
        .route("/api/v1/auth/oauth/{provider}/start", post(start_oauth))
        .route(
            "/api/v1/auth/oauth/{provider}/callback",
            get(oauth_callback),
        )
        .route(
            "/api/v1/auth/oauth/link",
            get(pending_link_status)
                .post(confirm_link)
                .delete(cancel_link),
        )
        .route("/api/v1/me/privacy-consents", get(list_privacy_consents))
}

async fn options(State(state): State<AppState>) -> Json<AuthOptionsResponse> {
    Json(AuthOptionsResponse {
        privacy_policy: PrivacyPolicyOption {
            version: state.inner.privacy_policy_version.clone(),
            url: "/privacy".to_string(),
        },
        oauth: OAuthAvailability {
            google: state.inner.oauth_service.is_enabled(OAuthProvider::Google),
            github: state.inner.oauth_service.is_enabled(OAuthProvider::GitHub),
        },
    })
}

#[allow(clippy::too_many_lines)]
async fn start_oauth(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    auth: OptionalAuthUser,
    jar: CookieJar,
    Json(payload): Json<OAuthStartRequest>,
) -> Result<(CookieJar, Json<OAuthStartResponse>), AppError> {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::OAuthStart, &client)
        .await?;
    let provider = parse_provider(&provider)?;
    if !state.inner.oauth_service.is_enabled(provider) {
        return Err(AppError::ConflictWithCode {
            message: "OAuth provider is not configured".to_string(),
            code: "OAUTH_PROVIDER_DISABLED".to_string(),
        });
    }
    let reauth_user_id = match payload.intent {
        OAuthIntent::Reauth => {
            let auth = auth.0.ok_or_else(|| {
                AppError::Unauthorized("Authentication required for reauthentication".to_string())
            })?;
            let methods = oauth_auth_methods(&state, auth.user_id).await?;
            let linked = match provider {
                OAuthProvider::Google => methods.google,
                OAuthProvider::GitHub => methods.github,
            };
            if !linked {
                return Err(AppError::ConflictWithCode {
                    message: "This OAuth provider is not linked to the account".to_string(),
                    code: "REAUTH_METHOD_UNAVAILABLE".to_string(),
                });
            }
            Some(auth.user_id)
        }
        OAuthIntent::Login | OAuthIntent::Register => None,
    };
    let consent = match payload.intent {
        OAuthIntent::Login | OAuthIntent::Reauth => (None, None),
        OAuthIntent::Register => {
            if !payload.privacy_consent {
                return Err(AppError::BadRequest(
                    "Privacy consent is required".to_string(),
                ));
            }
            let version = payload
                .privacy_policy_version
                .as_deref()
                .unwrap_or_default();
            if version != state.inner.privacy_policy_version {
                return Err(privacy_policy_changed());
            }
            (Some(version), Some(Utc::now().naive_utc()))
        }
    };

    let browser_binding = jar
        .get(OAUTH_FLOW_COOKIE)
        .map(|cookie| cookie.value().to_string())
        .filter(|binding| binding.len() >= 32)
        .unwrap_or_else(random_urlsafe_token);
    let secrets = generate_flow_secrets(browser_binding);
    let now = Utc::now().naive_utc();
    let expires_at = now + chrono::Duration::seconds(OAUTH_TTL_SECONDS);
    if let Err(error) = oauth::cleanup_expired(&state.inner.pool, now).await {
        tracing::warn!(error = %error, "Failed to clean expired OAuth state");
    }
    oauth::insert_flow(
        &state.inner.pool,
        &oauth::NewOAuthFlow {
            state_hash: &secrets.state_hash,
            browser_binding_hash: &secrets.browser_binding_hash,
            provider: provider.as_str(),
            intent: payload.intent.as_str(),
            reauth_user_id,
            pkce_verifier: &secrets.pkce_verifier,
            privacy_policy_version: consent.0,
            consented_at: consent.1,
            created_at: now,
            expires_at,
        },
    )
    .await?;

    let redirect_uri = callback_uri(&state, provider);
    let authorization_url = state
        .inner
        .oauth_service
        .authorization_url(
            provider,
            &redirect_uri,
            &secrets.state,
            &secrets.pkce_challenge,
        )
        .map_err(map_service_error)?;
    let jar = jar.add(short_lived_cookie(
        OAUTH_FLOW_COOKIE,
        secrets.browser_binding,
        state.inner.cookie_secure,
    ));
    Ok((jar, Json(OAuthStartResponse { authorization_url })))
}

#[allow(clippy::too_many_lines)]
async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<OAuthCallbackQuery>,
    headers: HeaderMap,
    PeerAddress(peer_address): PeerAddress,
    jar: CookieJar,
) -> Response {
    let client = state
        .inner
        .request_security
        .client_identity(&headers, peer_address);
    if let Err(error) = state
        .inner
        .request_security
        .enforce_client(RateLimitPolicy::OAuthCallback, &client)
        .await
    {
        return error.into_response();
    }
    let Ok(provider) = OAuthProvider::from_str(&provider) else {
        return oauth_redirect(&state, "/login", "OAUTH_PROVIDER_DISABLED", jar).into_response();
    };
    let Some(state_value) = query.state.as_deref() else {
        return oauth_redirect(&state, "/login", "OAUTH_STATE_INVALID", jar).into_response();
    };
    let Some(browser_binding) = jar
        .get(OAUTH_FLOW_COOKIE)
        .map(|cookie| cookie.value().to_string())
    else {
        return oauth_redirect(&state, "/login", "OAUTH_STATE_INVALID", jar).into_response();
    };
    let now = Utc::now().naive_utc();
    let flow = match oauth::consume_flow(
        &state.inner.pool,
        &hash_secret(state_value),
        &hash_secret(&browser_binding),
        provider.as_str(),
        now,
    )
    .await
    {
        Ok(Some(flow)) => flow,
        Ok(None) => {
            return oauth_redirect(&state, "/login", "OAUTH_STATE_INVALID", jar).into_response();
        }
        Err(error) => {
            tracing::error!(error = %error, provider = %provider, "OAuth flow lookup failed");
            return oauth_redirect(&state, "/login", "OAUTH_PROVIDER_ERROR", jar).into_response();
        }
    };
    let intent = OAuthIntent::from_str(&flow.intent).unwrap_or(OAuthIntent::Login);
    let failure_path = match intent {
        OAuthIntent::Login => "/login",
        OAuthIntent::Register => "/register",
        OAuthIntent::Reauth => "/profile",
    };
    if query.error.is_some() {
        return oauth_redirect(&state, failure_path, "OAUTH_PROVIDER_DENIED", jar).into_response();
    }
    let Some(code) = query.code.as_deref() else {
        return oauth_redirect(&state, failure_path, "OAUTH_PROVIDER_ERROR", jar).into_response();
    };

    let profile = match state
        .inner
        .oauth_service
        .exchange_profile(
            provider,
            code,
            &callback_uri(&state, provider),
            &flow.pkce_verifier,
        )
        .await
    {
        Ok(profile) => profile,
        Err(OAuthServiceError::VerifiedEmailRequired) => {
            return oauth_redirect(&state, failure_path, "OAUTH_VERIFIED_EMAIL_REQUIRED", jar)
                .into_response();
        }
        Err(error) => {
            tracing::warn!(provider = %provider, error = %error, "OAuth provider exchange failed");
            return oauth_redirect(&state, failure_path, "OAUTH_PROVIDER_ERROR", jar)
                .into_response();
        }
    };

    match oauth::find_user_by_identity(&state.inner.pool, provider.as_str(), &profile.subject).await
    {
        Ok(Some(user)) => {
            if intent == OAuthIntent::Reauth && flow.reauth_user_id != Some(user.id) {
                return oauth_redirect(&state, failure_path, "OAUTH_REAUTH_MISMATCH", jar)
                    .into_response();
            }
            if let Err(error) =
                oauth::touch_identity(&state.inner.pool, provider.as_str(), &profile.subject, now)
                    .await
            {
                tracing::warn!(error = %error, user_id = user.id, "Failed to update OAuth login timestamp");
            }
            let success_path = if intent == OAuthIntent::Reauth {
                "/profile?reauth=success"
            } else {
                oauth_login_success_path(&jar)
            };
            return authenticated_or_recovery_redirect(&state, jar, &user, success_path).await;
        }
        Ok(None) => {}
        Err(error) => return AppError::from(error).into_response(),
    }

    if intent == OAuthIntent::Reauth {
        return oauth_redirect(&state, failure_path, "OAUTH_REAUTH_MISMATCH", jar).into_response();
    }

    match users::find_user_by_email(&state.inner.pool, &profile.email).await {
        Ok(Some(_)) => {
            return pending_link_redirect(&state, jar, &profile, now).await;
        }
        Ok(None) => {}
        Err(error) => return AppError::from(error).into_response(),
    }

    if intent == OAuthIntent::Login {
        return oauth_redirect(&state, "/login", "OAUTH_REGISTRATION_REQUIRED", jar)
            .into_response();
    }
    let (Some(policy_version), Some(consented_at)) =
        (flow.privacy_policy_version.as_deref(), flow.consented_at)
    else {
        return oauth_redirect(&state, "/register", "PRIVACY_CONSENT_REQUIRED", jar)
            .into_response();
    };
    if policy_version != state.inner.privacy_policy_version {
        return oauth_redirect(&state, "/register", "PRIVACY_POLICY_CHANGED", jar).into_response();
    }

    match oauth::create_oauth_user(&state.inner.pool, &profile, policy_version, consented_at).await
    {
        Ok(user) => {
            authenticated_or_recovery_redirect(&state, jar, &user, "/?oauth=registered").await
        }
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            tracing::info!(provider = %provider, "Concurrent OAuth registration detected");
            match oauth::find_user_by_identity(
                &state.inner.pool,
                provider.as_str(),
                &profile.subject,
            )
            .await
            {
                Ok(Some(user)) => {
                    let success_path = oauth_login_success_path(&jar);
                    authenticated_or_recovery_redirect(&state, jar, &user, success_path).await
                }
                Ok(None) => pending_link_redirect(&state, jar, &profile, now).await,
                Err(error) => AppError::from(error).into_response(),
            }
        }
        Err(error) => AppError::from(error).into_response(),
    }
}

async fn pending_link_status(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<PendingOAuthLinkResponse>, AppError> {
    let Some(token) = jar
        .get(OAUTH_LINK_COOKIE)
        .map(axum_extra::extract::cookie::Cookie::value)
    else {
        return Ok(Json(no_pending_link()));
    };
    let pending = oauth::find_pending_link(
        &state.inner.pool,
        &hash_secret(token),
        Utc::now().naive_utc(),
    )
    .await?;
    Ok(Json(match pending {
        Some(pending) => PendingOAuthLinkResponse {
            pending: true,
            provider: Some(pending.provider),
            masked_email: Some(mask_email(&pending.verified_email)),
            expires_at: Some(pending.expires_at),
            confirmation_token: Some(link_confirmation_token(token)),
        },
        None => no_pending_link(),
    }))
}

async fn confirm_link(
    State(state): State<AppState>,
    auth: AuthUser,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<(CookieJar, Json<MessageResponse>), AppError> {
    let token = jar
        .get(OAUTH_LINK_COOKIE)
        .map(axum_extra::extract::cookie::Cookie::value)
        .ok_or_else(link_required)?;
    let expected_confirmation = link_confirmation_token(token);
    let supplied_confirmation = headers
        .get(LINK_CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !constant_time_secret_eq(supplied_confirmation, &expected_confirmation) {
        return Err(AppError::Forbidden {
            message: "OAuth link confirmation token is missing or invalid".to_string(),
            code: Some("OAUTH_LINK_CSRF_INVALID".to_string()),
        });
    }
    match oauth::confirm_pending_link(
        &state.inner.pool,
        &hash_secret(token),
        auth.user_id,
        Utc::now().naive_utc(),
    )
    .await?
    {
        oauth::LinkPendingResult::Linked => {
            let jar = jar.add(clear_short_lived_cookie(
                OAUTH_LINK_COOKIE,
                state.inner.cookie_secure,
            ));
            Ok((
                jar,
                Json(MessageResponse {
                    message: "OAuth provider linked".to_string(),
                }),
            ))
        }
        oauth::LinkPendingResult::Missing => Err(link_required()),
        oauth::LinkPendingResult::EmailMismatch => Err(AppError::Forbidden {
            message: "Sign in to the matching existing account".to_string(),
            code: Some("OAUTH_LINK_MISMATCH".to_string()),
        }),
        oauth::LinkPendingResult::Conflict => Err(AppError::ConflictWithCode {
            message: "OAuth provider is already linked".to_string(),
            code: "OAUTH_LINK_CONFLICT".to_string(),
        }),
    }
}

async fn cancel_link(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Some(token) = jar
        .get(OAUTH_LINK_COOKIE)
        .map(axum_extra::extract::cookie::Cookie::value)
    {
        oauth::delete_pending_link(&state.inner.pool, &hash_secret(token)).await?;
    }
    Ok((
        jar.add(clear_short_lived_cookie(
            OAUTH_LINK_COOKIE,
            state.inner.cookie_secure,
        )),
        StatusCode::NO_CONTENT,
    ))
}

async fn list_privacy_consents(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<PrivacyConsentResponse>>, AppError> {
    Ok(Json(
        privacy_consents::find_for_user(&state.inner.pool, auth.user_id).await?,
    ))
}

fn parse_provider(value: &str) -> Result<OAuthProvider, AppError> {
    OAuthProvider::from_str(value).map_err(|()| AppError::ConflictWithCode {
        message: "OAuth provider is not supported".to_string(),
        code: "OAUTH_PROVIDER_DISABLED".to_string(),
    })
}

fn map_service_error(error: OAuthServiceError) -> AppError {
    match error {
        OAuthServiceError::ProviderDisabled => AppError::ConflictWithCode {
            message: error.to_string(),
            code: "OAUTH_PROVIDER_DISABLED".to_string(),
        },
        other => AppError::InternalError(other.into()),
    }
}

fn privacy_policy_changed() -> AppError {
    AppError::ConflictWithCode {
        message: "Privacy policy changed; please review it again".to_string(),
        code: "PRIVACY_POLICY_CHANGED".to_string(),
    }
}

fn link_required() -> AppError {
    AppError::ConflictWithCode {
        message: "OAuth link request is missing or expired".to_string(),
        code: "OAUTH_LINK_REQUIRED".to_string(),
    }
}

fn no_pending_link() -> PendingOAuthLinkResponse {
    PendingOAuthLinkResponse {
        pending: false,
        provider: None,
        masked_email: None,
        expires_at: None,
        confirmation_token: None,
    }
}

fn callback_uri(state: &AppState, provider: OAuthProvider) -> String {
    format!(
        "{}/api/v1/auth/oauth/{provider}/callback",
        state.inner.app_base_url.trim_end_matches('/')
    )
}

fn app_url(state: &AppState, path: &str) -> String {
    format!("{}{}", state.inner.app_base_url.trim_end_matches('/'), path)
}

fn oauth_login_success_path(jar: &CookieJar) -> &'static str {
    if jar.get(OAUTH_LINK_COOKIE).is_some() {
        "/oauth/link"
    } else {
        "/?oauth=success"
    }
}

async fn oauth_auth_methods(
    state: &AppState,
    user_id: u32,
) -> Result<crate::db::account_erasure::AuthMethodsRow, AppError> {
    crate::db::account_erasure::find_auth_methods(&state.inner.pool, user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Authentication required".to_string()))
}

async fn authenticated_or_recovery_redirect(
    state: &AppState,
    jar: CookieJar,
    user: &crate::models::user::User,
    success_path: &str,
) -> Response {
    if !user.is_active() {
        return match crate::services::account_erasure::issue_recovery_token(
            &state.inner.pool,
            user.id,
            Utc::now().naive_utc(),
        )
        .await
        {
            Ok(Some((raw_token, scheduled_for))) => {
                let jar = crate::services::account_erasure::recovery_jar(
                    jar.add(super::auth::clear_cookie("access_token", "/api"))
                        .add(super::auth::clear_cookie("refresh_token", "/api/v1/auth")),
                    raw_token,
                    scheduled_for,
                    state.inner.cookie_secure,
                );
                (jar, Redirect::to(&app_url(state, "/account/deletion"))).into_response()
            }
            Ok(None) => oauth_redirect(state, "/login", "ACCOUNT_DELETION_WINDOW_EXPIRED", jar)
                .into_response(),
            Err(error) => error.into_response(),
        };
    }
    match super::auth::authenticated_jar(state, jar, user).await {
        Ok(jar) => (jar, Redirect::to(&app_url(state, success_path))).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn pending_link_redirect(
    state: &AppState,
    jar: CookieJar,
    profile: &crate::models::oauth::OAuthIdentityProfile,
    now: chrono::NaiveDateTime,
) -> Response {
    let raw_link_token = random_urlsafe_token();
    let expires_at = now + chrono::Duration::seconds(OAUTH_TTL_SECONDS);
    let pending_link_created = match oauth::insert_pending_link_if_account_active(
        &state.inner.pool,
        &hash_secret(&raw_link_token),
        profile,
        now,
        expires_at,
    )
    .await
    {
        Ok(created) => created,
        Err(error) => return AppError::from(error).into_response(),
    };
    if !pending_link_created {
        return oauth_redirect(state, "/login", "ACCOUNT_DELETION_PENDING", jar).into_response();
    }
    let jar = jar.add(short_lived_cookie(
        OAUTH_LINK_COOKIE,
        raw_link_token,
        state.inner.cookie_secure,
    ));
    (jar, Redirect::to(&app_url(state, "/oauth/link"))).into_response()
}

fn oauth_redirect(
    state: &AppState,
    path: &str,
    code: &str,
    jar: CookieJar,
) -> (CookieJar, Redirect) {
    let separator = if path.contains('?') { '&' } else { '?' };
    (
        jar,
        Redirect::to(&app_url(
            state,
            &format!("{path}{separator}oauth_error={code}"),
        )),
    )
}

fn mask_email(email: &str) -> String {
    let Some((local, domain)) = email.split_once('@') else {
        return "***".to_string();
    };
    let first = local.chars().next().unwrap_or('*');
    format!("{first}***@{domain}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap as TestHashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::config::{OAuthCredentials, PhotoUploadConfig};
    use crate::routes::AppStateInner;
    use crate::services::email::EmailService;
    use crate::services::import_scheduler::ImportSchedulerConfig;
    use crate::services::media::MediaStorage;
    use crate::services::oauth::OAuthService;
    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderMap, Request, header};
    use axum::{Form, Json};
    use lilly_importer_core::AdapterRegistry;
    use serde_json::json;
    use sqlx::mysql::MySqlPoolOptions;
    use tower::ServiceExt;
    use url::Url;

    use super::*;

    fn test_state(pool: sqlx::MySqlPool, oauth_service: OAuthService) -> AppState {
        let media_path = PathBuf::from("/tmp/lilly-oauth-route-tests");
        AppState {
            inner: Arc::new(AppStateInner {
                pool,
                jwt_secret: "oauth-route-test-secret".to_string(),
                jwt_access_expiry: 900,
                jwt_refresh_expiry: 2_592_000,
                password_reset_ttl_seconds: 3_600,
                email_service: EmailService::Log {
                    from: "test@example.test".to_string(),
                },
                app_base_url: "https://lilly.test".to_string(),
                cookie_secure: true,
                oauth_service,
                privacy_policy_version: "policy-test-v1".to_string(),
                adapter_registry: AdapterRegistry::new(),
                media_path: media_path.clone(),
                media_url_prefix: "/media".to_string(),
                photo_upload_config: PhotoUploadConfig::default(),
                media_storage: MediaStorage::new(&media_path),
                erasure_ledger: crate::services::account_erasure::ErasureLedger::new(
                    media_path.join("erasure-ledger"),
                ),
                import_scheduler_config: ImportSchedulerConfig {
                    enabled: false,
                    schedule: "0 10 6 * * Sat *".to_string(),
                    timezone: "Europe/Berlin".to_string(),
                    adapters: Vec::new(),
                },
                request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
            }),
        }
    }

    fn enabled_service() -> OAuthService {
        let credentials = OAuthCredentials {
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
        };
        OAuthService::production(Some(credentials.clone()), Some(credentials))
    }

    fn lazy_pool() -> sqlx::MySqlPool {
        MySqlPoolOptions::new()
            .connect_lazy("mysql://test:test@localhost/test")
            .unwrap()
    }

    #[derive(Clone)]
    struct MockProviderState {
        new_account_email: String,
        pending_email: String,
        stale_email: String,
        consumed_email: String,
        github_email: String,
        github_id: u64,
    }

    async fn spawn_mock_provider(state: MockProviderState) -> String {
        async fn token(
            Form(payload): Form<TestHashMap<String, String>>,
        ) -> (StatusCode, Json<serde_json::Value>) {
            let code = payload.get("code").map(String::as_str).unwrap_or_default();
            if code == "provider-error" {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": "provider unavailable" })),
                );
            }
            (StatusCode::OK, Json(json!({ "access_token": code })))
        }

        async fn google_user(
            State(state): State<MockProviderState>,
            headers: HeaderMap,
        ) -> Json<serde_json::Value> {
            let access_token = bearer_token(&headers);
            let (email, verified) = match access_token {
                "new-account" => (state.new_account_email.as_str(), true),
                "pending-link" => (state.pending_email.as_str(), true),
                "stale-consent" => (state.stale_email.as_str(), true),
                "consumed-flow" => (state.consumed_email.as_str(), true),
                "unverified" => ("unverified@example.test", false),
                _ => ("default@example.test", true),
            };
            Json(json!({
                "sub": format!("subject-{email}"),
                "email": email,
                "email_verified": verified,
                "name": "OAuth Route Collector"
            }))
        }

        async fn github_user(State(state): State<MockProviderState>) -> Json<serde_json::Value> {
            Json(json!({
                "id": state.github_id,
                "login": "github-route-collector",
                "name": "GitHub Route Collector"
            }))
        }

        async fn github_emails(State(state): State<MockProviderState>) -> Json<serde_json::Value> {
            Json(json!([{
                "email": state.github_email,
                "primary": true,
                "verified": true
            }]))
        }

        let app = Router::new()
            .route("/token", post(token))
            .route("/google/user", get(google_user))
            .route("/github/user", get(github_user))
            .route("/github/emails", get(github_emails))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn bearer_token(headers: &HeaderMap) -> &str {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .unwrap_or_default()
    }

    async fn insert_callback_flow(
        pool: &sqlx::MySqlPool,
        provider: OAuthProvider,
        intent: OAuthIntent,
        state_value: &str,
        browser_binding: &str,
        policy_version: Option<&str>,
    ) {
        let now = Utc::now().naive_utc();
        oauth::insert_flow(
            pool,
            &oauth::NewOAuthFlow {
                state_hash: &hash_secret(state_value),
                browser_binding_hash: &hash_secret(browser_binding),
                provider: provider.as_str(),
                intent: intent.as_str(),
                reauth_user_id: None,
                pkce_verifier: "callback-test-pkce",
                privacy_policy_version: policy_version,
                consented_at: policy_version.map(|_| now),
                created_at: now,
                expires_at: now + chrono::Duration::minutes(10),
            },
        )
        .await
        .unwrap();
    }

    fn cookie_pair(response: &Response, name: &str) -> Option<String> {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with(&format!("{name}=")))
            .and_then(|value| value.split(';').next())
            .map(str::to_string)
    }

    fn location(response: &Response) -> &str {
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap()
    }

    #[test]
    fn email_mask_retains_only_first_character_and_domain() {
        assert_eq!(mask_email("holger@example.org"), "h***@example.org");
        assert_eq!(mask_email("a@example.org"), "a***@example.org");
        assert_eq!(mask_email("invalid"), "***");
    }

    #[tokio::test]
    async fn callback_uri_is_derived_from_app_base_url() {
        let state = test_state(lazy_pool(), OAuthService::disabled());
        assert_eq!(
            callback_uri(&state, OAuthProvider::Google),
            "https://lilly.test/api/v1/auth/oauth/google/callback"
        );
    }

    #[tokio::test]
    async fn options_exposes_policy_and_only_configured_providers() {
        let state = test_state(
            lazy_pool(),
            OAuthService::production(
                Some(OAuthCredentials {
                    client_id: "google-client".to_string(),
                    client_secret: "google-secret".to_string(),
                }),
                None,
            ),
        );
        let response = Router::new()
            .merge(router())
            .with_state(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/options")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["privacy_policy"]["version"], "policy-test-v1");
        assert_eq!(json["oauth"]["google"], true);
        assert_eq!(json["oauth"]["github"], false);
    }

    #[tokio::test]
    async fn oauth_start_route_uses_the_shared_rate_limit() {
        let mut state = test_state(lazy_pool(), OAuthService::disabled());
        let inner = Arc::get_mut(&mut state.inner).unwrap();
        let mut config = crate::config::RateLimitConfig::default();
        config.oauth_start = crate::config::RateLimitRule {
            max_requests: 2,
            window_seconds: 60,
        };
        inner.request_security = crate::services::rate_limit::RequestSecurity::new(
            config,
            Vec::new(),
            "oauth-limit-test",
        );
        let app = Router::new().merge(router()).with_state(state);

        for expected_status in [
            StatusCode::CONFLICT,
            StatusCode::CONFLICT,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/auth/oauth/google/start")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(r#"{"intent":"login"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected_status);
        }
    }

    #[tokio::test]
    async fn oauth_start_rejects_disabled_provider_missing_consent_and_stale_policy() {
        let disabled = test_state(lazy_pool(), OAuthService::disabled());
        let disabled_response = Router::new()
            .merge(router())
            .with_state(disabled)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/oauth/google/start")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"intent":"login"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(disabled_response.status(), StatusCode::CONFLICT);

        for (payload, status) in [
            (
                r#"{"intent":"register","privacy_consent":false,"privacy_policy_version":"policy-test-v1"}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                r#"{"intent":"register","privacy_consent":true,"privacy_policy_version":"stale"}"#,
                StatusCode::CONFLICT,
            ),
        ] {
            let response = Router::new()
                .merge(router())
                .with_state(test_state(lazy_pool(), enabled_service()))
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/auth/oauth/github/start")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(payload))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
    }

    #[tokio::test]
    async fn oauth_start_persists_only_hashed_state_and_sets_a_hardened_binding_cookie() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let response = Router::new()
            .merge(router())
            .with_state(test_state(pool.clone(), enabled_service()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/oauth/google/start")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"intent":"register","privacy_consent":true,"privacy_policy_version":"policy-test-v1"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(cookie.starts_with("oauth_flow_binding="));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("Path=/api/v1/auth/oauth"));
        let binding_cookie = cookie.split(';').next().unwrap().to_string();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let authorization_url = Url::parse(json["authorization_url"].as_str().unwrap()).unwrap();
        let state_value = authorization_url
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
            .unwrap();
        assert!(!cookie.contains(&state_value));
        let state_hash = hash_secret(&state_value);
        let stored: (String, String, Option<chrono::NaiveDateTime>, String) = sqlx::query_as(
            "SELECT intent, privacy_policy_version, consented_at, browser_binding_hash \
             FROM oauth_authorization_flows WHERE state_hash = ?",
        )
        .bind(&state_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored.0, "register");
        assert_eq!(stored.1, "policy-test-v1");
        assert!(stored.2.is_some());

        let second_response = Router::new()
            .merge(router())
            .with_state(test_state(pool.clone(), enabled_service()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/oauth/github/start")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::COOKIE, binding_cookie)
                    .body(Body::from(r#"{"intent":"login"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second_response.status(), StatusCode::OK);
        let second_body = to_bytes(second_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let second_json: serde_json::Value = serde_json::from_slice(&second_body).unwrap();
        let second_url = Url::parse(second_json["authorization_url"].as_str().unwrap()).unwrap();
        let second_state = second_url
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
            .unwrap();
        let second_state_hash = hash_secret(&second_state);
        let second_binding_hash: (String,) = sqlx::query_as(
            "SELECT browser_binding_hash FROM oauth_authorization_flows WHERE state_hash = ?",
        )
        .bind(&second_state_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(second_binding_hash.0, stored.3);

        sqlx::query("DELETE FROM oauth_authorization_flows WHERE state_hash IN (?, ?)")
            .bind(state_hash)
            .bind(second_state_hash)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn oauth_callback_covers_login_registration_linking_and_failure_paths() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _guard = crate::db::IMPORT_SYNC_TEST_LOCK.lock().await;
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .unwrap();
        crate::db::migrate_test_database(&pool).await.unwrap();
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let provider_state = MockProviderState {
            new_account_email: format!("oauth-new-{suffix}@example.test"),
            pending_email: format!("oauth-pending-{suffix}@example.test"),
            stale_email: format!("oauth-stale-{suffix}@example.test"),
            consumed_email: format!("oauth-consumed-{suffix}@example.test"),
            github_email: format!("oauth-github-{suffix}@example.test"),
            github_id: 10_000_000 + u64::try_from(suffix % 1_000_000).unwrap(),
        };
        let provider_base = spawn_mock_provider(provider_state.clone()).await;
        let state = test_state(pool.clone(), OAuthService::testing(&provider_base));
        let app = Router::new().merge(router()).with_state(state);
        let browser_binding = format!("callback-browser-binding-{suffix}");

        let github_profile = crate::models::oauth::OAuthIdentityProfile {
            provider: OAuthProvider::GitHub,
            subject: provider_state.github_id.to_string(),
            email: provider_state.github_email.clone(),
            display_name: "GitHub Route Collector".to_string(),
        };
        let github_user = oauth::create_oauth_user(
            &pool,
            &github_profile,
            "policy-test-v1",
            Utc::now().naive_utc(),
        )
        .await
        .unwrap();
        let pending_secret = format!("pending-link-{suffix}");
        let pending_profile = crate::models::oauth::OAuthIdentityProfile {
            provider: OAuthProvider::Google,
            subject: format!("pending-google-{suffix}"),
            email: provider_state.github_email.clone(),
            display_name: "Pending Google Collector".to_string(),
        };
        oauth::insert_pending_link(
            &pool,
            &hash_secret(&pending_secret),
            &pending_profile,
            Utc::now().naive_utc(),
            Utc::now().naive_utc() + chrono::Duration::minutes(10),
        )
        .await
        .unwrap();
        let github_login_state = format!("github-login-{suffix}");
        insert_callback_flow(
            &pool,
            OAuthProvider::GitHub,
            OAuthIntent::Login,
            &github_login_state,
            &browser_binding,
            None,
        )
        .await;

        let github_login = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/auth/oauth/github/callback?code=github-login&state={github_login_state}"
                    ))
                    .header(
                        header::COOKIE,
                        format!(
                            "{OAUTH_FLOW_COOKIE}={browser_binding}; {OAUTH_LINK_COOKIE}={pending_secret}"
                        ),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(github_login.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&github_login), "https://lilly.test/oauth/link");
        assert!(cookie_pair(&github_login, OAUTH_FLOW_COOKIE).is_none());
        let access_cookie = cookie_pair(&github_login, "access_token").unwrap();

        let normal_login_state = format!("github-normal-login-{suffix}");
        insert_callback_flow(
            &pool,
            OAuthProvider::GitHub,
            OAuthIntent::Login,
            &normal_login_state,
            &browser_binding,
            None,
        )
        .await;
        let normal_login = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/auth/oauth/github/callback?code=github-login&state={normal_login_state}"
                    ))
                    .header(
                        header::COOKIE,
                        format!("{OAUTH_FLOW_COOKIE}={browser_binding}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(location(&normal_login), "https://lilly.test/?oauth=success");

        let pending_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/oauth/link")
                    .header(
                        header::COOKIE,
                        format!("{OAUTH_LINK_COOKIE}={pending_secret}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pending_status.status(), StatusCode::OK);
        let pending_json: serde_json::Value = serde_json::from_slice(
            &to_bytes(pending_status.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let confirmation_token = pending_json["confirmation_token"]
            .as_str()
            .unwrap()
            .to_string();

        let missing_csrf = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/oauth/link")
                    .header(
                        header::COOKIE,
                        format!("{access_cookie}; {OAUTH_LINK_COOKIE}={pending_secret}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
        assert!(
            oauth::find_user_by_identity(&pool, "google", &pending_profile.subject)
                .await
                .unwrap()
                .is_none()
        );

        let confirmed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/oauth/link")
                    .header(
                        header::COOKIE,
                        format!("{access_cookie}; {OAUTH_LINK_COOKIE}={pending_secret}"),
                    )
                    .header(LINK_CSRF_HEADER, confirmation_token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(confirmed.status(), StatusCode::OK);
        assert_eq!(
            oauth::find_user_by_identity(&pool, "google", &pending_profile.subject)
                .await
                .unwrap()
                .unwrap()
                .id,
            github_user.id
        );

        let registration_state = format!("registration-{suffix}");
        insert_callback_flow(
            &pool,
            OAuthProvider::Google,
            OAuthIntent::Register,
            &registration_state,
            &browser_binding,
            Some("policy-test-v1"),
        )
        .await;
        let registered = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/auth/oauth/google/callback?code=new-account&state={registration_state}"
                    ))
                    .header(
                        header::COOKIE,
                        format!("{OAUTH_FLOW_COOKIE}={browser_binding}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            location(&registered),
            "https://lilly.test/?oauth=registered"
        );
        assert!(
            users::find_user_by_email(&pool, &provider_state.new_account_email)
                .await
                .unwrap()
                .is_some()
        );

        sqlx::query(
            "INSERT INTO users (email, display_name, role, email_verified) \
             VALUES (?, 'Pending Route Collector', 'user', TRUE)",
        )
        .bind(&provider_state.pending_email)
        .execute(&pool)
        .await
        .unwrap();
        let pending_state = format!("pending-{suffix}");
        insert_callback_flow(
            &pool,
            OAuthProvider::Google,
            OAuthIntent::Login,
            &pending_state,
            &browser_binding,
            None,
        )
        .await;
        let pending_redirect = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/auth/oauth/google/callback?code=pending-link&state={pending_state}"
                    ))
                    .header(
                        header::COOKIE,
                        format!("{OAUTH_FLOW_COOKIE}={browser_binding}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(location(&pending_redirect), "https://lilly.test/oauth/link");
        assert!(cookie_pair(&pending_redirect, OAUTH_LINK_COOKIE).is_some());

        for (label, query, expected_code) in [
            ("denied", "error=access_denied", "OAUTH_PROVIDER_DENIED"),
            (
                "provider-failure",
                "code=provider-error",
                "OAUTH_PROVIDER_ERROR",
            ),
            (
                "unverified",
                "code=unverified",
                "OAUTH_VERIFIED_EMAIL_REQUIRED",
            ),
        ] {
            let callback_state = format!("{label}-{suffix}");
            insert_callback_flow(
                &pool,
                OAuthProvider::Google,
                OAuthIntent::Login,
                &callback_state,
                &browser_binding,
                None,
            )
            .await;
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "/api/v1/auth/oauth/google/callback?{query}&state={callback_state}"
                        ))
                        .header(
                            header::COOKIE,
                            format!("{OAUTH_FLOW_COOKIE}={browser_binding}"),
                        )
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(location(&response).ends_with(&format!("oauth_error={expected_code}")));
        }

        let stale_state = format!("stale-{suffix}");
        insert_callback_flow(
            &pool,
            OAuthProvider::Google,
            OAuthIntent::Register,
            &stale_state,
            &browser_binding,
            Some("policy-stale"),
        )
        .await;
        let stale_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/auth/oauth/google/callback?code=stale-consent&state={stale_state}"
                    ))
                    .header(
                        header::COOKIE,
                        format!("{OAUTH_FLOW_COOKIE}={browser_binding}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(location(&stale_response).ends_with("oauth_error=PRIVACY_POLICY_CHANGED"));

        let consumed_state = format!("consumed-{suffix}");
        insert_callback_flow(
            &pool,
            OAuthProvider::Google,
            OAuthIntent::Login,
            &consumed_state,
            &browser_binding,
            None,
        )
        .await;
        for expected_code in ["OAUTH_REGISTRATION_REQUIRED", "OAUTH_STATE_INVALID"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "/api/v1/auth/oauth/google/callback?code=consumed-flow&state={consumed_state}"
                        ))
                        .header(
                            header::COOKIE,
                            format!("{OAUTH_FLOW_COOKIE}={browser_binding}"),
                        )
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(location(&response).ends_with(&format!("oauth_error={expected_code}")));
        }

        let invalid = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/auth/oauth/google/callback?code=unused&state=invalid-{suffix}"
                    ))
                    .header(
                        header::COOKIE,
                        format!("{OAUTH_FLOW_COOKIE}={browser_binding}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(location(&invalid).ends_with("oauth_error=OAUTH_STATE_INVALID"));

        sqlx::query("DELETE FROM pending_oauth_links WHERE verified_email = ?")
            .bind(&provider_state.pending_email)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM oauth_authorization_flows WHERE browser_binding_hash = ?")
            .bind(hash_secret(&browser_binding))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE email IN (?, ?, ?)")
            .bind(&provider_state.github_email)
            .bind(&provider_state.new_account_email)
            .bind(&provider_state.pending_email)
            .execute(&pool)
            .await
            .unwrap();
    }
}
