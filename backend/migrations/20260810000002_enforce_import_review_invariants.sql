ALTER TABLE import_job_results
    ADD CONSTRAINT chk_import_job_results_positive_number CHECK (issue_number > 0),
    ADD CONSTRAINT chk_import_job_results_terminal_time
        CHECK (outcome = 'not_processed' OR processed_at IS NOT NULL),
    ADD CONSTRAINT chk_import_job_results_failed_blocking
        CHECK (outcome <> 'failed' OR severity = 'blocking');

ALTER TABLE series_publication_events
    ADD CONSTRAINT chk_series_publication_activation_decision
        CHECK (action <> 'activated' OR decision IS NOT NULL),
    ADD CONSTRAINT chk_series_publication_no_blockers
        CHECK (action <> 'activated' OR blocking_count = 0);
