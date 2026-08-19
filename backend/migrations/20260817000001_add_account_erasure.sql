-- Account-erasure lifecycle, revocable sessions, and anonymised shared history.

ALTER TABLE users
    ADD COLUMN account_state ENUM('active', 'pending_deletion')
        NOT NULL DEFAULT 'active' AFTER email_verified,
    ADD COLUMN session_version INT UNSIGNED NOT NULL DEFAULT 0 AFTER account_state,
    ADD COLUMN erasure_subject CHAR(64) NULL AFTER session_version;

UPDATE users
SET erasure_subject = LOWER(HEX(RANDOM_BYTES(32)))
WHERE erasure_subject IS NULL;

ALTER TABLE users
    MODIFY COLUMN erasure_subject CHAR(64) NOT NULL,
    ADD UNIQUE INDEX idx_users_erasure_subject (erasure_subject),
    ADD INDEX idx_users_account_state (account_state);

CREATE TRIGGER users_assign_erasure_subject
    BEFORE INSERT ON users
    FOR EACH ROW
    SET NEW.erasure_subject = COALESCE(
        NULLIF(NEW.erasure_subject, ''),
        LOWER(HEX(RANDOM_BYTES(32)))
    );

ALTER TABLE refresh_tokens
    ADD COLUMN authenticated_at DATETIME(6) NULL AFTER expires_at;

UPDATE refresh_tokens
SET authenticated_at = created_at
WHERE authenticated_at IS NULL;

ALTER TABLE refresh_tokens
    MODIFY COLUMN authenticated_at DATETIME(6) NOT NULL;

