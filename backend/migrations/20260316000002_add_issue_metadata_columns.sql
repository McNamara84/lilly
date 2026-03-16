-- Add additional metadata columns to issues table for cover artist, keywords, and notes
ALTER TABLE `issues`
    ADD COLUMN `cover_artist` VARCHAR(255) NULL DEFAULT NULL AFTER `cycle`,
    ADD COLUMN `keywords` TEXT NULL DEFAULT NULL AFTER `cover_artist`,
    ADD COLUMN `notes` TEXT NULL DEFAULT NULL AFTER `keywords`;
