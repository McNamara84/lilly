use lilly_importer_core::AdapterError;

pub(crate) async fn parse_json(
    response: reqwest::Response,
) -> Result<serde_json::Value, AdapterError> {
    let body = response.bytes().await?;
    let json = serde_json::from_slice(&body)
        .map_err(|error| AdapterError::Parse(format!("Invalid JSON response: {error}")))?;
    validate_mediawiki_response(&json)?;
    Ok(json)
}

pub(crate) fn validate_mediawiki_response(json: &serde_json::Value) -> Result<(), AdapterError> {
    let Some(error) = json.get("error") else {
        return Ok(());
    };
    let code = error["code"].as_str().unwrap_or("unknown");
    let info = error["info"]
        .as_str()
        .map_or_else(|| error.to_string(), ToString::to_string);
    Err(AdapterError::Parse(format!(
        "MediaWiki API error '{code}': {info}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_top_level_mediawiki_errors() {
        let error = serde_json::json!({
            "error": {
                "code": "badvalue",
                "info": "Invalid request"
            }
        });

        assert!(matches!(
            validate_mediawiki_response(&error),
            Err(AdapterError::Parse(message))
                if message == "MediaWiki API error 'badvalue': Invalid request"
        ));
    }

    #[test]
    fn accepts_responses_without_a_top_level_error() {
        assert!(
            validate_mediawiki_response(&serde_json::json!({ "query": { "pages": [] } })).is_ok()
        );
    }
}
