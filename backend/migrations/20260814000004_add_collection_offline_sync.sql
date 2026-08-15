ALTER TABLE collection_entries
    ADD COLUMN revision BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER notes;

-- Every write path, including trade completion, advances the optimistic-lock
-- revision. Keeping this in the database prevents individual callers from
-- accidentally bypassing conflict detection.
CREATE TRIGGER collection_entries_increment_revision
    BEFORE UPDATE ON collection_entries
    FOR EACH ROW
    SET NEW.revision = OLD.revision + 1;

CREATE TABLE collection_mutation_receipts (
    user_id INT UNSIGNED NOT NULL,
    mutation_id CHAR(36) NOT NULL,
    operation VARCHAR(16) NOT NULL,
    request_fingerprint CHAR(64) NOT NULL,
    result_json JSON NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, mutation_id),
    CONSTRAINT fk_collection_mutation_receipts_user
        FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    INDEX idx_collection_mutation_receipts_created_at (created_at)
);
