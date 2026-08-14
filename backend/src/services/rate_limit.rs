use std::collections::VecDeque;
use std::hash::Hash;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use hashlink::LruCache;
use ipnet::IpNet;
use sha2::{Digest, Sha256};

use crate::auth::jwt;
use crate::config::{RateLimitConfig, RateLimitRule};
use crate::error::AppError;
use crate::routes::AppState;

/// The socket peer when Axum is served with connect information enabled.
/// Direct router tests do not have transport metadata, so the address is
/// deliberately optional and falls back to an anonymous client identity.
#[derive(Clone, Copy, Debug)]
pub struct PeerAddress(pub Option<SocketAddr>);

impl<S> FromRequestParts<S> for PeerAddress
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|connect_info| connect_info.0),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateLimitPolicy {
    Register,
    LoginClient,
    LoginAccount,
    ResendVerification,
    PasswordResetRequest,
    PasswordResetConfirm,
    OAuthStart,
    OAuthCallback,
    Refresh,
    PublicApi,
    AuthenticatedApi,
}

impl RateLimitPolicy {
    const fn name(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::LoginClient => "login_client",
            Self::LoginAccount => "login_account",
            Self::ResendVerification => "resend_verification",
            Self::PasswordResetRequest => "password_reset_request",
            Self::PasswordResetConfirm => "password_reset_confirm",
            Self::OAuthStart => "oauth_start",
            Self::OAuthCallback => "oauth_callback",
            Self::Refresh => "refresh",
            Self::PublicApi => "public_api",
            Self::AuthenticatedApi => "authenticated_api",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RateLimitKey {
    Client(String),
    Account(String),
    Token(String),
    User(String),
}

impl RateLimitKey {
    const fn kind(&self) -> &'static str {
        match self {
            Self::Client(_) => "client",
            Self::Account(_) => "account",
            Self::Token(_) => "token",
            Self::User(_) => "user",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BucketKey {
    policy: RateLimitPolicy,
    key: RateLimitKey,
}

// Keeps the in-memory limiter bounded even when clients submit attacker-controlled
// identifiers such as random password-reset tokens. Active buckets are retained
// preferentially by the LRU policy.
const MAX_RATE_LIMIT_BUCKETS: usize = 20_000;

#[derive(Clone)]
struct RateLimiter {
    attempts: Arc<tokio::sync::Mutex<LruCache<BucketKey, VecDeque<Instant>>>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self::with_capacity(MAX_RATE_LIMIT_BUCKETS)
    }

    fn with_capacity(max_buckets: usize) -> Self {
        assert!(
            max_buckets > 0,
            "rate-limit bucket capacity must be positive"
        );
        Self {
            attempts: Arc::new(tokio::sync::Mutex::new(LruCache::new(max_buckets))),
        }
    }

    async fn check(
        &self,
        policy: RateLimitPolicy,
        key: RateLimitKey,
        rule: RateLimitRule,
    ) -> Result<(), u64> {
        self.check_at(policy, key, rule, Instant::now()).await
    }

    async fn check_at(
        &self,
        policy: RateLimitPolicy,
        key: RateLimitKey,
        rule: RateLimitRule,
        now: Instant,
    ) -> Result<(), u64> {
        let window = Duration::from_secs(rule.window_seconds);
        let cutoff = now.checked_sub(window).unwrap_or(now);
        let mut attempts = self.attempts.lock().await;

        let bucket_key = BucketKey { policy, key };
        if !attempts.contains_key(&bucket_key) {
            attempts.insert(bucket_key.clone(), VecDeque::new());
        }
        let bucket = attempts
            .get_mut(&bucket_key)
            .expect("a rate-limit bucket was just inserted or already existed");
        while bucket.front().is_some_and(|attempt| *attempt <= cutoff) {
            bucket.pop_front();
        }

        if bucket.len() >= rule.max_requests {
            let retry_after = bucket.front().map_or(rule.window_seconds, |oldest| {
                let elapsed = now.saturating_duration_since(*oldest);
                window.saturating_sub(elapsed).as_secs().max(1)
            });
            return Err(retry_after);
        }
        bucket.push_back(now);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    fingerprint: String,
    pub address: Option<IpAddr>,
}

#[derive(Clone)]
pub struct RequestSecurity {
    limiter: RateLimiter,
    config: RateLimitConfig,
    trusted_proxies: Arc<Vec<IpNet>>,
    fingerprint_secret: Arc<str>,
}

impl RequestSecurity {
    #[must_use]
    pub fn new(
        config: RateLimitConfig,
        trusted_proxies: Vec<IpNet>,
        fingerprint_secret: &str,
    ) -> Self {
        Self {
            limiter: RateLimiter::new(),
            config,
            trusted_proxies: Arc::new(trusted_proxies),
            fingerprint_secret: Arc::from(fingerprint_secret),
        }
    }

    #[must_use]
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self::new(
            RateLimitConfig::default(),
            Vec::new(),
            "rate-limit-test-secret",
        )
    }

    #[must_use]
    pub fn client_identity(&self, headers: &HeaderMap, peer: Option<SocketAddr>) -> ClientIdentity {
        let address = resolve_client_ip(headers, peer, &self.trusted_proxies);
        let value = address.map_or_else(|| "unknown".to_string(), |ip| ip.to_string());
        ClientIdentity {
            fingerprint: self.fingerprint("client", &value),
            address,
        }
    }

    pub async fn enforce_client(
        &self,
        policy: RateLimitPolicy,
        client: &ClientIdentity,
    ) -> Result<(), AppError> {
        self.enforce(policy, RateLimitKey::Client(client.fingerprint.clone()))
            .await
    }

    pub async fn enforce_account(
        &self,
        policy: RateLimitPolicy,
        account: &str,
    ) -> Result<(), AppError> {
        self.enforce(
            policy,
            RateLimitKey::Account(self.fingerprint("account", account)),
        )
        .await
    }

    pub async fn enforce_token(
        &self,
        policy: RateLimitPolicy,
        token: &str,
    ) -> Result<(), AppError> {
        self.enforce(
            policy,
            RateLimitKey::Token(self.fingerprint("token", token)),
        )
        .await
    }

    pub async fn enforce_user(
        &self,
        policy: RateLimitPolicy,
        user_id: u32,
    ) -> Result<(), AppError> {
        self.enforce(
            policy,
            RateLimitKey::User(self.fingerprint("user", &user_id.to_string())),
        )
        .await
    }

    async fn enforce(&self, policy: RateLimitPolicy, key: RateLimitKey) -> Result<(), AppError> {
        let rule = self.rule(policy);
        if let Err(retry_after_seconds) = self.limiter.check(policy, key.clone(), rule).await {
            tracing::warn!(
                policy = policy.name(),
                key_kind = key.kind(),
                retry_after_seconds,
                "Request rate limit exceeded"
            );
            return Err(AppError::TooManyRequests {
                message: "Too many requests. Please try again later.".to_string(),
                code: "RATE_LIMITED".to_string(),
                retry_after_seconds,
            });
        }
        Ok(())
    }

    const fn rule(&self, policy: RateLimitPolicy) -> RateLimitRule {
        match policy {
            RateLimitPolicy::Register => self.config.register,
            RateLimitPolicy::LoginClient => self.config.login_client,
            RateLimitPolicy::LoginAccount => self.config.login_account,
            RateLimitPolicy::ResendVerification => self.config.resend_verification,
            RateLimitPolicy::PasswordResetRequest => self.config.password_reset_request,
            RateLimitPolicy::PasswordResetConfirm => self.config.password_reset_confirm,
            RateLimitPolicy::OAuthStart => self.config.oauth_start,
            RateLimitPolicy::OAuthCallback => self.config.oauth_callback,
            RateLimitPolicy::Refresh => self.config.refresh,
            RateLimitPolicy::PublicApi => self.config.public_api,
            RateLimitPolicy::AuthenticatedApi => self.config.authenticated_api,
        }
    }

    fn fingerprint(&self, context: &str, value: &str) -> String {
        let mut digest = Sha256::new();
        digest.update(self.fingerprint_secret.as_bytes());
        digest.update([0]);
        digest.update(context.as_bytes());
        digest.update([0]);
        digest.update(value.as_bytes());
        hex::encode(digest.finalize())
    }
}

pub async fn enforce_general_api_rate_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| *address);
    let client = state
        .inner
        .request_security
        .client_identity(request.headers(), peer);
    let jar = CookieJar::from_headers(request.headers());
    let authenticated_user_id = jar
        .get("access_token")
        .and_then(|cookie| jwt::validate_token(cookie.value(), &state.inner.jwt_secret).ok())
        .map(|claims| claims.sub);

    let result = if let Some(user_id) = authenticated_user_id {
        if let Err(error) = state
            .inner
            .request_security
            .enforce_client(RateLimitPolicy::AuthenticatedApi, &client)
            .await
        {
            Err(error)
        } else {
            state
                .inner
                .request_security
                .enforce_user(RateLimitPolicy::AuthenticatedApi, user_id)
                .await
        }
    } else {
        state
            .inner
            .request_security
            .enforce_client(RateLimitPolicy::PublicApi, &client)
            .await
    };

    match result {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}

fn resolve_client_ip(
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    trusted_proxies: &[IpNet],
) -> Option<IpAddr> {
    let peer_ip = peer.map(|address| address.ip())?;
    if !is_trusted(peer_ip, trusted_proxies) {
        return Some(peer_ip);
    }

    let forwarded = forwarded_chain(headers);
    if forwarded.is_empty() {
        return Some(peer_ip);
    }
    forwarded
        .iter()
        .rev()
        .find(|address| !is_trusted(**address, trusted_proxies))
        .copied()
        .or_else(|| forwarded.first().copied())
}

fn forwarded_chain(headers: &HeaderMap) -> Vec<IpAddr> {
    if let Some(value) = headers
        .get("forwarded")
        .and_then(|value| value.to_str().ok())
    {
        let chain = value
            .split(',')
            .filter_map(|element| {
                element.split(';').find_map(|parameter| {
                    let (name, value) = parameter.trim().split_once('=')?;
                    name.eq_ignore_ascii_case("for")
                        .then(|| parse_forwarded_ip(value))
                        .flatten()
                })
            })
            .collect::<Vec<_>>();
        if !chain.is_empty() {
            return chain;
        }
    }

    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        let chain = value
            .split(',')
            .filter_map(parse_forwarded_ip)
            .collect::<Vec<_>>();
        if !chain.is_empty() {
            return chain;
        }
    }

    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_forwarded_ip)
        .into_iter()
        .collect()
}

