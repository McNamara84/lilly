CREATE TABLE collection_transfer_keys (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id INT UNSIGNED NOT NULL,
    copy_key VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    entry_id INT UNSIGNED NULL,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    UNIQUE KEY uq_transfer_copy (user_id, copy_key),
    UNIQUE KEY uq_transfer_entry (entry_id),
    CONSTRAINT fk_transfer_key_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT fk_transfer_key_entry FOREIGN KEY (entry_id) REFERENCES collection_entries(id) ON DELETE SET NULL
) ENGINE=InnoDB;

INSERT INTO collection_transfer_keys (user_id, copy_key, entry_id)
SELECT user_id, CONCAT('lilly-', REPLACE(UUID(), '-', '')), id FROM collection_entries;

-- A database trigger also covers bulk wishes, trade completion and fixture writers.
CREATE TRIGGER collection_transfer_key_created AFTER INSERT ON collection_entries
FOR EACH ROW INSERT INTO collection_transfer_keys (user_id, copy_key, entry_id)
VALUES (NEW.user_id, CONCAT('lilly-', REPLACE(UUID(), '-', '')), NEW.id);

CREATE TABLE transfer_jobs (
    id CHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    user_id INT UNSIGNED NULL,
    kind ENUM('import', 'export') NOT NULL,
    format ENUM('csv', 'json', 'zip') NOT NULL,
    status ENUM('prepared', 'queued', 'running', 'completed', 'ready', 'failed', 'cancelled', 'expired') NOT NULL,
    created_at DATETIME(6) NOT NULL,
    completed_at DATETIME(6) NULL,
    snapshot_at DATETIME(6) NULL,
    expires_at DATETIME(6) NOT NULL,
    invalidated_at DATETIME(6) NULL,
    files_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    error_code VARCHAR(64) NULL,
    total_rows INT UNSIGNED NOT NULL DEFAULT 0,
    byte_size BIGINT UNSIGNED NOT NULL DEFAULT 0,
    source_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    erasure_job_id BIGINT UNSIGNED NULL,
    INDEX idx_transfer_owner (user_id, kind, created_at),
    INDEX idx_transfer_queue (status, created_at),
    INDEX idx_transfer_expiry (expires_at, files_deleted),
    CONSTRAINT fk_transfer_job_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT fk_transfer_job_erasure FOREIGN KEY (erasure_job_id) REFERENCES account_erasure_jobs(id) ON DELETE SET NULL
) ENGINE=InnoDB;

CREATE TABLE collection_import_results (
    job_id CHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    record_number INT UNSIGNED NOT NULL,
    copy_key VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NULL,
    status ENUM('new', 'created', 'unchanged', 'conflict', 'invalid', 'failed') NOT NULL,
    code VARCHAR(64) NULL,
    entry_id INT UNSIGNED NULL,
    PRIMARY KEY (job_id, record_number),
    CONSTRAINT fk_import_result_job FOREIGN KEY (job_id) REFERENCES transfer_jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE data_export_subjects (
    job_id CHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    subject_user_id INT UNSIGNED NULL,
    session_version INT UNSIGNED NOT NULL,
    account_state VARCHAR(32) NOT NULL,
    INDEX idx_export_subject (subject_user_id, job_id),
    UNIQUE KEY uq_export_subject (job_id, subject_user_id),
    CONSTRAINT fk_export_subject_job FOREIGN KEY (job_id) REFERENCES transfer_jobs(id) ON DELETE CASCADE,
    CONSTRAINT fk_export_subject_user FOREIGN KEY (subject_user_id) REFERENCES users(id) ON DELETE SET NULL
) ENGINE=InnoDB;
