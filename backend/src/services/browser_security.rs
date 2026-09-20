//! Browser-facing protections for the JSON API: defensive response headers and a same-origin
//! check for state-changing requests.
//!
//! Sessions live in `SameSite=Lax` cookies, which already keeps browsers from attaching them to
//! cross-site POST/PUT/PATCH/DELETE requests. The `Origin` check below is a second, independent
//! layer so a future cookie or CORS misconfiguration cannot silently re-enable cross-site writes.

use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::error::AppError;
use crate::routes::AppState;

const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const DEV_ORIGINS: [&str; 3] = [
    "http://localhost",
    "http://localhost:80",
    "http://localhost:5173",
];

/// Origins allowed to send state-changing browser requests: the public application origin, plus the
/// local development origins for non-production (non-`Secure`) installations only.
pub fn allowed_origins(app_base_url: &str, cookie_secure: bool) -> Vec<String> {
    let mut origins = vec![app_base_url.trim_end_matches('/').to_ascii_lowercase()];
    if !cookie_secure {
        origins.extend(DEV_ORIGINS.iter().map(|origin| (*origin).to_string()));
    }
    origins
}

const fn is_state_changing(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn origin_is_allowed(request: &Request, allowed: &[String]) -> bool {
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        return origin
            .to_str()
            .is_ok_and(|origin| allowed.iter().any(|a| a.eq_ignore_ascii_case(origin)));
    }
    // Non-browser clients send no `Origin`. Browsers that omit it still announce cross-site
    // requests through `Sec-Fetch-Site`.
    request
        .headers()
        .get(SEC_FETCH_SITE)
        .is_none_or(|site| site.as_bytes() != b"cross-site")
}

/// Reject state-changing requests that a browser reports as coming from another origin.
pub async fn enforce_same_origin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    if is_state_changing(request.method()) {
        let allowed = allowed_origins(&state.inner.app_base_url, state.inner.cookie_secure);
        if !origin_is_allowed(&request, &allowed) {
            return Err(AppError::Forbidden {
                message: "Cross-origin request rejected".to_string(),
                code: Some("CROSS_ORIGIN_REQUEST_REJECTED".to_string()),
            });
        }
    }
    Ok(next.run(request).await)
}

