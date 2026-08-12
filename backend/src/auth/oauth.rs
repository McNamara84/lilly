use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngExt as _;
use sha2::{Digest, Sha256};

pub const OAUTH_FLOW_COOKIE: &str = "oauth_flow_binding";
pub const OAUTH_LINK_COOKIE: &str = "oauth_pending_link";
pub const OAUTH_COOKIE_PATH: &str = "/api/v1/auth/oauth";
pub const OAUTH_TTL_SECONDS: i64 = 600;
const LINK_CONFIRMATION_CONTEXT: &[u8] = b"lilly-oauth-link-confirmation\0";

#[derive(Debug)]
pub struct OAuthFlowSecrets {
    pub state: String,
    pub state_hash: String,
    pub browser_binding: String,
    pub browser_binding_hash: String,
    pub pkce_verifier: String,
    pub pkce_challenge: String,
}

#[must_use]
pub fn generate_flow_secrets(browser_binding: String) -> OAuthFlowSecrets {
    let state = random_urlsafe_token();
    let pkce_verifier = random_urlsafe_token();
    let pkce_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce_verifier.as_bytes()));

    OAuthFlowSecrets {
        state_hash: hash_secret(&state),
        browser_binding_hash: hash_secret(&browser_binding),
        state,
        browser_binding,
        pkce_verifier,
        pkce_challenge,
    }
}

#[must_use]
pub fn random_urlsafe_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[must_use]
pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

#[must_use]
pub fn link_confirmation_token(pending_link_secret: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(LINK_CONFIRMATION_CONTEXT);
    digest.update(pending_link_secret.as_bytes());
    URL_SAFE_NO_PAD.encode(digest.finalize())
}

#[must_use]
pub fn constant_time_secret_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[must_use]
pub fn short_lived_cookie(name: &str, value: String, secure: bool) -> Cookie<'static> {
    Cookie::build((name.to_string(), value))
        .path(OAUTH_COOKIE_PATH)
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(OAUTH_TTL_SECONDS))
        .build()
}

#[must_use]
pub fn clear_short_lived_cookie(name: &str, secure: bool) -> Cookie<'static> {
    Cookie::build((name.to_string(), String::new()))
        .path(OAUTH_COOKIE_PATH)
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::ZERO)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_flow_values_are_urlsafe_and_distinct() {
        let flow = generate_flow_secrets(random_urlsafe_token());
        for value in [
            &flow.state,
            &flow.browser_binding,
            &flow.pkce_verifier,
            &flow.pkce_challenge,
        ] {
            assert!(value.len() >= 43);
            assert!(
                value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()
                        || character == '-'
                        || character == '_')
            );
        }
        assert_ne!(flow.state, flow.browser_binding);
        assert_ne!(flow.state_hash, flow.browser_binding_hash);
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier() {
        let flow = generate_flow_secrets(random_urlsafe_token());
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(flow.pkce_verifier.as_bytes()));
        assert_eq!(flow.pkce_challenge, expected);
    }

    #[test]
    fn oauth_cookie_is_short_lived_and_hardened() {
        let cookie = short_lived_cookie(OAUTH_FLOW_COOKIE, "secret".to_string(), true);
        assert_eq!(cookie.path(), Some(OAUTH_COOKIE_PATH));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert!(cookie.http_only().unwrap_or(false));
        assert!(cookie.secure().unwrap_or(false));
        assert_eq!(
            cookie.max_age(),
            Some(time::Duration::seconds(OAUTH_TTL_SECONDS))
        );
    }

    #[test]
    fn link_confirmation_token_is_bound_to_the_pending_secret() {
        let first = link_confirmation_token("first-pending-link");
        let second = link_confirmation_token("second-pending-link");

        assert_ne!(first, second);
        assert!(constant_time_secret_eq(&first, &first));
        assert!(!constant_time_secret_eq(&first, &second));
        assert!(!constant_time_secret_eq(&first, "short"));
    }
}
