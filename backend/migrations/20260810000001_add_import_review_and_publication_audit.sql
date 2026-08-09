ALTER TABLE import_job_errors
    ADD COLUMN severity ENUM('info', 'warning', 'blocking') NOT NULL DEFAULT 'blocking' AFTER stage,
    ADD COLUMN code VARCHAR(64) NOT NULL DEFAULT 'import_error' AFTER severity,
    ADD INDEX idx_import_job_errors_job_severity (job_id, severity);

CREATE TABLE IF NOT EXISTS import_job_results (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    job_id INT UNSIGNED NOT NULL,
    issue_id INT UNSIGNED NULL,
    issue_number INT UNSIGNED NOT NULL,
    outcome ENUM('not_processed', 'created', 'updated', 'unchanged', 'skipped', 'failed')
        NOT NULL DEFAULT 'not_processed',
    severity ENUM('info', 'warning', 'blocking') NOT NULL DEFAULT 'info',
    stage VARCHAR(64) NULL,
    message TEXT NULL,
    source_key VARCHAR(64) NOT NULL,
    source_record_id VARCHAR(255) NULL,
    source_url VARCHAR(500) NULL,
    title VARCHAR(500) NULL,
    authors_json TEXT NOT NULL,
    cover_artists_json TEXT NOT NULL,
    published_at DATE NULL,
    part_number INT UNSIGNED NULL,
    part_total INT UNSIGNED NULL,
    cycle VARCHAR(255) NULL,
    cover_status ENUM(
        'imported',
        'reused',
        'missing_at_source',
        'not_permitted',
        'fetch_failed',
        'invalid',
        'storage_failed',
        'not_checked'
    ) NOT NULL DEFAULT 'not_checked',
    cover_reason TEXT NULL,
    cover_local_path VARCHAR(500) NULL,
    processed_at DATETIME NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    CONSTRAINT fk_import_job_results_job
        FOREIGN KEY (job_id) REFERENCES import_jobs(id) ON DELETE CASCADE,
    CONSTRAINT fk_import_job_results_issue
        FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE SET NULL,
    UNIQUE INDEX idx_import_job_results_job_issue (job_id, issue_number),
    INDEX idx_import_job_results_job_outcome (job_id, outcome),
    INDEX idx_import_job_results_job_severity (job_id, severity),
    INDEX idx_import_job_results_job_cover (job_id, cover_status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS series_publication_events (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    series_id INT UNSIGNED NOT NULL,
    import_job_id INT UNSIGNED NULL,
    actor_user_id INT UNSIGNED NOT NULL,
    action ENUM('activated', 'deactivated') NOT NULL,
    decision ENUM('clean', 'warnings_acknowledged') NULL,
    warning_count INT UNSIGNED NOT NULL DEFAULT 0,
    blocking_count INT UNSIGNED NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_series_publication_events_series
        FOREIGN KEY (series_id) REFERENCES series(id) ON DELETE CASCADE,
    CONSTRAINT fk_series_publication_events_job
        FOREIGN KEY (import_job_id) REFERENCES import_jobs(id) ON DELETE SET NULL,
    CONSTRAINT fk_series_publication_events_actor
        FOREIGN KEY (actor_user_id) REFERENCES users(id) ON DELETE RESTRICT,
    INDEX idx_series_publication_events_series (series_id, created_at),
    INDEX idx_series_publication_events_job (import_job_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
