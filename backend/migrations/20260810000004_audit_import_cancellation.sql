ALTER TABLE import_jobs
    ADD COLUMN cancel_requested_by INT UNSIGNED NULL AFTER cancel_requested_at;

ALTER TABLE import_jobs
    ADD CONSTRAINT fk_import_jobs_cancel_requested_by
        FOREIGN KEY (cancel_requested_by) REFERENCES users(id) ON DELETE SET NULL,
    ADD INDEX idx_import_jobs_cancel_requested_by (cancel_requested_by);

-- MariaDB rejects a CHECK that references an ON DELETE SET NULL foreign-key
-- column (error 1901). These triggers enforce the same invariant while keeping
-- the actor reference nullable for audit retention after account deletion.
CREATE TRIGGER trg_import_jobs_cancel_audit_insert
BEFORE INSERT ON import_jobs
FOR EACH ROW
SET NEW.cancel_requested_at = CASE
    WHEN NEW.cancel_requested_by IS NOT NULL AND NEW.cancel_requested_at IS NULL
        THEN CURRENT_TIMESTAMP
    ELSE NEW.cancel_requested_at
END;

CREATE TRIGGER trg_import_jobs_cancel_audit_update
BEFORE UPDATE ON import_jobs
FOR EACH ROW
SET NEW.cancel_requested_at = CASE
    WHEN NEW.cancel_requested_by IS NOT NULL AND NEW.cancel_requested_at IS NULL
        THEN CURRENT_TIMESTAMP
    ELSE NEW.cancel_requested_at
END;
