-- Collection entries: tracks which issues a user owns, wants, or has as duplicates.
-- Supports multiple copies per issue (SV-009) via copy_number.

CREATE TABLE IF NOT EXISTS collection_entries (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id INT UNSIGNED NOT NULL,
    issue_id INT UNSIGNED NOT NULL,
    copy_number TINYINT UNSIGNED NOT NULL DEFAULT 1,
    condition_grade ENUM('Z0', 'Z1', 'Z2', 'Z3', 'Z4', 'Z5') NOT NULL,
    status ENUM('owned', 'duplicate', 'wanted') NOT NULL DEFAULT 'owned',
    notes TEXT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_collection_user_issue_copy (user_id, issue_id, copy_number)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Photo storage for collection entries (table prepared for future photo upload feature).

CREATE TABLE IF NOT EXISTS collection_photos (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    entry_id INT UNSIGNED NOT NULL,
    file_path VARCHAR(500) NOT NULL,
    sort_order TINYINT NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (entry_id) REFERENCES collection_entries(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
