use serde::{Deserialize, Serialize};

use crate::models::collection::CollectionEntryRow;

#[derive(Debug, sqlx::FromRow)]
pub struct OwnProfileRow {
    pub id: u32,
    pub email: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub location: Option<String>,
    pub profile_public: bool,
    pub collection_public: bool,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct OwnProfileResponse {
    pub id: u32,
    pub email: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub location: Option<String>,
    pub profile_public: bool,
    pub collection_public: bool,
    pub created_at: chrono::NaiveDateTime,
}

impl From<OwnProfileRow> for OwnProfileResponse {
    fn from(row: OwnProfileRow) -> Self {
        Self {
            id: row.id,
            email: row.email,
            display_name: row.display_name,
            avatar_path: row.avatar_path,
            location: row.location,
            profile_public: row.profile_public,
            collection_public: row.collection_public,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateVisibilityRequest {
    pub profile_public: bool,
    pub collection_public: bool,
}

#[derive(Debug, Serialize)]
pub struct VisibilityResponse {
    pub profile_public: bool,
    pub collection_public: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PublicProfileRow {
    pub id: u32,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub location: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct PublicProfileResponse {
    pub id: u32,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub location: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

impl From<PublicProfileRow> for PublicProfileResponse {
    fn from(row: PublicProfileRow) -> Self {
        Self {
            id: row.id,
            display_name: row.display_name,
            avatar_path: row.avatar_path,
            location: row.location,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PublicCollectionEntryResponse {
    pub issue_id: u32,
    pub issue_number: u32,
    pub title: String,
    pub series_id: u32,
    pub series_name: String,
    pub series_slug: String,
    pub cover_url: Option<String>,
    pub cover_local_path: Option<String>,
    pub copy_number: u8,
    pub condition_grade: String,
    pub status: String,
    pub notes: Option<String>,
}

impl From<&CollectionEntryRow> for PublicCollectionEntryResponse {
    fn from(row: &CollectionEntryRow) -> Self {
        Self {
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            title: row.title.clone(),
            series_id: row.series_id,
            series_name: row.series_name.clone(),
            series_slug: row.series_slug.clone(),
            cover_url: row.cover_url.clone(),
            cover_local_path: row.cover_local_path.clone(),
            copy_number: row.copy_number,
            condition_grade: row.condition_grade.clone(),
            status: row.status.clone(),
            notes: row.notes.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedPublicCollectionResponse {
    pub data: Vec<PublicCollectionEntryResponse>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_profile_serialization_contains_only_public_fields() {
        let response = PublicProfileResponse {
            id: 7,
            display_name: "Collector".to_string(),
            avatar_path: Some("/media/avatar.webp".to_string()),
            location: Some("Berlin".to_string()),
            created_at: chrono::NaiveDateTime::default(),
        };

        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["id"], 7);
        assert_eq!(value["display_name"], "Collector");
        for private_field in [
            "email",
            "password_hash",
            "oauth_provider",
            "oauth_id",
            "role",
            "profile_public",
            "collection_public",
        ] {
            assert!(value.get(private_field).is_none());
        }
    }

    #[test]
    fn public_collection_entry_serialization_omits_owner_and_internal_entry_id() {
        let response = PublicCollectionEntryResponse {
            issue_id: 4,
            issue_number: 12,
            title: "Test".to_string(),
            series_id: 2,
            series_name: "Series".to_string(),
            series_slug: "series".to_string(),
            cover_url: None,
            cover_local_path: None,
            copy_number: 1,
            condition_grade: "Z2".to_string(),
            status: "owned".to_string(),
            notes: Some("Public note".to_string()),
        };

        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["notes"], "Public note");
        assert!(value.get("id").is_none());
        assert!(value.get("user_id").is_none());
        assert!(value.get("email").is_none());
    }
}
