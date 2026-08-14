use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::models::collection::CollectionEntryRow;

pub const MIN_DISPLAY_NAME_LENGTH: usize = 2;
pub const MAX_DISPLAY_NAME_LENGTH: usize = 100;
pub const MAX_LOCATION_LENGTH: usize = 255;

#[must_use]
pub fn avatar_content_url(user_id: u32, has_avatar: bool) -> Option<String> {
    has_avatar.then(|| format!("/api/v1/users/{user_id}/avatar"))
}

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
    pub avatar_url: Option<String>,
    pub location: Option<String>,
    pub profile_public: bool,
    pub collection_public: bool,
    pub created_at: chrono::NaiveDateTime,
}

impl From<OwnProfileRow> for OwnProfileResponse {
    fn from(row: OwnProfileRow) -> Self {
        let avatar_url = avatar_content_url(row.id, row.avatar_path.is_some());
        Self {
            id: row.id,
            email: row.email,
            display_name: row.display_name,
            avatar_url,
            location: row.location,
            profile_public: row.profile_public,
            collection_public: row.collection_public,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: String,
    pub location: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NormalizedProfileUpdate {
    pub display_name: String,
    pub location: Option<String>,
}

impl UpdateProfileRequest {
    pub fn normalize(self) -> Result<NormalizedProfileUpdate, BTreeMap<String, String>> {
        let display_name = self.display_name.trim().to_string();
        let location = self.location.and_then(|value| {
            let value = value.trim().to_string();
            (!value.is_empty()).then_some(value)
        });
        let mut fields = BTreeMap::new();
        let display_name_length = display_name.chars().count();
        if !(MIN_DISPLAY_NAME_LENGTH..=MAX_DISPLAY_NAME_LENGTH).contains(&display_name_length) {
            fields.insert(
                "display_name".to_string(),
                format!(
                    "Display name must be {MIN_DISPLAY_NAME_LENGTH}–{MAX_DISPLAY_NAME_LENGTH} characters"
                ),
            );
        }
        if location
            .as_ref()
            .is_some_and(|value| value.chars().count() > MAX_LOCATION_LENGTH)
        {
            fields.insert(
                "location".to_string(),
                format!("Location must not exceed {MAX_LOCATION_LENGTH} characters"),
            );
        }
        if fields.is_empty() {
            Ok(NormalizedProfileUpdate {
                display_name,
                location,
            })
        } else {
            Err(fields)
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
    pub avatar_url: Option<String>,
    pub location: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

impl From<PublicProfileRow> for PublicProfileResponse {
    fn from(row: PublicProfileRow) -> Self {
        let avatar_url = avatar_content_url(row.id, row.avatar_path.is_some());
        Self {
            id: row.id,
            display_name: row.display_name,
            avatar_url,
            location: row.location,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct AvatarRow {
    pub user_id: u32,
    pub storage_key: String,
    pub profile_public: bool,
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
    pub condition_grade: Option<String>,
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
            avatar_url: Some("/api/v1/users/7/avatar".to_string()),
            location: Some("Berlin".to_string()),
            created_at: chrono::NaiveDateTime::default(),
        };

        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["id"], 7);
        assert_eq!(value["display_name"], "Collector");
        assert_eq!(value["avatar_url"], "/api/v1/users/7/avatar");
        assert!(value.get("avatar_path").is_none());
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
    fn profile_update_trims_values_and_normalizes_an_empty_location() {
        let normalized = UpdateProfileRequest {
            display_name: "  Sammlerin 📚  ".to_string(),
            location: Some("   ".to_string()),
        }
        .normalize()
        .unwrap();

        assert_eq!(normalized.display_name, "Sammlerin 📚");
        assert_eq!(normalized.location, None);
    }

    #[test]
    fn profile_update_validates_character_not_byte_lengths() {
        assert!(
            UpdateProfileRequest {
                display_name: "ÄÖ".to_string(),
                location: Some("Köln".to_string()),
            }
            .normalize()
            .is_ok()
        );

        let too_short = UpdateProfileRequest {
            display_name: " X ".to_string(),
            location: None,
        }
        .normalize()
        .unwrap_err();
        assert!(too_short.contains_key("display_name"));

        let too_long = UpdateProfileRequest {
            display_name: "X".repeat(MAX_DISPLAY_NAME_LENGTH + 1),
            location: Some("Y".repeat(MAX_LOCATION_LENGTH + 1)),
        }
        .normalize()
        .unwrap_err();
        assert!(too_long.contains_key("display_name"));
        assert!(too_long.contains_key("location"));
    }

    #[test]
    fn avatar_url_discloses_no_storage_key() {
        assert_eq!(
            avatar_content_url(42, true).as_deref(),
            Some("/api/v1/users/42/avatar")
        );
        assert_eq!(avatar_content_url(42, false), None);
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
            condition_grade: Some("Z2".to_string()),
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
