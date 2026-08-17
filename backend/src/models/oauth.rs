use std::fmt;
use std::str::FromStr;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthProvider {
    Google,
    GitHub,
}

impl OAuthProvider {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::GitHub => "github",
        }
    }
}

impl fmt::Display for OAuthProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for OAuthProvider {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "google" => Ok(Self::Google),
            "github" => Ok(Self::GitHub),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OAuthIntent {
    Login,
    Register,
    Reauth,
}

impl OAuthIntent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Register => "register",
            Self::Reauth => "reauth",
        }
    }
}

impl FromStr for OAuthIntent {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "login" => Ok(Self::Login),
            "register" => Ok(Self::Register),
            "reauth" => Ok(Self::Reauth),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct OAuthStartRequest {
    pub intent: OAuthIntent,
    #[serde(default)]
    pub privacy_consent: bool,
    pub privacy_policy_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OAuthStartResponse {
    pub authorization_url: String,
}

#[derive(Debug, Serialize)]
pub struct AuthOptionsResponse {
    pub privacy_policy: PrivacyPolicyOption,
    pub oauth: OAuthAvailability,
}

#[derive(Debug, Serialize)]
pub struct PrivacyPolicyOption {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct OAuthAvailability {
    pub google: bool,
    pub github: bool,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthIdentityProfile {
    pub provider: OAuthProvider,
    pub subject: String,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct OAuthFlowRow {
    pub browser_binding_hash: String,
    pub provider: String,
    pub intent: String,
    pub reauth_user_id: Option<u32>,
    pub pkce_verifier: String,
    pub privacy_policy_version: Option<String>,
    pub consented_at: Option<NaiveDateTime>,
    pub expires_at: NaiveDateTime,
    pub consumed_at: Option<NaiveDateTime>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PendingOAuthLinkRow {
    pub provider: String,
    pub provider_subject: String,
    pub verified_email: String,
    pub expires_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct PendingOAuthLinkResponse {
    pub pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<NaiveDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_token: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PrivacyConsentResponse {
    pub policy_version: String,
    pub consented_at: NaiveDateTime,
    pub registration_method: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_allowlist_is_exact() {
        assert_eq!(OAuthProvider::from_str("google"), Ok(OAuthProvider::Google));
        assert_eq!(OAuthProvider::from_str("github"), Ok(OAuthProvider::GitHub));
        assert!(OAuthProvider::from_str("GitHub").is_err());
        assert!(OAuthProvider::from_str("https://example.com").is_err());
    }

    #[test]
    fn intent_allowlist_is_exact() {
        assert_eq!(OAuthIntent::from_str("login"), Ok(OAuthIntent::Login));
        assert_eq!(OAuthIntent::from_str("register"), Ok(OAuthIntent::Register));
        assert_eq!(OAuthIntent::from_str("reauth"), Ok(OAuthIntent::Reauth));
        assert!(OAuthIntent::from_str("link").is_err());
    }
}
