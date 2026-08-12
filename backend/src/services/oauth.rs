use serde::Deserialize;
use url::Url;

use crate::config::OAuthCredentials;
use crate::models::oauth::{OAuthIdentityProfile, OAuthProvider};
use crate::models::user::normalize_email;

const GOOGLE_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const GITHUB_AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_URL: &str = "https://api.github.com/user";
const GITHUB_EMAILS_URL: &str = "https://api.github.com/user/emails";
const GITHUB_API_VERSION: &str = "2026-03-10";

#[derive(Debug, thiserror::Error)]
pub enum OAuthServiceError {
    #[error("OAuth provider is not configured")]
    ProviderDisabled,
    #[error("OAuth provider returned an invalid response")]
    InvalidProviderResponse,
    #[error("OAuth provider did not return a verified email address")]
    VerifiedEmailRequired,
    #[error("OAuth provider request failed")]
    Request(#[source] reqwest::Error),
    #[error("OAuth URL configuration is invalid")]
    InvalidUrl(#[source] url::ParseError),
}

#[derive(Clone)]
pub struct OAuthService {
    http: reqwest::Client,
    google: Option<ProviderClient>,
    github: Option<ProviderClient>,
}

#[derive(Clone)]
struct ProviderClient {
    credentials: OAuthCredentials,
    authorize_url: String,
    token_url: String,
    user_url: String,
    emails_url: Option<String>,
    scopes: &'static [&'static str],
}

impl OAuthService {
    #[must_use]
    pub fn production(google: Option<OAuthCredentials>, github: Option<OAuthCredentials>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent("LILLY/0.1 OAuth")
                .build()
                .expect("static OAuth HTTP client configuration must be valid"),
            google: google.map(|credentials| ProviderClient {
                credentials,
                authorize_url: GOOGLE_AUTHORIZE_URL.to_string(),
                token_url: GOOGLE_TOKEN_URL.to_string(),
                user_url: GOOGLE_USERINFO_URL.to_string(),
                emails_url: None,
                scopes: &["openid", "email", "profile"],
            }),
            github: github.map(|credentials| ProviderClient {
                credentials,
                authorize_url: GITHUB_AUTHORIZE_URL.to_string(),
                token_url: GITHUB_TOKEN_URL.to_string(),
                user_url: GITHUB_USER_URL.to_string(),
                emails_url: Some(GITHUB_EMAILS_URL.to_string()),
                scopes: &["read:user", "user:email"],
            }),
        }
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn testing(base_url: &str) -> Self {
        let credentials = OAuthCredentials {
            client_id: "test-client".to_string(),
            client_secret: String::new(),
        };
        Self {
            http: reqwest::Client::new(),
            google: Some(ProviderClient {
                credentials: credentials.clone(),
                authorize_url: format!("{base_url}/authorize"),
                token_url: format!("{base_url}/token"),
                user_url: format!("{base_url}/google/user"),
                emails_url: None,
                scopes: &["openid", "email", "profile"],
            }),
            github: Some(ProviderClient {
                credentials,
                authorize_url: format!("{base_url}/authorize"),
                token_url: format!("{base_url}/token"),
                user_url: format!("{base_url}/github/user"),
                emails_url: Some(format!("{base_url}/github/emails")),
                scopes: &["read:user", "user:email"],
            }),
        }
    }

    #[must_use]
    #[cfg(test)]
    pub fn disabled() -> Self {
        Self::production(None, None)
    }

    #[must_use]
    pub fn is_enabled(&self, provider: OAuthProvider) -> bool {
        self.client(provider).is_ok()
    }

    pub fn authorization_url(
        &self,
        provider: OAuthProvider,
        redirect_uri: &str,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<String, OAuthServiceError> {
        let client = self.client(provider)?;
        let mut url = Url::parse(&client.authorize_url).map_err(OAuthServiceError::InvalidUrl)?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("client_id", &client.credentials.client_id)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("response_type", "code")
                .append_pair("scope", &client.scopes.join(" "))
                .append_pair("state", state)
                .append_pair("code_challenge", pkce_challenge)
                .append_pair("code_challenge_method", "S256");
            if provider == OAuthProvider::Google {
                query.append_pair("prompt", "select_account");
            }
        }
        Ok(url.to_string())
    }

    pub async fn exchange_profile(
        &self,
        provider: OAuthProvider,
        code: &str,
        redirect_uri: &str,
        pkce_verifier: &str,
    ) -> Result<OAuthIdentityProfile, OAuthServiceError> {
        let client = self.client(provider)?;
        let token = self
            .exchange_code(client, code, redirect_uri, pkce_verifier)
            .await?;
        match provider {
            OAuthProvider::Google => self.fetch_google_profile(client, &token).await,
            OAuthProvider::GitHub => self.fetch_github_profile(client, &token).await,
        }
    }

