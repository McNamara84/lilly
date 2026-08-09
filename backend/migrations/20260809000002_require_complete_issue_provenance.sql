-- The preceding migration can derive Maddrax record IDs from the issue number,
-- but John Sinclair record IDs are canonical wiki page titles and cannot be
-- reconstructed losslessly from the legacy relational columns. Remove the
-- partial identity; the required initial full synchronization repairs both
-- values atomically from the authoritative overview page.
UPDATE issues i
JOIN series s ON s.id = i.series_id
SET i.source_key = NULL
WHERE s.slug = 'john-sinclair'
  AND i.source_key = 'gruselroman-wiki'
  AND i.source_record_id IS NULL;

ALTER TABLE issues
    ADD CONSTRAINT chk_issues_source_identity_complete
        CHECK (
            (source_key IS NULL AND source_record_id IS NULL)
            OR (source_key IS NOT NULL AND source_record_id IS NOT NULL)
        );
