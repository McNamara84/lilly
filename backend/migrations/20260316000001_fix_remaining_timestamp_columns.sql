-- Fix remaining TIMESTAMP columns that cause SQLx ColumnDecode errors
-- when used with chrono::NaiveDateTime in Rust.
-- Previous migration (20260315000007) fixed series, issues, and import_jobs.
-- This migration fixes users and refresh_tokens.

ALTER TABLE `users`
    MODIFY COLUMN `verification_token_expires_at` DATETIME NULL DEFAULT NULL,
    MODIFY COLUMN `privacy_consent_at` DATETIME NULL DEFAULT NULL,
    MODIFY COLUMN `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP;

ALTER TABLE `refresh_tokens`
    MODIFY COLUMN `expires_at` DATETIME NOT NULL,
    MODIFY COLUMN `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP;
