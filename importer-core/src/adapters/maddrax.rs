use async_trait::async_trait;
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;

use crate::adapter::{AdapterError, WikiAdapter};
use crate::types::{CoverData, IssueData, SeriesData, SeriesStatus};

const MADDRAXIKON_BASE: &str = "https://de.maddraxikon.com";
const DEFAULT_DELAY_MS: u64 = 500;

pub struct MaddraxAdapter {
    client: Client,
    delay: Duration,
}

impl MaddraxAdapter {
    /// Creates a new `MaddraxAdapter`.
    ///
    /// # Errors
    ///
    /// Returns `AdapterError::Other` if the HTTP client cannot be built.
    pub fn new() -> Result<Self, AdapterError> {
        Ok(Self {
            client: Client::builder()
                .user_agent("LILLY-Importer/0.9 (Heftroman-Collection-Manager)")
                .timeout(Duration::from_secs(30))
                .build()?,
            delay: Duration::from_millis(DEFAULT_DELAY_MS),
        })
    }

    #[must_use]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    async fn rate_limit(&self) {
        tokio::time::sleep(self.delay).await;
    }

    fn parse_issue_list_from_html(html: &str) -> Result<Vec<u32>, AdapterError> {
        let document = Html::parse_document(html);
        let row_selector = Selector::parse("table.wikitable tr")
            .map_err(|e| AdapterError::Parse(format!("Failed to parse row selector: {e}")))?;
        let cell_selector = Selector::parse("td")
            .map_err(|e| AdapterError::Parse(format!("Failed to parse cell selector: {e}")))?;

        let mut issue_numbers = Vec::new();

        for row in document.select(&row_selector) {
            let cells: Vec<_> = row.select(&cell_selector).collect();
            if let Some(first_cell) = cells.first() {
                let text = first_cell.text().collect::<String>().trim().to_string();
                if let Ok(num) = text.parse::<u32>() {
                    issue_numbers.push(num);
                }
            }
        }

        if issue_numbers.is_empty() {
            return Err(AdapterError::Parse(
                "No issue numbers found in wiki table".to_string(),
            ));
        }

        Ok(issue_numbers)
    }
}

impl Default for MaddraxAdapter {
    fn default() -> Self {
        Self::new().expect("Failed to build HTTP client")
    }
}

