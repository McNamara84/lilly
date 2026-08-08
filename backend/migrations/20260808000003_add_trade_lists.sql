-- Wanted entries describe issues that are not physically owned yet and
-- therefore do not necessarily have a condition grade.
ALTER TABLE collection_entries
    MODIFY COLUMN condition_grade ENUM('Z0', 'Z1', 'Z2', 'Z3', 'Z4') NULL;

-- Supports active offer/wanted projections and the future issue-based
-- matching query without introducing duplicate trade-list tables.
CREATE INDEX idx_collection_status_issue_user
    ON collection_entries (status, issue_id, user_id);
