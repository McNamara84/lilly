-- Preserve the first persisted cancellation audit pair on ordinary updates.
-- InnoDB applies ON DELETE SET NULL without invoking child-table triggers, so
-- deleting the referenced user can still clear cancel_requested_by while the
-- original timestamp remains available.
DROP TRIGGER IF EXISTS trg_import_jobs_cancel_audit_update;

CREATE TRIGGER trg_import_jobs_cancel_audit_update
BEFORE UPDATE ON import_jobs
FOR EACH ROW
SET
    NEW.cancel_requested_at = CASE
        WHEN OLD.cancel_requested_by IS NOT NULL
            THEN COALESCE(OLD.cancel_requested_at, CURRENT_TIMESTAMP)
        WHEN NEW.cancel_requested_by IS NOT NULL AND NEW.cancel_requested_at IS NULL
            THEN CURRENT_TIMESTAMP
        ELSE NEW.cancel_requested_at
    END,
    NEW.cancel_requested_by = COALESCE(OLD.cancel_requested_by, NEW.cancel_requested_by);