/// Add defensive headers to every API response. Values already set by a handler are kept.
pub async fn add_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
        ),
    ] {
        headers
            .entry(name)
            .or_insert_with(|| HeaderValue::from_static(value));
    }
    response
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::StatusCode;

    use super::*;

    fn request(method: Method, origin: Option<&str>, fetch_site: Option<&str>) -> Request {
        let mut builder = Request::builder().method(method).uri("/api/v1/x");
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        if let Some(site) = fetch_site {
            builder = builder.header(SEC_FETCH_SITE, site);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn development_origins_are_only_allowed_without_secure_cookies() {
        assert_eq!(
            allowed_origins("https://lilly.example/", true),
            vec!["https://lilly.example".to_string()]
        );
        let development = allowed_origins("http://localhost", false);
        assert!(development.contains(&"http://localhost:5173".to_string()));
    }

    #[test]
    fn matching_origin_is_accepted_case_insensitively() {
        let allowed = allowed_origins("https://lilly.example", true);
        let ok = request(Method::POST, Some("https://LILLY.example"), None);
        assert!(origin_is_allowed(&ok, &allowed));
    }

    #[test]
    fn foreign_null_and_subdomain_origins_are_rejected() {
        let allowed = allowed_origins("https://lilly.example", true);
        for origin in [
            "https://evil.example",
            "null",
            "https://lilly.example.evil.example",
            "http://lilly.example",
            "https://sub.lilly.example",
        ] {
            let request = request(Method::POST, Some(origin), Some("same-origin"));
            assert!(!origin_is_allowed(&request, &allowed), "{origin}");
        }
    }

    #[test]
    fn missing_origin_is_only_accepted_when_not_reported_cross_site() {
        let allowed = allowed_origins("https://lilly.example", true);
        assert!(origin_is_allowed(
            &request(Method::POST, None, None),
            &allowed
        ));
        assert!(origin_is_allowed(
            &request(Method::POST, None, Some("same-origin")),
            &allowed
        ));
        assert!(!origin_is_allowed(
            &request(Method::POST, None, Some("cross-site")),
            &allowed
        ));
    }

    #[test]
    fn only_unsafe_methods_are_checked() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!is_state_changing(&method));
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(is_state_changing(&method));
        }
    }

    #[tokio::test]
    async fn responses_get_defensive_headers_without_overriding_handlers() {
        use axum::routing::get;
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route("/plain", get(|| async { "ok" }))
            .route(
                "/cached",
                get(|| async { ([(header::REFERRER_POLICY, "same-origin")], "ok") }),
            )
            .layer(axum::middleware::from_fn(add_security_headers));

        let plain = app
            .clone()
            .oneshot(Request::get("/plain").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(plain.status(), StatusCode::OK);
        assert_eq!(plain.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(plain.headers()[header::REFERRER_POLICY], "no-referrer");
        assert!(
            plain.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'")
        );

        let cached = app
            .oneshot(Request::get("/cached").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(cached.headers()[header::REFERRER_POLICY], "same-origin");
        assert_eq!(cached.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }

    fn test_state(app_base_url: &str, cookie_secure: bool) -> AppState {
        use std::path::PathBuf;
        use std::sync::Arc;

        use lilly_importer_core::adapter::AdapterRegistry;

        use crate::routes::AppStateInner;
        use crate::services::email::EmailService;

        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect_lazy("mysql://test:test@127.0.0.1:9/lilly")
            .unwrap();
        let media = "/tmp/lilly-browser-security-test";
        AppState {
            inner: Arc::new(AppStateInner {
                pool,
                jwt_secret: "browser-security-test-secret".to_string(),
                jwt_access_expiry: 900,
                jwt_refresh_expiry: 2_592_000,
                password_reset_ttl_seconds: 3_600,
                email_service: EmailService::Log {
                    from: "test@example.test".to_string(),
                },
                app_base_url: app_base_url.to_string(),
                cookie_secure,
                oauth_service: crate::services::oauth::OAuthService::disabled(),
                privacy_policy_version: "test-v1".to_string(),
                adapter_registry: AdapterRegistry::new(),
                media_path: PathBuf::from(media),
                media_url_prefix: "/media".to_string(),
                photo_upload_config: crate::config::PhotoUploadConfig::default(),
                media_storage: crate::services::media::MediaStorage::new(std::path::Path::new(
                    media,
                )),
                erasure_ledger: crate::services::account_erasure::ErasureLedger::new(
                    "/tmp/lilly-browser-security-test-erasure-ledger",
                ),
                import_scheduler_config: crate::services::import_scheduler::ImportSchedulerConfig {
                    enabled: false,
                    schedule: "0 10 6 * * Sat *".to_string(),
                    timezone: "Europe/Berlin".to_string(),
                    adapters: Vec::new(),
                },
                request_security: crate::services::rate_limit::RequestSecurity::for_tests(),
            }),
        }
    }

    #[tokio::test]
    async fn state_changing_requests_from_other_origins_never_reach_the_handler() {
        use axum::routing::{get, post};
        use tower::ServiceExt;

        let state = test_state("https://lilly.example", true);
        let app = axum::Router::new()
            .route("/write", post(|| async { "written" }))
            .route("/read", get(|| async { "read" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                enforce_same_origin,
            ));

        let send = |method: Method, uri: &'static str, origin: Option<&'static str>| {
            let app = app.clone();
            async move {
                let mut builder = Request::builder().method(method).uri(uri);
                if let Some(origin) = origin {
                    builder = builder.header(header::ORIGIN, origin);
                }
                app.oneshot(builder.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
            }
        };

        let foreign = send(Method::POST, "/write", Some("https://evil.example")).await;
        assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(foreign.into_body(), 4_096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "CROSS_ORIGIN_REQUEST_REJECTED");

        let same = send(Method::POST, "/write", Some("https://lilly.example")).await;
        assert_eq!(same.status(), StatusCode::OK);
        let no_origin = send(Method::POST, "/write", None).await;
        assert_eq!(no_origin.status(), StatusCode::OK);
        let read = send(Method::GET, "/read", Some("https://evil.example")).await;
        assert_eq!(read.status(), StatusCode::OK);
    }
}
