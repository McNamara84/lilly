//! Built-in source adapters for LILLY.
//!
//! The generic importer contract lives in `lilly-importer-core`; this crate is
//! the only composition root that knows the concrete wiki implementations.

pub mod adapters;
mod cover_image;
mod http;

use lilly_importer_core::{AdapterError, AdapterRegistry};

/// Construct the production registry with every built-in source adapter.
pub fn builtin_registry() -> Result<AdapterRegistry, AdapterError> {
    let mut registry = AdapterRegistry::new();
    registry.register(Box::new(adapters::maddrax::MaddraxAdapter::new()?))?;
    registry.register(Box::new(
        adapters::john_sinclair::JohnSinclairAdapter::new()?
    ))?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lilly_importer_core::{
        AdapterError, WikiAdapter, normalize_and_validate_issue, verify_adapter_contract,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    use super::adapters::john_sinclair::JohnSinclairAdapter;
    use super::adapters::maddrax::MaddraxAdapter;

    #[derive(Clone, Copy)]
    enum FixtureSource {
        Maddrax,
        JohnSinclair,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum FixtureMode {
        Valid,
        InvalidMandatoryFields,
        InvalidJson,
        TruncatedBody,
        ServerError,
        Slow,
    }

    async fn spawn_fixture_server(
        source: FixtureSource,
        mode: FixtureMode,
    ) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut request = vec![0_u8; 16 * 1024];
                let Ok(length) = stream.read(&mut request).await else {
                    continue;
                };
                let request = String::from_utf8_lossy(&request[..length]);
                let request_line = request.lines().next().unwrap_or_default();
                if mode == FixtureMode::Slow {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                let (status, body) = if mode == FixtureMode::ServerError {
                    ("503 Service Unavailable", "service unavailable".to_string())
                } else if mode == FixtureMode::InvalidJson {
                    ("200 OK", "this is not JSON".to_string())
                } else {
                    let body = match source {
                        FixtureSource::Maddrax => maddrax_response(request_line, mode),
                        FixtureSource::JohnSinclair => john_sinclair_response(request_line, mode),
                    };
                    ("200 OK", body)
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    if mode == FixtureMode::TruncatedBody {
                        body.len() + 64
                    } else {
                        body.len()
                    }
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{address}"), server)
    }

    fn maddrax_response(request: &str, mode: FixtureMode) -> String {
        if request.contains("page=Zyklen") {
            return serde_json::json!({
                "parse": { "wikitext": { "*": "" } }
            })
            .to_string();
        }
        if request.contains("action=query") {
            let issue_pattern = regex::Regex::new(r"Quelle%3AMX(\d+)").unwrap();
            let requested_numbers = issue_pattern
                .captures_iter(request)
                .filter_map(|capture| capture[1].parse::<u32>().ok());
            let redirects = requested_numbers
                .filter(|number| {
                    if mode == FixtureMode::Valid {
                        *number <= 555
                    } else {
                        *number == 1
                    }
                })
                .map(|number| serde_json::json!({ "from": format!("Quelle:MX{number}") }))
                .collect::<Vec<_>>();
            return serde_json::json!({ "query": { "redirects": redirects } }).to_string();
        }

        let number = [1_u32, 409, 555]
            .into_iter()
            .find(|number| request.contains(&format!("Quelle%3AMX{number}")))
            .unwrap_or(1);
        if request.contains("prop=text") {
            return serde_json::json!({
                "parse": { "text": { "*": "<p>Kein Cover im Fixture</p>" } }
            })
            .to_string();
        }
        let (title, fixture) = match number {
            409 => (
                "Falsche Götter",
                include_str!("../tests/fixtures/maddrax/mx0409.wiki"),
            ),
            555 => (
                "Das Echo des Wandlers",
                include_str!("../tests/fixtures/maddrax/mx0555.wiki"),
            ),
            _ => (
                "Der Gott aus dem Eis",
                include_str!("../tests/fixtures/maddrax/mx0001.wiki"),
            ),
        };
        let wikitext = if mode == FixtureMode::InvalidMandatoryFields {
            format!("{{{{Roman Zyklus 01\n|Titel = {title}\n}}}}")
        } else {
            fixture.to_string()
        };
        serde_json::json!({
            "parse": { "title": title, "wikitext": { "*": wikitext } }
        })
        .to_string()
    }

    fn john_sinclair_response(request: &str, mode: FixtureMode) -> String {
        if request.contains("action=query") {
            return serde_json::json!({ "query": { "pages": {} } }).to_string();
        }
        if request.contains("page=JS_Romanhefte") {
            let overview = if mode == FixtureMode::InvalidMandatoryFields {
                "| 1 || [[JS 0001 - Im Nachtclub der Vampire|Im Nachtclub der Vampire]] || || || ||"
            } else {
                include_str!("../tests/fixtures/john_sinclair/reference-overview.wiki")
            };
            return serde_json::json!({
                "parse": { "wikitext": { "*": overview } }
            })
            .to_string();
        }
        let fixture = if mode == FixtureMode::InvalidMandatoryFields {
            "| Besonderes || Pflichtfelder fehlen"
        } else if request.contains("JS%201000") {
            include_str!("../tests/fixtures/john_sinclair/js1000.wiki")
        } else if request.contains("JS%202303") {
            include_str!("../tests/fixtures/john_sinclair/js2303.wiki")
        } else {
            include_str!("../tests/fixtures/john_sinclair/js0001.wiki")
        };
        serde_json::json!({
            "parse": { "wikitext": { "*": fixture } }
        })
        .to_string()
    }

    #[test]
    fn built_in_registry_is_complete_and_deterministic() {
        let registry = super::builtin_registry().unwrap();
        let names = registry
            .list()
            .into_iter()
            .map(|(name, _, _, _)| name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["john-sinclair", "maddrax"]);
    }

    #[tokio::test]
    async fn maddrax_passes_the_shared_fixture_contract() {
        let (base_url, server) =
            spawn_fixture_server(FixtureSource::Maddrax, FixtureMode::Valid).await;
        let adapter = MaddraxAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(base_url);

        verify_adapter_contract(&adapter).await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn john_sinclair_passes_the_shared_fixture_contract() {
        let (base_url, server) =
            spawn_fixture_server(FixtureSource::JohnSinclair, FixtureMode::Valid).await;
        let adapter = JohnSinclairAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(base_url);

        verify_adapter_contract(&adapter).await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn every_adapter_surfaces_http_failures_as_network_errors() {
        let (maddrax_url, maddrax_server) =
            spawn_fixture_server(FixtureSource::Maddrax, FixtureMode::ServerError).await;
        let maddrax = MaddraxAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(maddrax_url);
        assert!(matches!(
            maddrax.fetch_issue_list().await,
            Err(AdapterError::Network(_))
        ));
        maddrax_server.abort();

        let (sinclair_url, sinclair_server) =
            spawn_fixture_server(FixtureSource::JohnSinclair, FixtureMode::ServerError).await;
        let sinclair = JohnSinclairAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(sinclair_url);
        assert!(matches!(
            sinclair.fetch_issue_list().await,
            Err(AdapterError::Network(_))
        ));
        sinclair_server.abort();
    }

    #[tokio::test]
    async fn every_adapter_maps_timeouts_to_network_errors() {
        let (maddrax_url, maddrax_server) =
            spawn_fixture_server(FixtureSource::Maddrax, FixtureMode::Slow).await;
        let maddrax = MaddraxAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(maddrax_url)
            .with_request_timeout(Duration::from_millis(10))
            .unwrap();
        assert!(matches!(
            maddrax.fetch_issue_list().await,
            Err(AdapterError::Network(error)) if error.is_timeout()
        ));
        maddrax_server.abort();

        let (sinclair_url, sinclair_server) =
            spawn_fixture_server(FixtureSource::JohnSinclair, FixtureMode::Slow).await;
        let sinclair = JohnSinclairAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(sinclair_url)
            .with_request_timeout(Duration::from_millis(10))
            .unwrap();
        assert!(matches!(
            sinclair.fetch_issue_list().await,
            Err(AdapterError::Network(error)) if error.is_timeout()
        ));
        sinclair_server.abort();
    }

    #[tokio::test]
    async fn every_adapter_maps_invalid_json_to_parse_errors() {
        let (maddrax_url, maddrax_server) =
            spawn_fixture_server(FixtureSource::Maddrax, FixtureMode::InvalidJson).await;
        let maddrax = MaddraxAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(maddrax_url);
        assert!(matches!(
            maddrax.fetch_issue_list().await,
            Err(AdapterError::Parse(message)) if message.contains("Invalid JSON response")
        ));
        maddrax_server.abort();

        let (sinclair_url, sinclair_server) =
            spawn_fixture_server(FixtureSource::JohnSinclair, FixtureMode::InvalidJson).await;
        let sinclair = JohnSinclairAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(sinclair_url);
        assert!(matches!(
            sinclair.fetch_issue_list().await,
            Err(AdapterError::Parse(message)) if message.contains("Invalid JSON response")
        ));
        sinclair_server.abort();
    }

    #[tokio::test]
    async fn every_adapter_preserves_body_read_failures_as_network_errors() {
        let (maddrax_url, maddrax_server) =
            spawn_fixture_server(FixtureSource::Maddrax, FixtureMode::TruncatedBody).await;
        let maddrax = MaddraxAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(maddrax_url);
        let maddrax_error = maddrax.fetch_issue_list().await.unwrap_err();
        assert!(
            matches!(maddrax_error, AdapterError::Network(_)),
            "expected a retryable network error, got {maddrax_error:?}"
        );
        maddrax_server.abort();

        let (sinclair_url, sinclair_server) =
            spawn_fixture_server(FixtureSource::JohnSinclair, FixtureMode::TruncatedBody).await;
        let sinclair = JohnSinclairAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(sinclair_url);
        let sinclair_error = sinclair.fetch_issue_list().await.unwrap_err();
        assert!(
            matches!(sinclair_error, AdapterError::Network(_)),
            "expected a retryable network error, got {sinclair_error:?}"
        );
        sinclair_server.abort();
    }

    #[tokio::test]
    async fn generic_validation_rejects_missing_mandatory_fields_from_every_adapter() {
        let (maddrax_url, maddrax_server) =
            spawn_fixture_server(FixtureSource::Maddrax, FixtureMode::InvalidMandatoryFields).await;
        let maddrax = MaddraxAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(maddrax_url);
        let _ = maddrax.fetch_issue_list().await.unwrap();
        let issue = maddrax.fetch_issue_details(1).await.unwrap();
        assert!(normalize_and_validate_issue(maddrax.source_descriptor(), 1, issue).is_err());
        maddrax_server.abort();

        let (sinclair_url, sinclair_server) = spawn_fixture_server(
            FixtureSource::JohnSinclair,
            FixtureMode::InvalidMandatoryFields,
        )
        .await;
        let sinclair = JohnSinclairAdapter::new()
            .unwrap()
            .with_delay(Duration::ZERO)
            .with_request_base_url(sinclair_url);
        let _ = sinclair.fetch_issue_list().await.unwrap();
        let issue = sinclair.fetch_issue_details(1).await.unwrap();
        assert!(normalize_and_validate_issue(sinclair.source_descriptor(), 1, issue).is_err());
        sinclair_server.abort();
    }
}
