CREATE TABLE IF NOT EXISTS import_jobs (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    series_id INT UNSIGNED NOT NULL,
    adapter_name VARCHAR(100) NOT NULL,
    status ENUM('pending', 'running', 'completed', 'failed') NOT NULL DEFAULT 'pending',
    total_issues INT UNSIGNED NOT NULL DEFAULT 0,
    imported_issues INT UNSIGNED NOT NULL DEFAULT 0,
    error_message TEXT NULL,
    started_by INT UNSIGNED NOT NULL,
    started_at TIMESTAMP NULL,
    completed_at TIMESTAMP NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (series_id) REFERENCES series(id) ON DELETE CASCADE,
    FOREIGN KEY (started_by) REFERENCES users(id),
    INDEX idx_import_jobs_series (series_id),
    INDEX idx_import_jobs_status (status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
