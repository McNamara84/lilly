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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesData {
    pub name: String,
    pub slug: String,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub frequency: Option<String>,
    pub total_issues: Option<u32>,
    pub status: SeriesStatus,
    pub source: SourceReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueData {
    pub issue_number: u32,
    pub title: String,
    pub authors: Vec<String>,
    pub published_at: Option<chrono::NaiveDate>,
    pub part_number: Option<u32>,
    pub part_total: Option<u32>,
    pub cycle: Option<String>,
    pub cover_artists: Vec<String>,
    pub keywords: Vec<String>,
    pub notes: Vec<String>,
    pub source: SourceReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReference {
    pub source_key: String,
    pub source_record_id: String,
    pub source_url: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CoverData {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// Stable technical identity of one source image revision.
///
/// This intentionally contains no licence or attribution fields. Those belong
/// to the separate, broader cover-provenance work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverIdentity {
    pub file_name: String,
    pub source_sha1: String,
    pub source_updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CoverFetchResult {
    Missing,
    Unchanged(CoverIdentity),
    Downloaded {
        data: CoverData,
        identity: CoverIdentity,
    },
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
            source: SourceReference {
                source_key: "maddraxikon".to_string(),
                source_record_id: "Hauptseite".to_string(),
                source_url: "https://de.maddraxikon.com/wiki/Hauptseite".to_string(),
            },
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
            part_number: Some(1),
            part_total: Some(2),
            cycle: Some("Euree".to_string()),
            cover_artists: vec!["Koveck".to_string()],
            keywords: vec!["Kometeneinschlag".to_string(), "Taratzen".to_string()],
            notes: vec![],
            source: SourceReference {
                source_key: "maddraxikon".to_string(),
                source_record_id: "Quelle:MX1".to_string(),
                source_url: "https://de.maddraxikon.com/wiki/Dunkle_Zukunft".to_string(),
            },
        };
        assert_eq!(data.issue_number, 1);
        assert_eq!(data.title, "Dunkle Zukunft");
        assert_eq!(data.cover_artists[0], "Koveck");
        assert_eq!(data.keywords.len(), 2);
        assert_eq!(data.part_number, Some(1));
        assert_eq!(data.part_total, Some(2));
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

    #[test]
    fn test_cover_fetch_result_construction() {
        let identity = CoverIdentity {
            file_name: "005tibi.jpg".to_string(),
            source_sha1: "a2cd7afc8bc68e58cafbcdd30c947bb9ad44f04e".to_string(),
            source_updated_at: chrono::DateTime::parse_from_rfc3339("2012-11-25T10:07:13Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        };
        let result = CoverFetchResult::Downloaded {
            data: CoverData {
                bytes: vec![0xFF, 0xD8, 0xFF],
                content_type: "image/jpeg".to_string(),
            },
            identity: identity.clone(),
        };

        assert!(matches!(
            result,
            CoverFetchResult::Downloaded {
                identity: actual,
                ..
            } if actual == identity
        ));
    }
}
