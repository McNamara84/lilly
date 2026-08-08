-- Keep profile and collection visibility independent and privacy-preserving.
ALTER TABLE users
    ADD COLUMN collection_public BOOLEAN NOT NULL DEFAULT FALSE AFTER profile_public;

-- The collector-community scale ends at Z4. Existing Z5 entries map to the
-- new Z4 definition, which explicitly includes loose or missing pages.
UPDATE collection_entries
SET condition_grade = 'Z4'
WHERE condition_grade = 'Z5';

ALTER TABLE collection_entries
    MODIFY COLUMN condition_grade ENUM('Z0', 'Z1', 'Z2', 'Z3', 'Z4') NOT NULL;
