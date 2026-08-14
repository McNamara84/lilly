ALTER TABLE issues
    ADD COLUMN cover_source_file VARCHAR(255) NULL AFTER cover_local_path,
    ADD COLUMN cover_source_sha1 CHAR(40) NULL AFTER cover_source_file,
    ADD COLUMN cover_source_updated_at DATETIME NULL AFTER cover_source_sha1;
