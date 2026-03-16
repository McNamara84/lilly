-- Fix TIMESTAMP columns to DATETIME for SQLx compatibility with chrono::NaiveDateTime.
-- MariaDB TIMESTAMP is stored as UTC and converted on retrieval, which causes
-- type mismatch errors in SQLx. DATETIME stores the value as-is.

ALTER TABLE series
    MODIFY COLUMN created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    MODIFY COLUMN updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP;

ALTER TABLE issues
    MODIFY COLUMN created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP;

ALTER TABLE import_jobs
    MODIFY COLUMN started_at DATETIME NULL,
    MODIFY COLUMN completed_at DATETIME NULL,
    MODIFY COLUMN created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP;