CREATE TABLE account_erasure_jobs (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id INT UNSIGNED NULL,
    status ENUM('scheduled', 'running', 'storage_pending', 'failed', 'completed')
        NOT NULL DEFAULT 'scheduled',
    previous_profile_public BOOLEAN NOT NULL,
    previous_collection_public BOOLEAN NOT NULL,
    requested_at DATETIME(6) NOT NULL,
    scheduled_for DATETIME(6) NOT NULL,
    started_at DATETIME(6) NULL,
    completed_at DATETIME(6) NULL,
    ledger_recorded_at DATETIME(6) NULL,
    attempts INT UNSIGNED NOT NULL DEFAULT 0,
    next_retry_at DATETIME(6) NULL,
    last_error_category VARCHAR(64) NULL,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT fk_account_erasure_jobs_user FOREIGN KEY (user_id)
        REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE INDEX idx_account_erasure_jobs_user (user_id),
    INDEX idx_account_erasure_jobs_due (status, scheduled_for, next_retry_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE account_erasure_recovery_tokens (
    token_hash CHAR(64) NOT NULL PRIMARY KEY,
    user_id INT UNSIGNED NOT NULL,
    created_at DATETIME(6) NOT NULL,
    expires_at DATETIME(6) NOT NULL,
    consumed_at DATETIME(6) NULL,
    CONSTRAINT fk_account_erasure_recovery_user FOREIGN KEY (user_id)
        REFERENCES users(id) ON DELETE CASCADE,
    INDEX idx_account_erasure_recovery_user (user_id, consumed_at, expires_at),
    INDEX idx_account_erasure_recovery_expiry (expires_at, consumed_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

ALTER TABLE media_deletion_jobs
    ADD COLUMN erasure_job_id BIGINT UNSIGNED NULL AFTER storage_key,
    ADD CONSTRAINT fk_media_deletion_jobs_erasure FOREIGN KEY (erasure_job_id)
        REFERENCES account_erasure_jobs(id) ON DELETE SET NULL,
    ADD INDEX idx_media_deletion_jobs_erasure (erasure_job_id, processed_at);

-- Terminal trades and their messages belong to both participants. Preserve the
-- shared history while making every reference to an erased participant nullable.
ALTER TABLE trades
    DROP CONSTRAINT chk_trade_distinct_users,
    DROP FOREIGN KEY fk_trade_match,
    DROP FOREIGN KEY fk_trade_initiator,
    DROP FOREIGN KEY fk_trade_responder,
    MODIFY COLUMN match_id INT UNSIGNED NULL,
    MODIFY COLUMN initiator_id INT UNSIGNED NULL,
    MODIFY COLUMN responder_id INT UNSIGNED NULL,
    ADD CONSTRAINT fk_trade_match FOREIGN KEY (match_id)
        REFERENCES trade_matches(id) ON DELETE SET NULL,
    ADD CONSTRAINT fk_trade_initiator FOREIGN KEY (initiator_id)
        REFERENCES users(id) ON DELETE SET NULL,
    ADD CONSTRAINT fk_trade_responder FOREIGN KEY (responder_id)
        REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE trade_items
    DROP CONSTRAINT chk_trade_item_distinct_users,
    MODIFY COLUMN offered_by_user_id INT UNSIGNED NULL,
    MODIFY COLUMN receiving_user_id INT UNSIGNED NULL,
    ADD CONSTRAINT fk_trade_item_offerer FOREIGN KEY (offered_by_user_id)
        REFERENCES users(id) ON DELETE SET NULL,
    ADD CONSTRAINT fk_trade_item_receiver FOREIGN KEY (receiving_user_id)
        REFERENCES users(id) ON DELETE SET NULL;

-- MariaDB rejects CHECK constraints that reference ON DELETE SET NULL
-- foreign-key columns (error 1901). Triggers preserve the distinct-user
-- invariant for application writes; FK cascade actions do not invoke triggers.
CREATE TRIGGER trg_trades_distinct_users_insert
BEFORE INSERT ON trades
FOR EACH ROW
BEGIN
    IF NEW.initiator_id IS NOT NULL AND NEW.initiator_id = NEW.responder_id THEN
        SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'Trade participants must be distinct';
    END IF;
END;

CREATE TRIGGER trg_trades_distinct_users_update
BEFORE UPDATE ON trades
FOR EACH ROW
BEGIN
    IF NEW.initiator_id IS NOT NULL AND NEW.initiator_id = NEW.responder_id THEN
        SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'Trade participants must be distinct';
    END IF;
END;

CREATE TRIGGER trg_trade_items_distinct_users_insert
BEFORE INSERT ON trade_items
FOR EACH ROW
BEGIN
    IF NEW.offered_by_user_id IS NOT NULL
        AND NEW.offered_by_user_id = NEW.receiving_user_id THEN
        SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'Trade item participants must be distinct';
    END IF;
END;

CREATE TRIGGER trg_trade_items_distinct_users_update
BEFORE UPDATE ON trade_items
FOR EACH ROW
BEGIN
    IF NEW.offered_by_user_id IS NOT NULL
        AND NEW.offered_by_user_id = NEW.receiving_user_id THEN
        SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'Trade item participants must be distinct';
    END IF;
END;

ALTER TABLE messages
    DROP FOREIGN KEY fk_message_sender,
    MODIFY COLUMN sender_id INT UNSIGNED NULL,
    ADD CONSTRAINT fk_message_sender FOREIGN KEY (sender_id)
        REFERENCES users(id) ON DELETE SET NULL;

-- Historic import runs must not block an administrator from erasing an account.
ALTER TABLE import_jobs
    DROP CONSTRAINT chk_import_jobs_trigger,
    DROP FOREIGN KEY `2`,
    MODIFY COLUMN started_by INT UNSIGNED NULL,
    ADD CONSTRAINT fk_import_jobs_started_by FOREIGN KEY (started_by)
        REFERENCES users(id) ON DELETE SET NULL,
    ADD CONSTRAINT chk_import_jobs_trigger CHECK (
        (trigger_type = 'manual' AND scheduled_for IS NULL)
        OR
        (trigger_type = 'scheduled' AND scheduled_for IS NOT NULL)
    );

-- Reauthentication reuses the OAuth PKCE machinery but is explicitly bound to
-- the already authenticated local account.
ALTER TABLE oauth_authorization_flows
    DROP CONSTRAINT chk_oauth_flows_intent,
    DROP CONSTRAINT chk_oauth_flows_consent,
    ADD COLUMN reauth_user_id INT UNSIGNED NULL AFTER intent,
    ADD CONSTRAINT fk_oauth_flows_reauth_user FOREIGN KEY (reauth_user_id)
        REFERENCES users(id) ON DELETE CASCADE,
    ADD CONSTRAINT chk_oauth_flows_intent CHECK (
        intent IN ('login', 'register', 'reauth')
    ),
    ADD CONSTRAINT chk_oauth_flows_consent CHECK (
        (intent = 'login'
            AND privacy_policy_version IS NULL
            AND consented_at IS NULL)
        OR
        (intent = 'register'
            AND privacy_policy_version IS NOT NULL
            AND consented_at IS NOT NULL)
        OR
        (intent = 'reauth'
            AND privacy_policy_version IS NULL
            AND consented_at IS NULL)
    );
