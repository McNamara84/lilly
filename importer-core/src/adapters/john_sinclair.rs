use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::NaiveDate;
#[cfg(test)]
use chrono::Utc;
use reqwest::Client;

use crate::adapter::{AdapterError, SourceDescriptor, WikiAdapter};
use crate::cover_image::download_cover_image;
use crate::types::{CoverData, IssueData, SeriesData, SeriesStatus, SourceReference};

const BASE_URL: &str = "https://www.gruselroman-wiki.de";
const OVERVIEW_PAGE: &str = "JS_Romanhefte";
const SOURCE_DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    source_key: "gruselroman-wiki",
    display_name: "Gruselroman-Wiki",
    allowed_host: "www.gruselroman-wiki.de",
    series_name: "Geisterjäger John Sinclair",
    series_slug: "john-sinclair",
    series_record_id: OVERVIEW_PAGE,
    series_url: "https://www.gruselroman-wiki.de/index.php?title=JS_Romanhefte",
};
const DEFAULT_DELAY_MS: u64 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
struct IssueSummary {
    issue_number: u32,
    page_title: String,
    title: String,
    authors: Vec<String>,
    published_at: Option<NaiveDate>,
    part_number: Option<u32>,
    part_total: Option<u32>,
    cover_artists: Vec<String>,
}

pub struct JohnSinclairAdapter {
    client: Client,
    pub(crate) delay: Duration,
    #[cfg(test)]
    today_override: Option<NaiveDate>,
    issue_index: tokio::sync::RwLock<HashMap<u32, IssueSummary>>,
}

