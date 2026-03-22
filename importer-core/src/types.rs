use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SeriesStatus {
    Running,
    Completed,
    Cancelled,
}

impl fmt::Display for SeriesStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SeriesData {
    pub name: String,
    pub slug: String,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub frequency: Option<String>,
    pub total_issues: Option<u32>,
    pub status: SeriesStatus,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IssueData {
    pub issue_number: u32,
    pub title: String,
    pub authors: Vec<String>,
    pub published_at: Option<chrono::NaiveDate>,
    pub cycle: Option<String>,
    pub cover_artists: Vec<String>,
    pub keywords: Vec<String>,
    pub notes: Vec<String>,
    pub source_wiki_url: Option<String>,
}

#[derive(Debug)]
pub struct CoverData {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_series_status_display() {
        assert_eq!(SeriesStatus::Running.to_string(), "running");
        assert_eq!(SeriesStatus::Completed.to_string(), "completed");
        assert_eq!(SeriesStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_series_status_equality() {
        assert_eq!(SeriesStatus::Running, SeriesStatus::Running);
        assert_ne!(SeriesStatus::Running, SeriesStatus::Completed);
    }

    #[test]
    fn test_series_data_construction() {
        let data = SeriesData {
            name: "Maddrax".to_string(),
            slug: "maddrax".to_string(),
            publisher: Some("Bastei Lübbe".to_string()),
            genre: Some("Science-Fiction".to_string()),
            frequency: Some("14-tägig".to_string()),
            total_issues: Some(620),
            status: SeriesStatus::Running,
            source_url: Some("https://maddraxikon.de".to_string()),
        };
        assert_eq!(data.name, "Maddrax");
        assert_eq!(data.status, SeriesStatus::Running);
    }

    #[test]
    fn test_issue_data_construction() {
        let data = IssueData {
            issue_number: 1,
            title: "Dunkle Zukunft".to_string(),
            authors: vec!["Jo Zybell".to_string()],
            published_at: None,
            cycle: Some("Euree".to_string()),
            cover_artists: vec!["Koveck".to_string()],
            keywords: vec!["Kometeneinschlag".to_string(), "Taratzen".to_string()],
            notes: vec![],
            source_wiki_url: None,
        };
        assert_eq!(data.issue_number, 1);
        assert_eq!(data.title, "Dunkle Zukunft");
        assert_eq!(data.cover_artists[0], "Koveck");
        assert_eq!(data.keywords.len(), 2);
    }

    #[test]
    fn test_cover_data_construction() {
        let data = CoverData {
            bytes: vec![0xFF, 0xD8, 0xFF],
            content_type: "image/jpeg".to_string(),
        };
        assert_eq!(data.content_type, "image/jpeg");
        assert_eq!(data.bytes.len(), 3);
    }
}