#[async_trait]
impl WikiAdapter for MaddraxAdapter {
    fn name(&self) -> &'static str {
        "maddrax"
    }

    fn display_name(&self) -> &'static str {
        "Maddrax \u{2013} Die dunkle Zukunft der Erde"
    }

    fn version(&self) -> &'static str {
        "0.9"
    }

    async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
        Ok(SeriesData {
            name: "Maddrax \u{2013} Die dunkle Zukunft der Erde".to_string(),
            slug: "maddrax".to_string(),
            publisher: Some("Bastei L\u{00fc}bbe".to_string()),
            genre: Some("Science-Fiction".to_string()),
            frequency: Some("14-t\u{00e4}gig".to_string()),
            total_issues: None,
            status: SeriesStatus::Running,
            source_url: Some(format!("{MADDRAXIKON_BASE}/wiki/Hauptseite")),
        })
    }

    async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
        self.rate_limit().await;

        let url = format!(
            "{MADDRAXIKON_BASE}/w/index.php?action=parse&page=Romane&prop=text&formatversion=2&format=json"
        );

        let response = self.client.get(&url).send().await?;
        let json: serde_json::Value = response.json().await?;

        let html = json["parse"]["text"]
            .as_str()
            .ok_or_else(|| AdapterError::Parse("Missing parse.text in API response".to_string()))?;

        Self::parse_issue_list_from_html(html)
    }

    async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
        self.rate_limit().await;

        let page_title = format!("Maddrax {issue_number}");
        let url = format!(
            "{MADDRAXIKON_BASE}/w/index.php?action=parse&page={}&prop=text&formatversion=2&format=json",
            urlencoding::encode(&page_title)
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AdapterError::NotFound(format!(
                "Issue {issue_number} not found"
            )));
        }

        let json: serde_json::Value = response.json().await?;
        let html = json["parse"]["text"]
            .as_str()
            .ok_or_else(|| AdapterError::Parse("Missing parse.text in API response".to_string()))?;

        let document = Html::parse_document(html);

        // Extract title from infobox or first heading
        let title = extract_infobox_field(&document, "Titel")
            .unwrap_or_else(|| format!("Maddrax {issue_number}"));

        let author = extract_infobox_field(&document, "Autor");
        let cycle = extract_infobox_field(&document, "Zyklus");

        let published_at =
            extract_infobox_field(&document, "Ersterscheinung").and_then(|s| parse_german_date(&s));

        let source_wiki_url = format!(
            "{MADDRAXIKON_BASE}/wiki/{}",
            urlencoding::encode(&page_title)
        );

        Ok(IssueData {
            issue_number,
            title,
            author,
            published_at,
            cycle,
            source_wiki_url: Some(source_wiki_url),
        })
    }

    async fn fetch_cover(&self, issue_number: u32) -> Result<Option<CoverData>, AdapterError> {
        self.rate_limit().await;

        let page_title = format!("Maddrax {issue_number}");
        let url = format!(
            "{MADDRAXIKON_BASE}/w/index.php?action=parse&page={}&prop=text&formatversion=2&format=json",
            urlencoding::encode(&page_title)
        );

        let response = self.client.get(&url).send().await?;
        let json: serde_json::Value = response.json().await?;
        let html = json["parse"]["text"]
            .as_str()
            .ok_or_else(|| AdapterError::Parse("Missing parse.text".to_string()))?;

        // Extract image URL synchronously to avoid holding Html across await
        let img_url = extract_cover_url(html);

        let Some(img_url) = img_url else {
            return Ok(None);
        };

        let img_response = self.client.get(&img_url).send().await?;
        let content_type = img_response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();

        let bytes = img_response.bytes().await?.to_vec();

        Ok(Some(CoverData {
            bytes,
            content_type,
        }))
    }
}

fn extract_infobox_field(document: &Html, field_name: &str) -> Option<String> {
    let row_sel = Selector::parse("tr").ok()?;
    let header_sel = Selector::parse("th").ok()?;
    let data_sel = Selector::parse("td").ok()?;

    for row in document.select(&row_sel) {
        let header = row.select(&header_sel).next();
        if let Some(header) = header {
            let text = header.text().collect::<String>();
            if text.trim().eq_ignore_ascii_case(field_name) {
                if let Some(data_cell) = row.select(&data_sel).next() {
                    let value = data_cell.text().collect::<String>().trim().to_string();
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
            }
        }
    }
    None
}

fn extract_cover_url(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selectors = ["img.thumbimage", ".infobox img", "table.wikitable img"];

    for sel_str in &selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(img) = document.select(&sel).next() {
                if let Some(src) = img.value().attr("src") {
                    let url = if src.starts_with("//") {
                        format!("https:{src}")
                    } else if src.starts_with('/') {
                        format!("{MADDRAXIKON_BASE}{src}")
                    } else {
                        src.to_string()
                    };
                    return Some(url);
                }
            }
        }
    }
    None
}

