use async_trait::async_trait;
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;

use crate::adapter::{AdapterError, WikiAdapter};
use crate::types::{CoverData, IssueData, SeriesData, SeriesStatus};

const MADDRAXIKON_BASE: &str = "https://de.maddraxikon.com";
const DEFAULT_DELAY_MS: u64 = 500;
/// `MediaWiki` API allows up to 50 titles per query request
const BATCH_SIZE: u32 = 50;
/// Stop scanning after this many consecutive missing issue numbers
const MAX_CONSECUTIVE_MISSING: u32 = 10;

pub struct MaddraxAdapter {
    client: Client,
    pub(crate) delay: Duration,
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

    /// Probe `Quelle:MX{start}..Quelle:MX{start+BATCH_SIZE-1}` via `MediaWiki` Query API.
    /// Returns the set of issue numbers that have valid redirects.
    async fn probe_issue_batch(&self, start: u32, end: u32) -> Result<Vec<u32>, AdapterError> {
        let titles: Vec<String> = (start..=end).map(|n| format!("Quelle:MX{n}")).collect();
        let titles_param = titles.join("|");

        let url = format!(
            "{MADDRAXIKON_BASE}/api.php?action=query&titles={}&redirects=1&format=json",
            urlencoding::encode(&titles_param)
        );

        let response = self.client.get(&url).send().await?;
        let json: serde_json::Value = response.json().await?;

        let mut found = Vec::new();

        // Each redirect entry means the Quelle:MX{n} page exists
        if let Some(redirects) = json["query"]["redirects"].as_array() {
            for redirect in redirects {
                if let Some(from) = redirect["from"].as_str() {
                    // Extract number from "Quelle:MX123"
                    if let Some(num_str) = from.strip_prefix("Quelle:MX") {
                        if let Ok(num) = num_str.parse::<u32>() {
                            found.push(num);
                        }
                    }
                }
            }
        }

        found.sort_unstable();
        Ok(found)
    }

    /// Parse wikitext template parameters from the `{{Roman Zyklus ...}}` infobox.
    /// Returns a map of `field_name` → value.
    fn parse_wikitext_infobox(wikitext: &str) -> std::collections::HashMap<String, String> {
        let mut fields = std::collections::HashMap::new();

        for line in wikitext.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix('|') {
                if let Some((key, value)) = rest.split_once('=') {
                    let key = key.trim().to_string();
                    let value = value.trim().to_string();
                    if !key.is_empty() && !value.is_empty() {
                        fields.insert(key, value);
                    }
                }
            }
        }

