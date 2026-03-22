CREATE TABLE IF NOT EXISTS series (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL,
    publisher VARCHAR(255) NULL,
    genre VARCHAR(100) NULL,
    frequency VARCHAR(50) NULL,
    total_issues INT UNSIGNED NULL,
    status ENUM('running', 'completed', 'cancelled') NOT NULL DEFAULT 'running',
    active BOOLEAN NOT NULL DEFAULT FALSE,
    source_url VARCHAR(500) NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    UNIQUE INDEX idx_series_name (name),
    UNIQUE INDEX idx_series_slug (slug)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS issues (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    series_id INT UNSIGNED NOT NULL,
    issue_number INT UNSIGNED NOT NULL,
    title VARCHAR(500) NOT NULL,
    author VARCHAR(500) NULL,
    published_at DATE NULL,
    cycle VARCHAR(255) NULL,
    cover_url VARCHAR(500) NULL,
    cover_local_path VARCHAR(500) NULL,
    source_wiki_url VARCHAR(500) NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (series_id) REFERENCES series(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_issues_series_number (series_id, issue_number)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