    fn client(&self, provider: OAuthProvider) -> Result<&ProviderClient, OAuthServiceError> {
        match provider {
            OAuthProvider::Google => self.google.as_ref(),
            OAuthProvider::GitHub => self.github.as_ref(),
        }
        .ok_or(OAuthServiceError::ProviderDisabled)
    }

    async fn exchange_code(
        &self,
        client: &ProviderClient,
        code: &str,
        redirect_uri: &str,
        pkce_verifier: &str,
    ) -> Result<String, OAuthServiceError> {
        let response = self
            .http
            .post(&client.token_url)
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[
                ("client_id", client.credentials.client_id.as_str()),
                ("client_secret", client.credentials.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
                ("code_verifier", pkce_verifier),
            ])
            .send()
            .await
            .map_err(OAuthServiceError::Request)?;
        if !response.status().is_success() {
            return Err(OAuthServiceError::InvalidProviderResponse);
        }
        let token: AccessTokenResponse =
            response.json().await.map_err(OAuthServiceError::Request)?;
        token
            .access_token
            .filter(|value| !value.is_empty())
            .ok_or(OAuthServiceError::InvalidProviderResponse)
    }

    async fn fetch_google_profile(
        &self,
        client: &ProviderClient,
        access_token: &str,
    ) -> Result<OAuthIdentityProfile, OAuthServiceError> {
        let response = self
            .http
            .get(&client.user_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(OAuthServiceError::Request)?;
        if !response.status().is_success() {
            return Err(OAuthServiceError::InvalidProviderResponse);
        }
        let profile: GoogleUserInfo = response.json().await.map_err(OAuthServiceError::Request)?;
        if !profile.email_verified {
            return Err(OAuthServiceError::VerifiedEmailRequired);
        }
        let email = normalize_email(&profile.email)
            .map_err(|_| OAuthServiceError::InvalidProviderResponse)?;
        Ok(OAuthIdentityProfile {
            provider: OAuthProvider::Google,
            subject: require_non_empty(profile.sub)?,
            display_name: normalized_display_name(profile.name.as_deref(), "Google-Sammler"),
            email,
        })
    }

    async fn fetch_github_profile(
        &self,
        client: &ProviderClient,
        access_token: &str,
    ) -> Result<OAuthIdentityProfile, OAuthServiceError> {
        let user_response = self
            .github_get(&client.user_url, access_token)
            .send()
            .await
            .map_err(OAuthServiceError::Request)?;
        if !user_response.status().is_success() {
            return Err(OAuthServiceError::InvalidProviderResponse);
        }
        let profile: GitHubUser = user_response
            .json()
            .await
            .map_err(OAuthServiceError::Request)?;

        let emails_url = client
            .emails_url
            .as_deref()
            .ok_or(OAuthServiceError::InvalidProviderResponse)?;
        let email_response = self
            .github_get(emails_url, access_token)
            .send()
            .await
            .map_err(OAuthServiceError::Request)?;
        if !email_response.status().is_success() {
            return Err(OAuthServiceError::VerifiedEmailRequired);
        }
        let emails: Vec<GitHubEmail> = email_response
            .json()
            .await
            .map_err(OAuthServiceError::Request)?;
        let email = emails
            .into_iter()
            .find(|candidate| candidate.primary && candidate.verified)
            .ok_or(OAuthServiceError::VerifiedEmailRequired)?
            .email;
        let email =
            normalize_email(&email).map_err(|_| OAuthServiceError::InvalidProviderResponse)?;
        let display_name = profile.name.as_deref().unwrap_or(&profile.login);

        Ok(OAuthIdentityProfile {
            provider: OAuthProvider::GitHub,
            subject: profile.id.to_string(),
            display_name: normalized_display_name(Some(display_name), "GitHub-Sammler"),
            email,
        })
    }

    fn github_get(&self, url: &str, access_token: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .bearer_auth(access_token)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
    }
}

fn require_non_empty(value: String) -> Result<String, OAuthServiceError> {
    if value.trim().is_empty() {
        Err(OAuthServiceError::InvalidProviderResponse)
    } else {
        Ok(value)
    }
}

fn normalized_display_name(value: Option<&str>, fallback: &str) -> String {
    let value = value.map(str::trim).filter(|value| value.len() >= 2);
    value
        .unwrap_or(fallback)
        .chars()
        .take(100)
        .collect::<String>()
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct GoogleUserInfo {
    sub: String,
    email: String,
    #[serde(default)]
    email_verified: bool,
    name: Option<String>,
}

#[derive(Deserialize)]
struct GitHubUser {
    id: u64,
    login: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum::routing::{get, post};
    use serde_json::json;

    fn credentials() -> OAuthCredentials {
        OAuthCredentials {
            client_id: "client-id".to_string(),
            client_secret: "client-secret".to_string(),
        }
    }

    async fn mock_provider_base() -> String {
        let app = axum::Router::new()
            .route(
                "/token",
                post(|| async { Json(json!({ "access_token": "provider-access-token" })) }),
            )
            .route(
                "/google/user",
                get(|| async {
                    Json(json!({
                        "sub": "google-subject-17",
                        "email": " Collector@Example.COM ",
                        "email_verified": true,
                        "name": "  Google Collector  "
                    }))
                }),
            )
            .route(
                "/google/unverified",
                get(|| async {
                    Json(json!({
                        "sub": "google-subject-18",
                        "email": "unverified@example.com",
                        "email_verified": false
                    }))
                }),
            )
            .route(
                "/github/user",
                get(|| async {
                    Json(json!({
                        "id": 4242,
                        "login": "collector-login",
                        "name": null
                    }))
                }),
            )
            .route(
                "/github/emails",
                get(|| async {
                    Json(json!([
                        { "email": "primary-unverified@example.com", "primary": true, "verified": false },
                        { "email": "secondary@example.com", "primary": false, "verified": true },
                        { "email": "Primary@Example.COM", "primary": true, "verified": true }
                    ]))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn test_service(provider: OAuthProvider, base: &str, user_path: &str) -> OAuthService {
        let client = ProviderClient {
            credentials: credentials(),
            authorize_url: format!("{base}/authorize"),
            token_url: format!("{base}/token"),
            user_url: format!("{base}{user_path}"),
            emails_url: (provider == OAuthProvider::GitHub)
                .then(|| format!("{base}/github/emails")),
            scopes: match provider {
                OAuthProvider::Google => &["openid", "email", "profile"],
                OAuthProvider::GitHub => &["read:user", "user:email"],
            },
        };
        OAuthService {
            http: reqwest::Client::new(),
            google: (provider == OAuthProvider::Google).then_some(client.clone()),
            github: (provider == OAuthProvider::GitHub).then_some(client),
        }
    }

    #[test]
    fn authorization_url_contains_only_expected_oauth_parameters() {
        let service = OAuthService::production(Some(credentials()), None);
        let url = service
            .authorization_url(
                OAuthProvider::Google,
                "https://lilly.example/api/v1/auth/oauth/google/callback",
                "state-value",
                "challenge-value",
            )
            .unwrap();
        let url = Url::parse(&url).unwrap();
        let pairs = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(pairs.get("state").map(AsRef::as_ref), Some("state-value"));
        assert_eq!(
            pairs.get("code_challenge_method").map(AsRef::as_ref),
            Some("S256")
        );
        assert_eq!(
            pairs.get("scope").map(AsRef::as_ref),
            Some("openid email profile")
        );
        assert!(!url.as_str().contains("client-secret"));
    }

    #[test]
    fn disabled_provider_cannot_build_authorization_url() {
        let service = OAuthService::disabled();
        assert!(matches!(
            service.authorization_url(OAuthProvider::GitHub, "https://example.test", "s", "c"),
            Err(OAuthServiceError::ProviderDisabled)
        ));
    }

    #[test]
    fn display_names_are_trimmed_bounded_and_have_a_fallback() {
        assert_eq!(
            normalized_display_name(Some("  Holger  "), "Fallback"),
            "Holger"
        );
        assert_eq!(normalized_display_name(Some(""), "Fallback"), "Fallback");
        assert_eq!(
            normalized_display_name(Some(&"x".repeat(120)), "Fallback")
                .chars()
                .count(),
            100
        );
    }

    #[tokio::test]
    async fn google_exchange_requires_and_normalizes_a_verified_email() {
        let base = mock_provider_base().await;
        let service = test_service(OAuthProvider::Google, &base, "/google/user");

        let profile = service
            .exchange_profile(
                OAuthProvider::Google,
                "authorization-code",
                "https://lilly.test/callback",
                "pkce-verifier",
            )
            .await
            .unwrap();

        assert_eq!(profile.subject, "google-subject-17");
        assert_eq!(profile.email, "collector@example.com");
        assert_eq!(profile.display_name, "Google Collector");
    }

    #[tokio::test]
    async fn google_exchange_rejects_an_unverified_email() {
        let base = mock_provider_base().await;
        let service = test_service(OAuthProvider::Google, &base, "/google/unverified");

        let result = service
            .exchange_profile(
                OAuthProvider::Google,
                "authorization-code",
                "https://lilly.test/callback",
                "pkce-verifier",
            )
            .await;

        assert!(matches!(
            result,
            Err(OAuthServiceError::VerifiedEmailRequired)
        ));
    }

    #[tokio::test]
    async fn github_exchange_selects_only_the_primary_verified_email() {
        let base = mock_provider_base().await;
        let service = test_service(OAuthProvider::GitHub, &base, "/github/user");

        let profile = service
            .exchange_profile(
                OAuthProvider::GitHub,
                "authorization-code",
                "https://lilly.test/callback",
                "pkce-verifier",
            )
            .await
            .unwrap();

        assert_eq!(profile.subject, "4242");
        assert_eq!(profile.email, "primary@example.com");
        assert_eq!(profile.display_name, "collector-login");
    }
}
