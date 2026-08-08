ALTER TABLE issues
    ADD COLUMN part_number SMALLINT UNSIGNED NULL AFTER published_at,
    ADD COLUMN part_total SMALLINT UNSIGNED NULL AFTER part_number,
    ADD COLUMN metadata_synced_at DATETIME NULL AFTER source_wiki_url,
    ADD CONSTRAINT chk_issues_multipart
        CHECK (
            (part_number IS NULL AND part_total IS NULL)
            OR
            (part_number >= 1 AND part_total >= part_number)
        );

ALTER TABLE import_jobs
    MODIFY COLUMN started_by INT UNSIGNED NULL,
    MODIFY COLUMN status ENUM(
        'pending',
        'running',
        'completed',
        'completed_with_errors',
        'failed'
    ) NOT NULL DEFAULT 'pending',
    ADD COLUMN trigger_type ENUM('manual', 'scheduled') NOT NULL DEFAULT 'manual' AFTER adapter_name,
    ADD COLUMN scheduled_for DATETIME NULL AFTER trigger_type,
    ADD COLUMN failed_issues INT UNSIGNED NOT NULL DEFAULT 0 AFTER imported_issues,
    ADD CONSTRAINT chk_import_jobs_trigger
        CHECK (
            (trigger_type = 'manual' AND started_by IS NOT NULL AND scheduled_for IS NULL)
            OR
            (trigger_type = 'scheduled' AND started_by IS NULL AND scheduled_for IS NOT NULL)
        ),
    ADD UNIQUE INDEX idx_import_jobs_scheduled_slot (adapter_name, scheduled_for);
