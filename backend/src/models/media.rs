use serde::Serialize;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CollectionPhotoRow {
    pub id: u32,
    pub owner_user_id: u32,
    pub collection_public: bool,
    pub storage_key: String,
    pub media_type: String,
    pub byte_size: u32,
    pub width: u32,
    pub height: u32,
    pub sort_order: u8,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionPhotoResponse {
    pub id: u32,
    pub content_url: String,
    pub sort_order: u8,
    pub media_type: String,
    pub byte_size: u32,
    pub width: u32,
    pub height: u32,
    pub created_at: chrono::NaiveDateTime,
}

impl From<&CollectionPhotoRow> for CollectionPhotoResponse {
    fn from(row: &CollectionPhotoRow) -> Self {
        Self {
            id: row.id,
            content_url: format!("/api/v1/collection-photos/{}/content", row.id),
            sort_order: row.sort_order,
            media_type: row.media_type.clone(),
            byte_size: row.byte_size,
            width: row.width,
            height: row.height,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PhotoPolicyResponse {
    pub allowed_media_types: [&'static str; 3],
    pub max_upload_bytes: usize,
    pub max_photos: u8,
    pub max_edge: u32,
}

#[derive(Debug, Clone)]
pub struct ProcessedPhoto {
    pub bytes: Vec<u8>,
    pub media_type: &'static str,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, sqlx::FromRow)]
pub struct MediaDeletionJob {
    pub id: u64,
    pub storage_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_hides_internal_storage_and_owner_data() {
        let row = CollectionPhotoRow {
            id: 17,
            owner_user_id: 4,
            collection_public: false,
            storage_key: "private-secret.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            byte_size: 123,
            width: 100,
            height: 200,
            sort_order: 2,
            created_at: chrono::NaiveDateTime::default(),
        };

        let value = serde_json::to_value(CollectionPhotoResponse::from(&row)).unwrap();
        assert_eq!(value["content_url"], "/api/v1/collection-photos/17/content");
        assert!(value.get("storage_key").is_none());
        assert!(value.get("owner_user_id").is_none());
        assert!(value.get("entry_id").is_none());
        assert!(value.get("collection_public").is_none());
    }
}
