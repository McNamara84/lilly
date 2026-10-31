//! Generated API contract and optional, self-hosted API documentation.

use std::collections::BTreeMap;

use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;
use axum::{Router, middleware};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa_swagger_ui::{Config, SwaggerUi};

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    #[schema(pattern = "^(ok|degraded)$")]
    pub status: String,
    pub database: bool,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct PhotoUpload {
    /// One JPEG, PNG or WebP file. The server decodes and normalizes the image.
    #[schema(value_type = String, format = Binary)]
    pub photo: Vec<u8>,
}

#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
#[allow(dead_code)]
pub struct BinaryImage(Vec<u8>);

pub fn configure(spec: &mut utoipa::openapi::OpenApi) {
    spec.info.title = "LILLY API".to_string();
    spec.info.version = "1.0.0".to_string();
    spec.info.description = Some(
        "Shared REST API for the LILLY PWA. Authentication uses HttpOnly cookies. \
         Administrative operations require the Admin role; user resources enforce ownership \
         and visibility. See docs/api.md for conventions and compatibility policy."
            .to_string(),
    );
    let components = spec.components.get_or_insert_with(Default::default);
    for (name, cookie) in [
        ("accessCookie", "access_token"),
        ("refreshCookie", "refresh_token"),
        (
            "recoveryCookie",
            crate::services::account_erasure::RECOVERY_COOKIE,
        ),
        ("oauthFlowCookie", crate::auth::oauth::OAUTH_FLOW_COOKIE),
        ("oauthLinkCookie", crate::auth::oauth::OAUTH_LINK_COOKIE),
    ] {
        components.add_security_scheme(
            name,
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(cookie))),
        );
    }
}

pub fn export() -> Result<String, serde_json::Error> {
    // Value uses sorted maps, making output independent of handler registration order.
    let value = serde_json::to_value(crate::api::specification())?;
    serde_json::to_string_pretty(&value).map(|json| format!("{json}\n"))
}

pub fn documentation(enabled: bool) -> Router {
    if !enabled {
        return Router::new();
    }
    Router::new()
        .merge(
            SwaggerUi::new("/api/v1/docs")
                .url("/api/v1/openapi.json", crate::api::specification())
                .config(Config::default().validator_url("").with_credentials(true)),
        )
        .layer(middleware::from_fn(documentation_headers))
}

async fn documentation_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; font-src 'self'; connect-src 'self'; \
             frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
    );
    response
}

