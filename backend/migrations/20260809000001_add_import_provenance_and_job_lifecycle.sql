ALTER TABLE series
    ADD COLUMN source_key VARCHAR(64) NULL AFTER active,
    ADD COLUMN source_record_id VARCHAR(255) NULL AFTER source_key;

UPDATE series
SET source_key = 'maddraxikon', source_record_id = 'Hauptseite'
WHERE slug = 'maddrax' AND source_key IS NULL;

UPDATE series
SET source_key = 'gruselroman-wiki', source_record_id = 'JS_Romanhefte'
WHERE slug = 'john-sinclair' AND source_key IS NULL;

ALTER TABLE series
    ADD UNIQUE INDEX idx_series_source_identity (source_key, source_record_id);

ALTER TABLE issues
    ADD COLUMN source_key VARCHAR(64) NULL AFTER cover_local_path,
    ADD COLUMN source_record_id VARCHAR(255) NULL AFTER source_key;

UPDATE issues i
JOIN series s ON s.id = i.series_id
SET i.source_key = s.source_key,
    i.source_record_id = CASE
        WHEN s.source_key = 'maddraxikon' THEN CONCAT('Quelle:MX', i.issue_number)
        ELSE i.source_record_id
    END
WHERE i.source_key IS NULL;

ALTER TABLE issues
    ADD UNIQUE INDEX idx_issues_source_identity (source_key, source_record_id);

ALTER TABLE import_jobs
    MODIFY COLUMN status ENUM(
        'pending',
        'running',
        'completed',
        'completed_with_errors',
        'failed',
        'cancelled',
        'interrupted'
    ) NOT NULL DEFAULT 'pending',
    ADD COLUMN source_key VARCHAR(64) NULL AFTER adapter_name,
    ADD COLUMN created_issues INT UNSIGNED NOT NULL DEFAULT 0 AFTER imported_issues,
    ADD COLUMN updated_issues INT UNSIGNED NOT NULL DEFAULT 0 AFTER created_issues,
    ADD COLUMN unchanged_issues INT UNSIGNED NOT NULL DEFAULT 0 AFTER updated_issues,
    ADD COLUMN skipped_issues INT UNSIGNED NOT NULL DEFAULT 0 AFTER unchanged_issues,
    ADD COLUMN updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP AFTER created_at,
    ADD COLUMN cancel_requested_at DATETIME NULL AFTER updated_at,
    ADD COLUMN retry_of_job_id INT UNSIGNED NULL AFTER cancel_requested_at,
    ADD CONSTRAINT fk_import_jobs_retry_of
        FOREIGN KEY (retry_of_job_id) REFERENCES import_jobs(id) ON DELETE SET NULL,
    ADD INDEX idx_import_jobs_retry_of (retry_of_job_id);

UPDATE import_jobs j
JOIN series s ON s.id = j.series_id
SET j.source_key = s.source_key
WHERE j.source_key IS NULL;

CREATE TABLE IF NOT EXISTS import_job_errors (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    job_id INT UNSIGNED NOT NULL,
    source_key VARCHAR(64) NOT NULL,
    issue_number INT UNSIGNED NULL,
    source_record_id VARCHAR(255) NULL,
    stage VARCHAR(64) NOT NULL,
    message TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (job_id) REFERENCES import_jobs(id) ON DELETE CASCADE,
    INDEX idx_import_job_errors_job (job_id, created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