impl JohnSinclairAdapter {
    /// Creates a new adapter for the first edition of the regular John Sinclair series.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn new() -> Result<Self, AdapterError> {
        Ok(Self {
            client: Client::builder()
                .user_agent("LILLY-Importer/0.1 (Heftroman-Collection-Manager)")
                .timeout(Duration::from_secs(30))
                .build()?,
            delay: Duration::from_millis(DEFAULT_DELAY_MS),
            #[cfg(test)]
            today_override: None,
            issue_index: tokio::sync::RwLock::new(HashMap::new()),
        })
    }

    #[must_use]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    #[cfg(test)]
    #[must_use]
    fn with_today(mut self, today: NaiveDate) -> Self {
        self.today_override = Some(today);
        self
    }

    async fn rate_limit(&self) {
        tokio::time::sleep(self.delay).await;
    }

    #[cfg(test)]
    fn today(&self) -> NaiveDate {
        self.today_override.unwrap_or_else(|| {
            Utc::now()
                .with_timezone(&chrono_tz::Europe::Berlin)
                .date_naive()
        })
    }

    async fn fetch_page_wikitext(&self, page: &str) -> Result<String, AdapterError> {
        self.rate_limit().await;
        let url = format!(
            "{BASE_URL}/api.php?action=parse&page={}&prop=wikitext&format=json",
            urlencoding::encode(page)
        );
        let response = self.client.get(url).send().await?.error_for_status()?;
        let json: serde_json::Value = response.json().await?;
        if let Some(error) = json.get("error") {
            return Err(AdapterError::NotFound(error.to_string()));
        }
        json["parse"]["wikitext"]["*"]
            .as_str()
            .map(ToString::to_string)
            .ok_or_else(|| AdapterError::Parse(format!("Missing wikitext for page '{page}'")))
    }

    async fn ensure_index(&self) -> Result<(), AdapterError> {
        if self.issue_index.read().await.is_empty() {
            self.fetch_issue_list().await?;
        }
        Ok(())
    }

    fn parse_overview(wikitext: &str) -> Result<Vec<IssueSummary>, AdapterError> {
        let title_link = regex::Regex::new(r"\[\[(JS\s+(\d+)\s*-\s*[^|\]]+)(?:\|([^\]]+))?\]\]")
            .map_err(|error| AdapterError::Parse(format!("Invalid title regex: {error}")))?;
        let mut summaries = HashMap::new();

        for raw_line in wikitext.lines() {
            let line = raw_line.trim();
            if !line.starts_with('|') || !line.contains("||") {
                continue;
            }
            let columns: Vec<&str> = line
                .trim_start_matches('|')
                .split("||")
                .map(str::trim)
                .collect();
            if columns.len() < 6 {
                continue;
            }
            let Ok(displayed_number) = columns[0].parse::<u32>() else {
                continue;
            };
            let Some(captures) = title_link.captures(columns[1]) else {
                return Err(AdapterError::Parse(format!(
                    "Issue {displayed_number} has no canonical JS page link"
                )));
            };
            let page_title = captures
                .get(1)
                .map(|value| value.as_str().trim().to_string())
                .ok_or_else(|| AdapterError::Parse("Missing page title capture".to_string()))?;
            let canonical_number = captures
                .get(2)
                .and_then(|value| value.as_str().parse::<u32>().ok())
                .ok_or_else(|| {
                    AdapterError::Parse(format!(
                        "Invalid canonical issue number for row {displayed_number}"
                    ))
                })?;
            if canonical_number != displayed_number {
                tracing::warn!(
                    displayed_number,
                    canonical_number,
                    "John Sinclair overview number differs from canonical page title"
                );
            }

            let fallback_title = page_title
                .split_once('-')
                .map_or(page_title.as_str(), |(_, title)| title)
                .trim();
            let title = captures
                .get(3)
                .map_or(fallback_title, |value| value.as_str())
                .trim()
                .to_string();
            let (part_number, part_total) = parse_part_position(columns[4]);
            let summary = IssueSummary {
                issue_number: canonical_number,
                page_title,
                title,
                authors: parse_people(columns[2]),
                cover_artists: parse_people(columns[3]),
                part_number,
                part_total,
                published_at: parse_german_date(columns[5]),
            };

            if summaries.insert(canonical_number, summary).is_some() {
                return Err(AdapterError::Parse(format!(
                    "Duplicate canonical issue number {canonical_number}"
                )));
            }
        }

        if summaries.is_empty() {
            return Err(AdapterError::Parse(
                "No John Sinclair issue rows found on overview page".to_string(),
            ));
        }

        let mut result: Vec<IssueSummary> = summaries.into_values().collect();
        result.sort_by_key(|summary| summary.issue_number);
        Ok(result)
    }

    #[cfg(test)]
    fn filter_published(summaries: Vec<IssueSummary>, today: NaiveDate) -> Vec<IssueSummary> {
        summaries
            .into_iter()
            .filter(|summary| summary.published_at.is_none_or(|date| date <= today))
            .collect()
    }
}

impl Default for JohnSinclairAdapter {
    fn default() -> Self {
        Self::new().expect("Failed to build HTTP client")
    }
}

