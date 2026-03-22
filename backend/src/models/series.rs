use serde::Serialize;

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct Series {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub frequency: Option<String>,
    pub total_issues: Option<u32>,
    pub status: String,
    pub active: bool,
    pub source_url: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct SeriesResponse {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub frequency: Option<String>,
    pub total_issues: Option<u32>,
    pub status: String,
    pub active: bool,
    pub source_url: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct Issue {
    pub id: u32,
    pub series_id: u32,
    #[allow(clippy::struct_field_names)]
    pub issue_number: u32,
    pub title: String,
    pub published_at: Option<chrono::NaiveDate>,
    pub cycle: Option<String>,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub source_wiki_url: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct IssueResponse {
    pub id: u32,
    pub series_id: u32,
    #[allow(clippy::struct_field_names)]
    pub issue_number: u32,
    pub title: String,
    pub authors: Vec<String>,
    pub published_at: Option<chrono::NaiveDate>,
    pub cycle: Option<String>,
    pub cover_artists: Vec<String>,
    pub keywords: Vec<String>,
    pub notes: Vec<String>,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub source_wiki_url: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct ImportJob {
    pub id: u32,
    pub series_id: u32,
    pub adapter_name: String,
    pub status: String,
    pub total_issues: u32,
    pub imported_issues: u32,
    pub error_message: Option<String>,
    pub started_by: u32,
    pub started_at: Option<chrono::NaiveDateTime>,
    pub completed_at: Option<chrono::NaiveDateTime>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ImportJobResponse {
    pub id: u32,
    pub series_id: u32,
    pub series_slug: String,
    pub adapter_name: String,
    pub status: String,
    pub total_issues: u32,
    pub imported_issues: u32,
    pub error_message: Option<String>,
    pub started_by: u32,
    pub started_at: Option<chrono::NaiveDateTime>,
    pub completed_at: Option<chrono::NaiveDateTime>,
}

impl From<&Series> for SeriesResponse {
    fn from(s: &Series) -> Self {
        Self {
            id: s.id,
            name: s.name.clone(),
            slug: s.slug.clone(),
            publisher: s.publisher.clone(),
            genre: s.genre.clone(),
            frequency: s.frequency.clone(),
            total_issues: s.total_issues,
            status: s.status.clone(),
            active: s.active,
            source_url: s.source_url.clone(),
        }
    }
}

impl IssueResponse {
    /// Build an `IssueResponse` from an `Issue` row plus its related data.
    pub fn from_issue_with_relations(
        i: &Issue,
        authors: Vec<String>,
        cover_artists: Vec<String>,
        keywords: Vec<String>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            id: i.id,
            series_id: i.series_id,
            issue_number: i.issue_number,
            title: i.title.clone(),
            authors,
            published_at: i.published_at,
            cycle: i.cycle.clone(),
            cover_artists,
            keywords,
            notes,
            cover_url: i.cover_url.clone(),
            cover_local_path: i.cover_local_path.clone(),
            source_wiki_url: i.source_wiki_url.clone(),
        }
    }
}

impl ImportJobResponse {
    pub fn from_job_with_slug(j: &ImportJob, series_slug: String) -> Self {
        Self {
            id: j.id,
            series_id: j.series_id,
            series_slug,
            adapter_name: j.adapter_name.clone(),
            status: j.status.clone(),
            total_issues: j.total_issues,
            imported_issues: j.imported_issues,
            error_message: j.error_message.clone(),
            started_by: j.started_by,
            started_at: j.started_at,
            completed_at: j.completed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_series_response_from_series() {
        let series = Series {
            id: 1,
            name: "Maddrax".to_string(),
            slug: "maddrax".to_string(),
            publisher: Some("Bastei".to_string()),
            genre: Some("Science Fiction".to_string()),
            frequency: Some("weekly".to_string()),
            total_issues: Some(620),
            status: "running".to_string(),
            active: true,
            source_url: Some("https://maddraxikon.de".to_string()),
            created_at: chrono::NaiveDateTime::default(),
            updated_at: chrono::NaiveDateTime::default(),
        };
        let resp = SeriesResponse::from(&series);
        assert_eq!(resp.id, 1);
        assert_eq!(resp.name, "Maddrax");
        assert_eq!(resp.slug, "maddrax");
        assert!(resp.active);
    }

    #[test]
    fn test_issue_response_from_issue() {
        let issue = Issue {
            id: 1,
            series_id: 1,
            issue_number: 42,
            title: "Die Welt bricht auseinander".to_string(),
            published_at: None,
            cycle: Some("Zyklus 3".to_string()),
            cover_url: None,
            cover_local_path: None,
            source_wiki_url: None,
            created_at: chrono::NaiveDateTime::default(),
        };
        let resp = IssueResponse::from_issue_with_relations(
            &issue,
            vec!["Jo Zybell".to_string()],
            vec!["Koveck".to_string()],
            vec!["Erde".to_string(), "Parallelwelt".to_string()],
            vec!["Jubiläumsausgabe".to_string()],
        );
        assert_eq!(resp.issue_number, 42);
        assert_eq!(resp.title, "Die Welt bricht auseinander");
        assert_eq!(resp.authors, vec!["Jo Zybell"]);
        assert_eq!(resp.cover_artists, vec!["Koveck"]);
        assert_eq!(resp.keywords, vec!["Erde", "Parallelwelt"]);
        assert_eq!(resp.notes, vec!["Jubiläumsausgabe"]);
    }

    #[test]
    fn test_import_job_response_from_job() {
        let job = ImportJob {
            id: 1,
            series_id: 1,
            adapter_name: "maddrax".to_string(),
            status: "running".to_string(),
            total_issues: 620,
            imported_issues: 150,
            error_message: None,
            started_by: 1,
            started_at: Some(chrono::NaiveDateTime::default()),
            completed_at: None,
            created_at: chrono::NaiveDateTime::default(),
        };
        let resp = ImportJobResponse::from_job_with_slug(&job, "maddrax".to_string());
        assert_eq!(resp.adapter_name, "maddrax");
        assert_eq!(resp.series_slug, "maddrax");
        assert_eq!(resp.total_issues, 620);
        assert_eq!(resp.imported_issues, 150);
    }

    #[test]
    fn test_series_response_serialization() {
        let resp = SeriesResponse {
            id: 1,
            name: "Test".to_string(),
            slug: "test".to_string(),
            publisher: None,
            genre: None,
            frequency: None,
            total_issues: None,
            status: "running".to_string(),
            active: false,
            source_url: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["active"], false);
    }
}