fn parse_forwarded_ip(value: &str) -> Option<IpAddr> {
    let value = value.trim().trim_matches('"');
    if value.eq_ignore_ascii_case("unknown") || value.starts_with('_') {
        return None;
    }
    value
        .parse::<IpAddr>()
        .ok()
        .or_else(|| value.parse::<SocketAddr>().ok().map(|address| address.ip()))
        .or_else(|| {
            value
                .strip_prefix('[')
                .and_then(|value| value.split_once(']'))
                .and_then(|(address, _)| address.parse().ok())
        })
}

fn is_trusted(address: IpAddr, trusted_proxies: &[IpNet]) -> bool {
    trusted_proxies
        .iter()
        .any(|network| network.contains(&address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn rule(max_requests: usize, window_seconds: u64) -> RateLimitRule {
        RateLimitRule {
            max_requests,
            window_seconds,
        }
    }

    #[tokio::test]
    async fn limiter_enforces_boundary_and_recovers_after_window() {
        let limiter = RateLimiter::new();
        let start = Instant::now();
        let key = RateLimitKey::Client("one".to_string());

        assert!(
            limiter
                .check_at(
                    RateLimitPolicy::LoginClient,
                    key.clone(),
                    rule(2, 60),
                    start
                )
                .await
                .is_ok()
        );
        assert!(
            limiter
                .check_at(
                    RateLimitPolicy::LoginClient,
                    key.clone(),
                    rule(2, 60),
                    start + Duration::from_secs(1),
                )
                .await
                .is_ok()
        );
        assert_eq!(
            limiter
                .check_at(
                    RateLimitPolicy::LoginClient,
                    key.clone(),
                    rule(2, 60),
                    start + Duration::from_secs(2),
                )
                .await,
            Err(58)
        );
        assert!(
            limiter
                .check_at(
                    RateLimitPolicy::LoginClient,
                    key,
                    rule(2, 60),
                    start + Duration::from_secs(61),
                )
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn limiter_separates_policies_and_keys() {
        let limiter = RateLimiter::new();
        let now = Instant::now();
        let first = RateLimitKey::Client("first".to_string());
        let second = RateLimitKey::Client("second".to_string());

        assert!(
            limiter
                .check_at(RateLimitPolicy::Register, first.clone(), rule(1, 60), now)
                .await
                .is_ok()
        );
        assert!(
            limiter
                .check_at(RateLimitPolicy::Register, first.clone(), rule(1, 60), now)
                .await
                .is_err()
        );
        assert!(
            limiter
                .check_at(RateLimitPolicy::Register, second, rule(1, 60), now)
                .await
                .is_ok()
        );
        assert!(
            limiter
                .check_at(RateLimitPolicy::LoginClient, first, rule(1, 60), now)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn limiter_only_prunes_the_bucket_being_checked() {
        let limiter = RateLimiter::with_capacity(4);
        let start = Instant::now();
        let first = RateLimitKey::Token("first".to_string());
        let second = RateLimitKey::Token("second".to_string());

        limiter
            .check_at(
                RateLimitPolicy::PasswordResetConfirm,
                first.clone(),
                rule(2, 60),
                start,
            )
            .await
            .unwrap();
        limiter
            .check_at(
                RateLimitPolicy::PasswordResetConfirm,
                second.clone(),
                rule(2, 60),
                start,
            )
            .await
            .unwrap();
        limiter
            .check_at(
                RateLimitPolicy::PasswordResetConfirm,
                first.clone(),
                rule(2, 60),
                start + Duration::from_secs(61),
            )
            .await
            .unwrap();

        let attempts = limiter.attempts.lock().await;
        let first_bucket = BucketKey {
            policy: RateLimitPolicy::PasswordResetConfirm,
            key: first,
        };
        let second_bucket = BucketKey {
            policy: RateLimitPolicy::PasswordResetConfirm,
            key: second,
        };
        assert_eq!(attempts.peek(&first_bucket).unwrap().len(), 1);
        assert_eq!(attempts.peek(&second_bucket).unwrap().len(), 1);
        assert_eq!(attempts.peek(&second_bucket).unwrap().front(), Some(&start));
    }

    #[tokio::test]
    async fn limiter_bounds_bucket_count_and_evicts_the_least_recently_used() {
        let limiter = RateLimiter::with_capacity(2);
        let start = Instant::now();
        let first = RateLimitKey::Token("first".to_string());
        let second = RateLimitKey::Token("second".to_string());
        let third = RateLimitKey::Token("third".to_string());

        for key in [&first, &second] {
            limiter
                .check_at(
                    RateLimitPolicy::PasswordResetConfirm,
                    key.clone(),
                    rule(3, 60),
                    start,
                )
                .await
                .unwrap();
        }
        limiter
            .check_at(
                RateLimitPolicy::PasswordResetConfirm,
                first.clone(),
                rule(3, 60),
                start + Duration::from_secs(1),
            )
            .await
            .unwrap();
        limiter
            .check_at(
                RateLimitPolicy::PasswordResetConfirm,
                third.clone(),
                rule(3, 60),
                start + Duration::from_secs(2),
            )
            .await
            .unwrap();

        let attempts = limiter.attempts.lock().await;
        assert_eq!(attempts.len(), 2);
        assert!(
            attempts
                .peek(&BucketKey {
                    policy: RateLimitPolicy::PasswordResetConfirm,
                    key: first,
                })
                .is_some()
        );
        assert!(
            attempts
                .peek(&BucketKey {
                    policy: RateLimitPolicy::PasswordResetConfirm,
                    key: second,
                })
                .is_none()
        );
        assert!(
            attempts
                .peek(&BucketKey {
                    policy: RateLimitPolicy::PasswordResetConfirm,
                    key: third,
                })
                .is_some()
        );
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_address() {
        let security = RequestSecurity::for_tests();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("198.51.100.9"));
        let peer = "192.0.2.20:1234".parse().unwrap();

        let identity = security.client_identity(&headers, Some(peer));

        assert_eq!(identity.address, Some("192.0.2.20".parse().unwrap()));
    }

    #[test]
    fn trusted_proxy_chain_selects_nearest_untrusted_hop() {
        let security = RequestSecurity::new(
            RateLimitConfig::default(),
            vec!["10.0.0.0/8".parse().unwrap()],
            "test-secret",
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.8, 203.0.113.7, 10.1.2.3"),
        );
        let peer = "10.2.3.4:443".parse().unwrap();

        let identity = security.client_identity(&headers, Some(peer));

        assert_eq!(identity.address, Some("203.0.113.7".parse().unwrap()));
    }

    #[test]
    fn forwarded_header_supports_ipv4_and_bracketed_ipv6() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            HeaderValue::from_static("for=192.0.2.60;proto=https, for=\"[2001:db8::1]:4711\""),
        );

        assert_eq!(
            forwarded_chain(&headers),
            vec![
                "192.0.2.60".parse::<IpAddr>().unwrap(),
                "2001:db8::1".parse::<IpAddr>().unwrap()
            ]
        );
    }

    #[test]
    fn malformed_forwarding_values_fall_back_to_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            HeaderValue::from_static("for=unknown, for=_hidden"),
        );
        headers.insert("x-forwarded-for", HeaderValue::from_static("not-an-ip"));
        headers.insert("x-real-ip", HeaderValue::from_static("198.51.100.12"));

        assert_eq!(
            forwarded_chain(&headers),
            vec!["198.51.100.12".parse::<IpAddr>().unwrap()]
        );
    }

    #[test]
    fn missing_peer_never_trusts_forwarding_headers() {
        let security = RequestSecurity::new(
            RateLimitConfig::default(),
            vec!["0.0.0.0/0".parse().unwrap()],
            "test-secret",
        );
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("198.51.100.9"));

        assert_eq!(security.client_identity(&headers, None).address, None);
    }
}