#[async_trait]
impl WikiAdapter for JohnSinclairAdapter {
    fn name(&self) -> &'static str {
        "john-sinclair"
    }

    fn display_name(&self) -> &'static str {
        "Geisterjäger John Sinclair"
    }

    fn version(&self) -> &'static str {
        "0.1"
    }

    fn source_descriptor(&self) -> SourceDescriptor {
        SOURCE_DESCRIPTOR
    }

    async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
        Ok(SeriesData {
            name: "Geisterjäger John Sinclair".to_string(),
            slug: "john-sinclair".to_string(),
            publisher: Some("Bastei Verlag".to_string()),
            genre: Some("Horror / Grusel".to_string()),
            frequency: Some("wöchentlich".to_string()),
            total_issues: None,
            status: SeriesStatus::Running,
            source: SourceReference {
                source_key: SOURCE_DESCRIPTOR.source_key.to_string(),
                source_record_id: SOURCE_DESCRIPTOR.series_record_id.to_string(),
                source_url: SOURCE_DESCRIPTOR.series_url.to_string(),
            },
        })
    }

    async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
        let wikitext = self.fetch_page_wikitext(OVERVIEW_PAGE).await?;
        let summaries = Self::parse_overview(&wikitext)?;
        let numbers = summaries
            .iter()
            .map(|summary| summary.issue_number)
            .collect();
        *self.issue_index.write().await = summaries
            .into_iter()
            .map(|summary| (summary.issue_number, summary))
            .collect();
        Ok(numbers)
    }

    async fn fetch_issue_details(&self, issue_number: u32) -> Result<IssueData, AdapterError> {
        self.ensure_index().await?;
        let summary = self
            .issue_index
            .read()
            .await
            .get(&issue_number)
            .cloned()
            .ok_or_else(|| AdapterError::NotFound(format!("Issue {issue_number} not found")))?;
        let wikitext = self.fetch_page_wikitext(&summary.page_title).await?;
        Ok(map_issue_details(summary, &wikitext))
    }

    async fn fetch_cover(&self, issue_number: u32) -> Result<Option<CoverData>, AdapterError> {
        self.ensure_index().await?;
        let page_title = self
            .issue_index
            .read()
            .await
            .get(&issue_number)
            .map(|summary| summary.page_title.clone())
            .ok_or_else(|| AdapterError::NotFound(format!("Issue {issue_number} not found")))?;

        self.rate_limit().await;
        let url = format!(
            "{BASE_URL}/api.php?action=query&generator=images&titles={}&gimlimit=max&prop=imageinfo&iiprop=url&format=json",
            urlencoding::encode(&page_title)
        );
        let response = self.client.get(url).send().await?.error_for_status()?;
        let json: serde_json::Value = response.json().await?;
        let Some(image_url) = extract_cover_url(&json, issue_number) else {
            return Ok(None);
        };

        download_cover_image(&self.client, &image_url)
            .await
            .map(Some)
    }
}