        fields
    }

    /// Strip `MediaWiki` markup (links, bold, etc.) from a value string.
    fn strip_wiki_markup(s: &str) -> String {
        let mut result = s.to_string();
        // Remove [[Target|Display]] → Display, [[Target]] → Target
        while let Some(start) = result.find("[[") {
            if let Some(end) = result[start..].find("]]") {
                let inner = &result[start + 2..start + end];
                let display = inner.split('|').next_back().unwrap_or(inner);
                let display = display.to_string();
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    display,
                    &result[start + end + 2..]
                );
            } else {
                break;
            }
        }
        result.replace("'''", "").replace("''", "")
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
        let mut all_issues = Vec::new();
        let mut consecutive_missing = 0u32;
        let mut current = 1u32;

        loop {
            self.rate_limit().await;

            let batch_end = current + BATCH_SIZE - 1;
            let found = self.probe_issue_batch(current, batch_end).await?;

            if found.is_empty() {
                consecutive_missing += BATCH_SIZE;
            } else {
                // Count consecutive missing from the end of this batch
                let max_found = found.iter().copied().max().unwrap_or(current);
                consecutive_missing = batch_end - max_found;
                all_issues.extend(found);
            }

            if consecutive_missing >= MAX_CONSECUTIVE_MISSING {
                break;
            }

            current = batch_end + 1;
        }

        if all_issues.is_empty() {
            return Err(AdapterError::Parse(
                "No issue numbers found via Quelle:MX redirects".to_string(),
            ));
        }

        all_issues.sort_unstable();
        all_issues.dedup();
        Ok(all_issues)
    }

    async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
        self.rate_limit().await;

        let url = format!(
            "{MADDRAXIKON_BASE}/api.php?action=parse&page={}&prop=wikitext&redirects=1&format=json",
            urlencoding::encode(&format!("Quelle:MX{issue_number}"))
        );

        let response = self.client.get(&url).send().await?;
        let json: serde_json::Value = response.json().await?;

        if json.get("error").is_some() {
            return Err(AdapterError::NotFound(format!(
                "Issue {issue_number} not found"
            )));
        }

        let wiki_title = json["parse"]["title"].as_str().unwrap_or("").to_string();

        let wikitext = json["parse"]["wikitext"]["*"]
            .as_str()
            .ok_or_else(|| AdapterError::Parse("Missing wikitext in API response".to_string()))?;

        let fields = Self::parse_wikitext_infobox(wikitext);

        let title = fields.get("Titel").map_or_else(
            || {
                if wiki_title.is_empty() {
                    format!("Maddrax {issue_number}")
                } else {
                    wiki_title.clone()
                }
            },
            |s| Self::strip_wiki_markup(s),
        );

        let author = fields.get("Autor").map(|s| Self::strip_wiki_markup(s));

        let cycle = fields.get("Zyklus").map(|s| Self::strip_wiki_markup(s));

        let published_at = fields
            .get("Erscheinungsdatum")
            .and_then(|s| parse_german_date(s));

        let source_wiki_url = format!(
            "{MADDRAXIKON_BASE}/wiki/{}",
            urlencoding::encode(&wiki_title)
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

        let url = format!(
            "{MADDRAXIKON_BASE}/api.php?action=parse&page={}&prop=text&redirects=1&format=json",
            urlencoding::encode(&format!("Quelle:MX{issue_number}"))
        );

        let response = self.client.get(&url).send().await?;
        let json: serde_json::Value = response.json().await?;

        let html = json["parse"]["text"]["*"]
            .as_str()
            .ok_or_else(|| AdapterError::Parse("Missing parse.text in API response".to_string()))?;

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

fn extract_cover_url(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selectors = [
        "img.mw-file-element",
        "img.thumbimage",
        ".infobox img",
        "table.wikitable img",
    ];

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
    fn test_parse_wikitext_infobox() {
        let wikitext = r"{{Roman Zyklus 01
|NummerVor = &nbsp;
|Nummer = 1
|NummerNach = 2
|Titel = Der Gott aus dem Eis
|Autor = Jo Zybell
|Erscheinungsdatum = 08.02.2000
|Titelbildzeichner = Koveck
}}Some text after";
        let fields = MaddraxAdapter::parse_wikitext_infobox(wikitext);
        assert_eq!(fields.get("Titel").unwrap(), "Der Gott aus dem Eis");
        assert_eq!(fields.get("Autor").unwrap(), "Jo Zybell");
        assert_eq!(fields.get("Nummer").unwrap(), "1");
        assert_eq!(fields.get("Erscheinungsdatum").unwrap(), "08.02.2000");
    }

    #[test]
    fn test_parse_wikitext_infobox_empty() {
        let fields = MaddraxAdapter::parse_wikitext_infobox("No template here");
        assert!(fields.is_empty());
    }

    #[test]
    fn test_strip_wiki_markup_link_with_display() {
        let result = MaddraxAdapter::strip_wiki_markup("[[Maddrax-Taschenbücher|Taschenbuch]]");
        assert_eq!(result, "Taschenbuch");
    }

    #[test]
    fn test_strip_wiki_markup_plain_link() {
        let result = MaddraxAdapter::strip_wiki_markup("[[Jo Zybell]]");
        assert_eq!(result, "Jo Zybell");
    }

    #[test]
    fn test_strip_wiki_markup_bold() {
        let result = MaddraxAdapter::strip_wiki_markup("'''bold text'''");
        assert_eq!(result, "bold text");
    }

    #[test]
    fn test_strip_wiki_markup_no_markup() {
        let result = MaddraxAdapter::strip_wiki_markup("plain text");
        assert_eq!(result, "plain text");
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
    fn test_extract_cover_url_mw_file_element() {
        let html = r#"<td><a href="/index.php?title=Datei:001tibi.jpg" class="mw-file-description"><img src="/images/thumb/1/10/001tibi.jpg/200px-001tibi.jpg" class="mw-file-element" /></a></td>"#;
        let result = extract_cover_url(html);
        assert_eq!(
            result,
            Some(format!(
                "{MADDRAXIKON_BASE}/images/thumb/1/10/001tibi.jpg/200px-001tibi.jpg"
            ))
        );
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