fn parse_german_date(s: &str) -> Option<chrono::NaiveDate> {
    let trimmed = s.trim();

    // Try numeric format first: "15.03.2026"
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%d.%m.%Y") {
        return Some(date);
    }

    // Handle German month names: "15. März 2026"
    let german_months = [
        ("Januar", "01"),
        ("Februar", "02"),
        ("März", "03"),
        ("April", "04"),
        ("Mai", "05"),
        ("Juni", "06"),
        ("Juli", "07"),
        ("August", "08"),
        ("September", "09"),
        ("Oktober", "10"),
        ("November", "11"),
        ("Dezember", "12"),
    ];

    for (name, num) in &german_months {
        if trimmed.contains(name) {
            let normalized = trimmed.replace(name, num);
            // "15. 03 2026" → parse with spaces
            if let Ok(date) = chrono::NaiveDate::parse_from_str(&normalized, "%d. %m %Y") {
                return Some(date);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_name() {
        let adapter = MaddraxAdapter::new().unwrap();
        assert_eq!(adapter.name(), "maddrax");
    }

    #[test]
    fn test_adapter_display_name() {
        let adapter = MaddraxAdapter::new().unwrap();
        assert!(adapter.display_name().contains("Maddrax"));
    }

    #[test]
    fn test_adapter_version() {
        let adapter = MaddraxAdapter::new().unwrap();
        assert_eq!(adapter.version(), "0.9");
    }

    #[tokio::test]
    async fn test_fetch_series_metadata() {
        let adapter = MaddraxAdapter::new().unwrap();
        let metadata = adapter.fetch_series_metadata().await.unwrap();
        assert_eq!(metadata.slug, "maddrax");
        assert_eq!(metadata.status, SeriesStatus::Running);
        assert!(metadata.publisher.is_some());
    }

    #[test]
    fn test_parse_issue_list_from_html() {
        let html = r#"
            <table class="wikitable">
                <tr><th>Nr.</th><th>Titel</th></tr>
                <tr><td>1</td><td>Dunkle Zukunft</td></tr>
                <tr><td>2</td><td>Die Flucht</td></tr>
                <tr><td>3</td><td>Apocalypse</td></tr>
            </table>
        "#;
        let issues = MaddraxAdapter::parse_issue_list_from_html(html).unwrap();
        assert_eq!(issues, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_issue_list_empty_table() {
        let html = r#"<table class="wikitable"><tr><th>Nr.</th></tr></table>"#;
        let result = MaddraxAdapter::parse_issue_list_from_html(html);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_german_date_dot_format() {
        let date = parse_german_date("15.03.2026");
        assert_eq!(
            date,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 3, 15).unwrap())
        );
    }

    #[test]
    fn test_parse_german_date_invalid() {
        assert!(parse_german_date("invalid").is_none());
    }

    #[test]
    fn test_parse_german_date_german_month_name() {
        let date = parse_german_date("15. März 2026");
        assert_eq!(
            date,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 3, 15).unwrap())
        );
    }

    #[test]
    fn test_parse_german_date_dezember() {
        let date = parse_german_date("24. Dezember 2025");
        assert_eq!(
            date,
            Some(chrono::NaiveDate::from_ymd_opt(2025, 12, 24).unwrap())
        );
    }

    #[test]
    fn test_default_adapter() {
        let adapter = MaddraxAdapter::default();
        assert_eq!(adapter.name(), "maddrax");
    }

    #[test]
    fn test_with_delay() {
        let adapter = MaddraxAdapter::new()
            .unwrap()
            .with_delay(Duration::from_millis(100));
        assert_eq!(adapter.delay, Duration::from_millis(100));
    }

    #[test]
    fn test_extract_infobox_field_from_html() {
        let html = r"<table><tr><th>Titel</th><td>Dunkle Zukunft</td></tr></table>";
        let document = Html::parse_document(html);
        let result = extract_infobox_field(&document, "Titel");
        assert_eq!(result, Some("Dunkle Zukunft".to_string()));
    }

    #[test]
    fn test_extract_infobox_field_missing() {
        let html = r"<table><tr><th>Autor</th><td>Test</td></tr></table>";
        let document = Html::parse_document(html);
        let result = extract_infobox_field(&document, "Titel");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_cover_url_thumbimage() {
        let html = r#"<div><img class="thumbimage" src="//example.com/cover.jpg" /></div>"#;
        let result = extract_cover_url(html);
        assert_eq!(result, Some("https://example.com/cover.jpg".to_string()));
    }

    #[test]
    fn test_extract_cover_url_relative() {
        let html = r#"<div class="infobox"><img src="/images/cover.jpg" /></div>"#;
        let result = extract_cover_url(html);
        assert_eq!(result, Some(format!("{MADDRAXIKON_BASE}/images/cover.jpg")));
    }

    #[test]
    fn test_extract_cover_url_none() {
        let html = r"<div><p>No images here</p></div>";
        let result = extract_cover_url(html);
        assert!(result.is_none());
    }
}