fn map_issue_details(summary: IssueSummary, wikitext: &str) -> IssueData {
    let fields = parse_infobox_fields(wikitext);
    let authors = fields
        .get("Autoren")
        .map_or_else(|| summary.authors.clone(), |value| parse_people(value));
    let cover_artists = fields
        .get("Cover")
        .or_else(|| fields.get("Coverzeichner"))
        .map_or_else(
            || summary.cover_artists.clone(),
            |value| parse_people(value),
        );
    let published_at = fields
        .get("Erscheinungsdatum")
        .and_then(|value| parse_german_date(value))
        .or(summary.published_at);
    let (detail_part_number, detail_part_total) = fields
        .get("Teil")
        .map_or((None, None), |value| parse_part_position(value));
    let (part_number, part_total) = if detail_part_number.is_some() {
        (detail_part_number, detail_part_total)
    } else {
        (summary.part_number, summary.part_total)
    };
    let notes = fields
        .get("Besonderes")
        .map(|value| {
            strip_wiki_markup(value)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let source_record_id = summary.page_title;

    IssueData {
        issue_number: summary.issue_number,
        title: summary.title,
        authors,
        published_at,
        part_number,
        part_total,
        cycle: None,
        cover_artists,
        keywords: Vec::new(),
        notes,
        source: SourceReference {
            source_key: SOURCE_DESCRIPTOR.source_key.to_string(),
            source_url: format!(
                "{BASE_URL}/index.php?title={}",
                urlencoding::encode(&source_record_id.replace(' ', "_"))
            ),
            source_record_id,
        },
    }
}

fn parse_infobox_fields(wikitext: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let lines: Vec<&str> = wikitext.lines().collect();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.starts_with('|')
            && !line.starts_with("|-")
            && !line.starts_with("|colspan")
            && let Some((key, first_value)) = line.trim_start_matches('|').split_once("||")
        {
            let mut value_parts = vec![first_value.trim()];
            let mut next = index + 1;
            while next < lines.len() {
                let candidate = lines[next].trim();
                if candidate == "|-" || candidate == "|}" {
                    break;
                }
                if !candidate.is_empty() {
                    value_parts.push(candidate);
                }
                next += 1;
            }
            fields.insert(key.trim().to_string(), value_parts.join("\n"));
            index = next;
            continue;
        }
        index += 1;
    }
    fields
}

fn parse_people(value: &str) -> Vec<String> {
    let plain = strip_wiki_markup(value).replace("\n*", " & ");
    plain
        .split(['&', ','])
        .map(str::trim)
        .filter(|person| !person.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn strip_wiki_markup(value: &str) -> String {
    let mut result = value.to_string();
    while let Some(start) = result.find("[[") {
        let Some(relative_end) = result[start..].find("]]") else {
            break;
        };
        let end = start + relative_end;
        let inner = &result[start + 2..end];
        let display = inner.split('|').next_back().unwrap_or(inner).to_string();
        result.replace_range(start..end + 2, &display);
    }
    result
        .replace("'''", "")
        .replace("''", "")
        .replace("<br>", "\n")
        .replace("</br>", "\n")
}

fn parse_part_position(value: &str) -> (Option<u32>, Option<u32>) {
    let Some((number, total)) = value.trim().split_once('/') else {
        return (None, None);
    };
    let Ok(number) = number.trim().parse::<u32>() else {
        return (None, None);
    };
    let Ok(total) = total.trim().parse::<u32>() else {
        return (None, None);
    };
    if number > 0 && number <= total {
        (Some(number), Some(total))
    } else {
        (None, None)
    }
}

fn parse_german_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%d.%m.%Y").ok()
}

fn extract_cover_url(json: &serde_json::Value, issue_number: u32) -> Option<String> {
    json["query"]["pages"]
        .as_object()?
        .values()
        .find_map(|page| {
            let title = page["title"].as_str()?;
            if !is_issue_cover_title(title, issue_number) {
                return None;
            }
            page["imageinfo"]
                .as_array()?
                .iter()
                .find_map(|image_info| image_info["url"].as_str().map(ToString::to_string))
        })
}

fn is_issue_cover_title(title: &str, issue_number: u32) -> bool {
    let file_name = title.rsplit(':').next().unwrap_or(title);
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _extension)| stem);
    let normalized = stem
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>();
    normalized.eq_ignore_ascii_case(&format!("js{issue_number:04}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OVERVIEW_FIXTURE: &str = include_str!("../../tests/fixtures/john_sinclair/overview.wiki");

    #[test]
    fn parse_overview_uses_canonical_number_and_extracts_metadata() {
        let summaries = JohnSinclairAdapter::parse_overview(OVERVIEW_FIXTURE).unwrap();
        assert_eq!(summaries.len(), 6);
        let first = &summaries[0];
        assert_eq!(first.issue_number, 1);
        assert_eq!(first.title, "Im Nachtclub der Vampire");
        assert_eq!(first.authors, vec!["Jason Dark"]);
        assert_eq!(first.cover_artists, vec!["Vicente Ballestar"]);
        assert_eq!(
            first.published_at,
            Some(NaiveDate::from_ymd_opt(1978, 1, 17).unwrap())
        );
        assert_eq!(summaries[3].issue_number, 2391);
    }

    #[test]
    fn parse_overview_extracts_optional_part_position() {
        let summaries = JohnSinclairAdapter::parse_overview(OVERVIEW_FIXTURE).unwrap();
        let issue = summaries
            .iter()
            .find(|summary| summary.issue_number == 11)
            .unwrap();
        assert_eq!((issue.part_number, issue.part_total), (Some(1), Some(2)));
        assert_eq!(
            (summaries[0].part_number, summaries[0].part_total),
            (None, None)
        );
    }

    #[test]
    fn parse_overview_rejects_duplicate_canonical_numbers() {
        let duplicated = format!(
            "{OVERVIEW_FIXTURE}\n{}",
            OVERVIEW_FIXTURE.lines().next().unwrap_or("")
        );
        // The fixture starts with a blank line, so append an explicit duplicate instead.
        let duplicated = format!(
            "{duplicated}\n| 1 || [[JS 0001 - Im Nachtclub der Vampire|Duplicate]] || [[Jason Dark]] || || ||"
        );
        assert!(matches!(
            JohnSinclairAdapter::parse_overview(&duplicated),
            Err(AdapterError::Parse(message)) if message.contains("Duplicate canonical")
        ));
    }

    #[test]
    fn parse_overview_rejects_rows_without_canonical_link() {
        let invalid = "| 1 || Im Nachtclub der Vampire || Jason Dark || || || 17.01.1978";
        assert!(matches!(
            JohnSinclairAdapter::parse_overview(invalid),
            Err(AdapterError::Parse(message)) if message.contains("no canonical")
        ));
    }

    #[test]
    fn parse_overview_rejects_empty_input() {
        assert!(matches!(
            JohnSinclairAdapter::parse_overview("no table"),
            Err(AdapterError::Parse(message)) if message.contains("No John Sinclair")
        ));
    }

    #[test]
    fn parse_part_position_is_optional_and_validated() {
        assert_eq!(parse_part_position("1/3"), (Some(1), Some(3)));
        assert_eq!(parse_part_position(" 2 / 3 "), (Some(2), Some(3)));
        for invalid in ["", "1", "0/2", "3/2", "x/y"] {
            assert_eq!(parse_part_position(invalid), (None, None));
        }
    }

    #[test]
    fn parse_people_handles_links_multiple_authors_and_plain_credits() {
        assert_eq!(
            parse_people("[[Michaela Froelian]] & [[Logan Dee]]"),
            vec!["Michaela Froelian", "Logan Dee"]
        );
        assert_eq!(
            parse_people("Team Bastei mit KI"),
            vec!["Team Bastei mit KI"]
        );
        assert!(parse_people("").is_empty());
    }

    #[test]
    fn parse_infobox_fields_keeps_multiline_values() {
        let fixture = include_str!("../../tests/fixtures/john_sinclair/detail.wiki");
        let fields = parse_infobox_fields(fixture);
        assert_eq!(
            fields.get("Autoren"),
            Some(&"[[Ian Rolf Hill]]".to_string())
        );
        assert_eq!(fields.get("Teil"), Some(&"4/4".to_string()));
        assert_eq!(
            fields.get("Besonderes"),
            Some(&"[[Metatron]] wird vernichtet.\nDie Engel werden erweckt.".to_string())
        );
    }

    #[test]
    fn cover_url_is_selected_by_issue_specific_file_title() {
        let cover = serde_json::json!({
            "query": { "pages": {
                "1": {
                    "title": "Datei:Gruselroman-Wiki Logo.png",
                    "imageinfo": [{ "url": "https://example.test/logo.png" }]
                },
                "2": {
                    "title": "Datei:JS 2391.jpg",
                    "imageinfo": [{ "url": "https://example.test/cover.jpg" }]
                },
                "3": {
                    "title": "Datei:JS 2392.jpg",
                    "imageinfo": [{ "url": "https://example.test/other-cover.jpg" }]
                }
            } }
        });
        assert_eq!(
            extract_cover_url(&cover, 2391).as_deref(),
            Some("https://example.test/cover.jpg")
        );
        assert_eq!(
            extract_cover_url(
                &serde_json::json!({ "query": { "pages": {
                    "1": {
                        "title": "Datei:Gruselroman-Wiki Logo.png",
                        "imageinfo": [{ "url": "https://example.test/logo.png" }]
                    }
                } } }),
                2391,
            ),
            None
        );
    }

    #[test]
    fn cover_title_matching_normalizes_historical_file_names() {
        assert!(is_issue_cover_title("Datei:Js0001.jpg", 1));
        assert!(is_issue_cover_title("Datei:JS 2391.jpg", 2391));
        assert!(!is_issue_cover_title("Datei:JS 2392.jpg", 2391));
        assert!(!is_issue_cover_title("Datei:Logo.png", 2391));
    }

    #[test]
    fn adapter_metadata_and_identity_match_mvp_series() {
        let adapter = JohnSinclairAdapter::new().unwrap();
        assert_eq!(adapter.name(), "john-sinclair");
        assert_eq!(adapter.display_name(), "Geisterjäger John Sinclair");
        assert_eq!(adapter.version(), "0.1");
        assert_eq!(adapter.delay, Duration::from_millis(DEFAULT_DELAY_MS));
        let descriptor = adapter.source_descriptor();
        assert_eq!(descriptor.source_key, "gruselroman-wiki");
        assert_eq!(descriptor.series_record_id, "JS_Romanhefte");
        assert_eq!(descriptor.allowed_host, "www.gruselroman-wiki.de");
    }

    #[tokio::test]
    async fn metadata_is_for_first_regular_series() {
        let adapter = JohnSinclairAdapter::new().unwrap();
        let metadata = adapter.fetch_series_metadata().await.unwrap();
        assert_eq!(metadata.slug, "john-sinclair");
        assert_eq!(metadata.frequency.as_deref(), Some("wöchentlich"));
        assert_eq!(metadata.status, SeriesStatus::Running);
        assert_eq!(metadata.source.source_key, "gruselroman-wiki");
    }

    #[test]
    fn reference_issues_map_to_expected_metadata_and_provenance() {
        const REFERENCE_OVERVIEW: &str =
            include_str!("../../tests/fixtures/john_sinclair/reference-overview.wiki");
        let summaries = JohnSinclairAdapter::parse_overview(REFERENCE_OVERVIEW).unwrap();
        let references = [
            (
                1,
                "Im Nachtclub der Vampire",
                "Jason Dark",
                NaiveDate::from_ymd_opt(1978, 1, 17).unwrap(),
                include_str!("../../tests/fixtures/john_sinclair/js0001.wiki"),
            ),
            (
                1000,
                "Das Schwert des Salomo",
                "Jason Dark",
                NaiveDate::from_ymd_opt(1997, 9, 1).unwrap(),
                include_str!("../../tests/fixtures/john_sinclair/js1000.wiki"),
            ),
            (
                2303,
                "Die Hure Babylon",
                "Ian Rolf Hill",
                NaiveDate::from_ymd_opt(2022, 8, 30).unwrap(),
                include_str!("../../tests/fixtures/john_sinclair/js2303.wiki"),
            ),
        ];

        for (number, title, author, date, fixture) in references {
            let summary = summaries
                .iter()
                .find(|summary| summary.issue_number == number)
                .unwrap()
                .clone();
            let issue = map_issue_details(summary, fixture);
            let issue = crate::adapter::normalize_and_validate_issue(
                SOURCE_DESCRIPTOR,
                number,
                issue,
            )
            .unwrap();
            assert_eq!(issue.title, title);
            assert_eq!(issue.authors, vec![author]);
            assert_eq!(issue.published_at, Some(date));
            assert_eq!(issue.source.source_key, "gruselroman-wiki");
            assert!(issue.source.source_record_id.starts_with(&format!("JS {number:04}")));
        }
    }

    #[test]
    fn fixed_today_supports_deterministic_publication_filtering() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        let adapter = JohnSinclairAdapter::new().unwrap().with_today(today);
        assert_eq!(adapter.today(), today);
    }

    #[test]
    fn publication_filter_keeps_unknown_and_published_but_excludes_future_issues() {
        let summaries = JohnSinclairAdapter::parse_overview(OVERVIEW_FIXTURE).unwrap();
        let published = JohnSinclairAdapter::filter_published(
            summaries,
            NaiveDate::from_ymd_opt(2026, 8, 8).unwrap(),
        );
        let numbers: Vec<u32> = published
            .into_iter()
            .map(|summary| summary.issue_number)
            .collect();
        assert!(
            numbers.contains(&11),
            "unknown historical date must be kept"
        );
        assert!(
            numbers.contains(&2509),
            "issue published today must be kept"
        );
        assert!(!numbers.contains(&2510), "future issue must be excluded");
    }

    #[test]
    fn with_delay_overrides_default() {
        let adapter = JohnSinclairAdapter::new()
            .unwrap()
            .with_delay(Duration::from_millis(1));
        assert_eq!(adapter.delay, Duration::from_millis(1));
    }
}
