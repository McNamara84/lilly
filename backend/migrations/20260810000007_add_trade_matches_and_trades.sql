CREATE TABLE trade_matches (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_low_id INT UNSIGNED NOT NULL,
    user_high_id INT UNSIGNED NOT NULL,
    status ENUM('active', 'stale') NOT NULL DEFAULT 'active',
    fingerprint CHAR(64) NOT NULL,
    revision INT UNSIGNED NOT NULL DEFAULT 1,
    detected_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    changed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    stale_at DATETIME NULL,
    CONSTRAINT chk_trade_match_distinct_users CHECK (user_low_id < user_high_id),
    CONSTRAINT fk_trade_match_user_low FOREIGN KEY (user_low_id)
        REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_match_user_high FOREIGN KEY (user_high_id)
        REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_trade_matches_pair (user_low_id, user_high_id),
    INDEX idx_trade_matches_low_status (user_low_id, status, changed_at),
    INDEX idx_trade_matches_high_status (user_high_id, status, changed_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE trade_match_items (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    match_id INT UNSIGNED NOT NULL,
    offer_entry_id INT UNSIGNED NOT NULL,
    wanted_entry_id INT UNSIGNED NOT NULL,
    issue_id INT UNSIGNED NOT NULL,
    offered_by_user_id INT UNSIGNED NOT NULL,
    wanted_by_user_id INT UNSIGNED NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_trade_match_item_distinct_users
        CHECK (offered_by_user_id <> wanted_by_user_id),
    CONSTRAINT fk_trade_match_item_match FOREIGN KEY (match_id)
        REFERENCES trade_matches(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_match_item_offer FOREIGN KEY (offer_entry_id)
        REFERENCES collection_entries(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_match_item_wanted FOREIGN KEY (wanted_entry_id)
        REFERENCES collection_entries(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_match_item_issue FOREIGN KEY (issue_id)
        REFERENCES issues(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_match_item_offerer FOREIGN KEY (offered_by_user_id)
        REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_match_item_wanter FOREIGN KEY (wanted_by_user_id)
        REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_trade_match_items_pair (match_id, offer_entry_id, wanted_entry_id),
    INDEX idx_trade_match_items_offer (offer_entry_id),
    INDEX idx_trade_match_items_wanted (wanted_entry_id),
    INDEX idx_trade_match_items_direction (match_id, offered_by_user_id, wanted_by_user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE trades (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    match_id INT UNSIGNED NOT NULL,
    initiator_id INT UNSIGNED NOT NULL,
    responder_id INT UNSIGNED NOT NULL,
    status ENUM('proposed', 'accepted', 'cancelled', 'completed') NOT NULL DEFAULT 'proposed',
    open_match_id INT UNSIGNED NULL,
    cancellation_reason VARCHAR(64) NULL,
    proposed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    accepted_at DATETIME NULL,
    cancelled_at DATETIME NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    CONSTRAINT chk_trade_distinct_users CHECK (initiator_id <> responder_id),
    CONSTRAINT fk_trade_match FOREIGN KEY (match_id)
        REFERENCES trade_matches(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_initiator FOREIGN KEY (initiator_id)
        REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_responder FOREIGN KEY (responder_id)
        REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_trades_open_match (open_match_id),
    INDEX idx_trades_initiator_status (initiator_id, status, updated_at),
    INDEX idx_trades_responder_status (responder_id, status, updated_at),
    INDEX idx_trades_match_history (match_id, created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE trade_items (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    trade_id INT UNSIGNED NOT NULL,
    offer_entry_id INT UNSIGNED NULL,
    wanted_entry_id INT UNSIGNED NULL,
    issue_id INT UNSIGNED NOT NULL,
    offered_by_user_id INT UNSIGNED NOT NULL,
    receiving_user_id INT UNSIGNED NOT NULL,
    copy_number_snapshot TINYINT UNSIGNED NOT NULL,
    condition_grade_snapshot ENUM('Z0', 'Z1', 'Z2', 'Z3', 'Z4') NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_trade_item_distinct_users
        CHECK (offered_by_user_id <> receiving_user_id),
    CONSTRAINT fk_trade_item_trade FOREIGN KEY (trade_id)
        REFERENCES trades(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_item_offer FOREIGN KEY (offer_entry_id)
        REFERENCES collection_entries(id) ON DELETE SET NULL,
    CONSTRAINT fk_trade_item_wanted FOREIGN KEY (wanted_entry_id)
        REFERENCES collection_entries(id) ON DELETE SET NULL,
    CONSTRAINT fk_trade_item_issue FOREIGN KEY (issue_id)
        REFERENCES issues(id) ON DELETE RESTRICT,
    UNIQUE INDEX idx_trade_items_offer (trade_id, offer_entry_id),
    INDEX idx_trade_items_wanted (wanted_entry_id),
    INDEX idx_trade_items_direction (trade_id, offered_by_user_id, receiving_user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
