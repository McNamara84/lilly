CREATE TABLE account_erasure_transfer_jobs (
    erasure_job_id BIGINT UNSIGNED NOT NULL,
    transfer_job_id CHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    PRIMARY KEY (erasure_job_id, transfer_job_id),
    INDEX idx_erasure_transfer_job (transfer_job_id),
    CONSTRAINT fk_erasure_transfer_erasure FOREIGN KEY (erasure_job_id)
        REFERENCES account_erasure_jobs(id) ON DELETE CASCADE,
    CONSTRAINT fk_erasure_transfer_job FOREIGN KEY (transfer_job_id)
        REFERENCES transfer_jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB;
