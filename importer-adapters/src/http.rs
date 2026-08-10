use lilly_importer_core::AdapterError;

pub(crate) async fn parse_json(
    response: reqwest::Response,
) -> Result<serde_json::Value, AdapterError> {
    let body = response.bytes().await?;

    serde_json::from_slice(&body)
        .map_err(|error| AdapterError::Parse(format!("Invalid JSON response: {error}")))
}
