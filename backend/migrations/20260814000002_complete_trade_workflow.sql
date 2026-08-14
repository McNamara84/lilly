ALTER TABLE trades
    ADD COLUMN completed_at DATETIME NULL AFTER cancelled_at;

CREATE TABLE trade_completion_confirmations (
    trade_id INT UNSIGNED NOT NULL,
    user_id INT UNSIGNED NOT NULL,
    confirmed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (trade_id, user_id),
    CONSTRAINT fk_trade_completion_trade FOREIGN KEY (trade_id)
        REFERENCES trades(id) ON DELETE CASCADE,
    CONSTRAINT fk_trade_completion_user FOREIGN KEY (user_id)
        REFERENCES users(id) ON DELETE CASCADE,
    INDEX idx_trade_completion_user_time (user_id, confirmed_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

ALTER TABLE notifications
    MODIFY COLUMN kind ENUM(
        'trade_match',
        'trade_match_updated',
        'trade_proposed',
        'trade_accepted',
        'trade_cancelled',
        'trade_completion_confirmed',
        'trade_completed',
        'trade_message'
    ) NOT NULL;