/// Give extractor and router rejections the same JSON envelope as application errors.
/// Existing JSON responses and status codes are preserved.
pub async fn normalize_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    if !response.status().is_client_error() && !response.status().is_server_error() {
        return response;
    }
    if response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"))
    {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let message = if parts.status.is_server_error() {
        "Internal server error".to_string()
    } else {
        match to_bytes(body, 65_536).await {
            Ok(bytes) if !bytes.is_empty() => String::from_utf8_lossy(&bytes).into_owned(),
            _ => parts
                .status
                .canonical_reason()
                .unwrap_or("Request failed")
                .to_string(),
        }
    };
    let json = serde_json::json!({ "error": message }).to_string();
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Response::from_parts(parts, Body::from(json))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use axum::http::{Method, Request, StatusCode};
    use axum::routing::get;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::auth::jwt;
    use crate::routes::admin::tests::test_state;

    fn assert_schema(spec: &Value, path: &str, method: &str, status: u16, value: &Value) {
        let mut schema = spec["paths"][path][method]["responses"][status.to_string()]
            ["content"]["application/json"]["schema"].clone();
        assert!(
            schema.is_object(),
            "Missing response schema: {method} {path} {status}"
        );
        schema["components"] = spec["components"].clone();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let errors = validator
            .iter_errors(value)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{method} {path} {status}: {errors:?}");
    }

    fn check_refs(node: &Value, root: &Value) {
        match node {
            Value::Object(fields) => {
                if let Some(reference) = fields.get("$ref").and_then(Value::as_str) {
                    assert!(reference.starts_with("#/"));
                    assert!(
                        root.pointer(&reference[1..]).is_some(),
                        "Unresolved {reference}"
                    );
                }
                for value in fields.values() {
                    check_refs(value, root);
                }
            }
            Value::Array(values) => {
                for value in values {
                    check_refs(value, root);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn contract_has_resolved_schemas_unique_operations_and_cookie_security() {
        let spec: Value = serde_json::from_str(&export().unwrap()).unwrap();
        assert_eq!(spec["openapi"], "3.1.0");
        let meta_schema: Value =
            serde_json::from_str(include_str!("../tests/fixtures/openapi-3.1-schema.json"))
                .unwrap();
        let validator = jsonschema::validator_for(&meta_schema).unwrap();
        let errors = validator
            .iter_errors(&spec)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "Invalid OpenAPI 3.1: {errors:?}");
        let mut operations = BTreeSet::new();
        for (path, item) in spec["paths"].as_object().unwrap() {
            assert!(path.starts_with("/api/v1/"));
            for (method, operation) in item.as_object().unwrap() {
                assert!(operations.insert(operation["operationId"].as_str().unwrap()));
                assert!(
                    operation["responses"]
                        .as_object()
                        .is_some_and(|responses| !responses.is_empty())
                );
                if path.starts_with("/api/v1/admin/") {
                    assert_eq!(operation["security"], json!([{ "accessCookie": [] }]));
                    assert!(
                        operation["responses"].get("401").is_some(),
                        "{method} {path}"
                    );
                    assert!(
                        operation["responses"].get("403").is_some(),
                        "{method} {path}"
                    );
                }
                let placeholders = path
                    .split('/')
                    .filter_map(|part| {
                        part.strip_prefix('{')
                            .and_then(|part| part.strip_suffix('}'))
                    })
                    .collect::<BTreeSet<_>>();
                let parameters = operation
                    .get("parameters")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let path_names = parameters
                    .iter()
                    .filter(|p| p["in"] == "path")
                    .map(|p| p["name"].as_str().unwrap())
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    placeholders, path_names,
                    "Path parameter mismatch: {method} {path}"
                );
            }
        }
        assert!(operations.contains("collection_sync_collection"));
        assert!(operations.contains("account_erasure_admin_jobs"));
        assert!(operations.contains("trades_complete_trade"));
        check_refs(&spec, &spec);
        for schema in spec["components"]["schemas"].as_object().unwrap().values() {
            jsonschema::meta::validate(schema).unwrap();
        }
        assert_eq!(
            spec["components"]["securitySchemes"]["accessCookie"]["in"],
            "cookie"
        );
        assert_eq!(
            spec["components"]["securitySchemes"]["accessCookie"]["name"],
            "access_token"
        );
        assert_eq!(export().unwrap(), export().unwrap());
    }

    #[tokio::test]
    async fn every_registered_admin_operation_enforces_roles_and_documented_errors() {
        let spec = serde_json::to_value(crate::api::specification()).unwrap();
        let app = crate::api::application(test_state(), false);
        for (path, item) in spec["paths"].as_object().unwrap() {
            if !path.starts_with("/api/v1/admin/") {
                continue;
            }
            let uri = path.replace("{id}", "1").replace("{slug}", "test");
            for method in item.as_object().unwrap().keys() {
                for (role, expected) in [(None, 401), (Some("user"), 403)] {
                    let mut request = Request::builder()
                        .method(Method::from_bytes(method.to_uppercase().as_bytes()).unwrap())
                        .uri(&uri)
                        .header("content-type", "application/json");
                    if let Some(role) = role {
                        let token = jwt::create_token(
                            7,
                            "Contract Tester",
                            role,
                            "admin-route-test-secret",
                            3600,
                        )
                        .unwrap();
                        request = request.header("cookie", format!("access_token={token}"));
                    }
                    let response = app
                        .clone()
                        .oneshot(request.body(Body::from("{}")).unwrap())
                        .await
                        .unwrap();
                    assert_eq!(response.status().as_u16(), expected, "{method} {path}");
                    let value: Value = serde_json::from_slice(
                        &to_bytes(response.into_body(), 65536).await.unwrap(),
                    )
                    .unwrap();
                    assert_schema(&spec, path, method, expected, &value);
                }
            }
        }
    }

    #[tokio::test]
    async fn real_public_and_admin_responses_match_the_contract() {
        let spec = serde_json::to_value(crate::api::specification()).unwrap();
        let app = crate::api::application(test_state(), false);
        for (path, admin) in [
            ("/api/v1/auth/options", false),
            ("/api/v1/media/photo-policy", false),
            ("/api/v1/admin/adapters", true),
            ("/api/v1/admin/import/schedule", true),
        ] {
            let mut request = Request::builder().uri(path);
            if admin {
                let token = jwt::create_token(7, "Admin", "admin", "admin-route-test-secret", 3600)
                    .unwrap();
                request = request.header("cookie", format!("access_token={token}"));
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let value: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap())
                    .unwrap();
            assert_schema(&spec, path, "get", 200, &value);
        }
    }

    #[tokio::test]
    async fn documentation_is_opt_in_self_hosted_and_has_scoped_headers() {
        let disabled = crate::api::application(test_state(), false);
        for path in ["/api/v1/docs", "/api/v1/openapi.json"] {
            let response = disabled
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        let enabled = crate::api::application(test_state(), true);
        for path in [
            "/api/v1/docs/",
            "/api/v1/docs/swagger-ui-bundle.js",
            "/api/v1/docs/swagger-initializer.js",
            "/api/v1/openapi.json",
        ] {
            let response = enabled
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert!(
                response.headers()[header::CONTENT_SECURITY_POLICY]
                    .to_str()
                    .unwrap()
                    .contains("script-src 'self'")
            );
            let bytes = to_bytes(response.into_body(), 10_000_000).await.unwrap();
            if path.ends_with("swagger-initializer.js") {
                let script = String::from_utf8(bytes.to_vec()).unwrap();
                let script = script.split_whitespace().collect::<String>();
                assert!(script.contains("\"validatorUrl\":\"\""));
                assert!(script.contains("\"withCredentials\":true"));
            }
        }
        let response = enabled
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/options")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.headers()[header::CONTENT_SECURITY_POLICY],
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'"
        );
    }

    #[tokio::test]
    async fn normalization_preserves_status_headers_and_json_but_hides_internal_errors() {
        let app = Router::new()
            .route(
                "/text",
                get(|| async { (StatusCode::UNPROCESSABLE_ENTITY, "Invalid JSON") }),
            )
            .route("/empty", get(|| async { StatusCode::NOT_FOUND }))
            .route(
                "/large",
                get(|| async { (StatusCode::BAD_REQUEST, "x".repeat(70000)) }),
            )
            .route(
                "/internal",
                get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "secret database details") }),
            )
            .route(
                "/json",
                get(|| async {
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        [(header::RETRY_AFTER, "10")],
                        axum::Json(json!({"error":"Limited", "code":"RATE_LIMITED"})),
                    )
                }),
            )
            .route("/ok", get(|| async { "unchanged" }))
            .layer(middleware::from_fn(normalize_errors));
        for (path, status, message) in [
            ("/text", 422, "Invalid JSON"),
            ("/empty", 404, "Not Found"),
            ("/large", 400, "Bad Request"),
            ("/internal", 500, "Internal server error"),
            ("/json", 429, "Limited"),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
            if path == "/json" {
                assert_eq!(response.headers()[header::RETRY_AFTER], "10");
            }
            let value: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap())
                    .unwrap();
            assert_eq!(value["error"], message);
            if path == "/json" {
                assert_eq!(value["code"], "RATE_LIMITED");
            }
        }
        let response = app
            .oneshot(Request::builder().uri("/ok").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            to_bytes(response.into_body(), 65536).await.unwrap(),
            "unchanged"
        );
    }
}
