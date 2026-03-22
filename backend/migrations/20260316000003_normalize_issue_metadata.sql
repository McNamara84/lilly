-- Normalize author, cover_artist, keywords and notes into separate tables with n:m relationships.

-- Persons table (for authors and cover artists)
CREATE TABLE IF NOT EXISTS persons (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    UNIQUE INDEX idx_persons_name (name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Junction: issue ↔ person with role
CREATE TABLE IF NOT EXISTS issue_persons (
    issue_id INT UNSIGNED NOT NULL,
    person_id INT UNSIGNED NOT NULL,
    role ENUM('author', 'cover_artist') NOT NULL,
    PRIMARY KEY (issue_id, person_id, role),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE,
    FOREIGN KEY (person_id) REFERENCES persons(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Keywords table
CREATE TABLE IF NOT EXISTS keywords (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    UNIQUE INDEX idx_keywords_name (name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Junction: issue ↔ keyword
CREATE TABLE IF NOT EXISTS issue_keywords (
    issue_id INT UNSIGNED NOT NULL,
    keyword_id INT UNSIGNED NOT NULL,
    PRIMARY KEY (issue_id, keyword_id),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE,
    FOREIGN KEY (keyword_id) REFERENCES keywords(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Notes table
CREATE TABLE IF NOT EXISTS notes (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    text VARCHAR(500) NOT NULL,
    UNIQUE INDEX idx_notes_text (text)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Junction: issue ↔ note
CREATE TABLE IF NOT EXISTS issue_notes (
    issue_id INT UNSIGNED NOT NULL,
    note_id INT UNSIGNED NOT NULL,
    PRIMARY KEY (issue_id, note_id),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE,
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Remove the old denormalized author column from issues
ALTER TABLE `issues`
    DROP COLUMN `author`;
