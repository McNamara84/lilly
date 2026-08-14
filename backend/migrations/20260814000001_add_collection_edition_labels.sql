-- Distinguish publication editions from physical copy numbers. NULL keeps
-- existing entries backwards compatible and means "edition not specified".
ALTER TABLE collection_entries
    ADD COLUMN edition_label VARCHAR(120) NULL AFTER copy_number;

-- Trade proposals are immutable snapshots. Keep both the offered edition and
-- a possibly edition-specific wanted constraint for later validation.
ALTER TABLE trade_items
    ADD COLUMN edition_label_snapshot VARCHAR(120) NULL AFTER copy_number_snapshot,
    ADD COLUMN wanted_edition_label_snapshot VARCHAR(120) NULL AFTER edition_label_snapshot;
