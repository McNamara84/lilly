-- Metadata dictionaries must preserve the spelling supplied for each issue.
-- Case/accent-insensitive uniqueness merged distinct source values and made
-- every later synchronization report the same unpersistable metadata change.
-- Existing IDs and issue links remain intact; subsequent imports backfill
-- source spellings that an earlier import collapsed.
ALTER TABLE persons
    MODIFY name VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;
ALTER TABLE keywords
    MODIFY name VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;
ALTER TABLE notes
    MODIFY text VARCHAR(500) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;
