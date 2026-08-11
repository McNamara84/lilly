-- Secure, normalised user-photo storage for physical collection entries.
-- The table existed as a placeholder and has not been exposed by the API yet.

ALTER TABLE collection_photos
    CHANGE COLUMN file_path storage_key VARCHAR(128) NOT NULL,
    MODIFY COLUMN sort_order TINYINT UNSIGNED NOT NULL DEFAULT 0,
    ADD COLUMN media_type VARCHAR(32) NOT NULL DEFAULT 'image/jpeg' AFTER storage_key,
    ADD COLUMN byte_size INT UNSIGNED NOT NULL DEFAULT 0 AFTER media_type,
    ADD COLUMN width INT UNSIGNED NOT NULL DEFAULT 0 AFTER byte_size,
    ADD COLUMN height INT UNSIGNED NOT NULL DEFAULT 0 AFTER width,
    ADD CONSTRAINT chk_collection_photos_sort_order CHECK (sort_order < 4),
    ADD UNIQUE INDEX idx_collection_photos_storage_key (storage_key),
    ADD UNIQUE INDEX idx_collection_photos_entry_slot (entry_id, sort_order);

-- Database rows and files cannot be deleted in one ACID transaction. Retaining
-- the storage key here makes deletion retryable after crashes and restarts.
CREATE TABLE IF NOT EXISTS media_deletion_jobs (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    storage_key VARCHAR(128) NOT NULL,
    attempts INT UNSIGNED NOT NULL DEFAULT 0,
    last_error VARCHAR(500) NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at DATETIME NULL,
    UNIQUE INDEX idx_media_deletion_storage_key (storage_key),
    INDEX idx_media_deletion_pending (processed_at, id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
