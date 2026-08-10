CREATE TABLE message_threads (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    trade_id INT UNSIGNED NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    CONSTRAINT fk_message_thread_trade FOREIGN KEY (trade_id)
        REFERENCES trades(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_message_threads_trade (trade_id),
    INDEX idx_message_threads_updated (updated_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE messages (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    thread_id INT UNSIGNED NOT NULL,
    sender_id INT UNSIGNED NOT NULL,
    client_message_id CHAR(36) NOT NULL,
    content TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    read_at DATETIME NULL,
    CONSTRAINT fk_message_thread FOREIGN KEY (thread_id)
        REFERENCES message_threads(id) ON DELETE CASCADE,
    CONSTRAINT fk_message_sender FOREIGN KEY (sender_id)
        REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_messages_idempotency (thread_id, sender_id, client_message_id),
    INDEX idx_messages_thread_cursor (thread_id, id),
    INDEX idx_messages_thread_unread (thread_id, read_at, sender_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE notifications (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id INT UNSIGNED NOT NULL,
    actor_user_id INT UNSIGNED NULL,
    kind ENUM(
        'trade_match',
        'trade_match_updated',
        'trade_proposed',
        'trade_accepted',
        'trade_cancelled',
        'trade_message'
    ) NOT NULL,
    match_id INT UNSIGNED NULL,
    trade_id INT UNSIGNED NULL,
    message_id INT UNSIGNED NULL,
    dedupe_key VARCHAR(255) NOT NULL,
    payload JSON NOT NULL,
    read_at DATETIME NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_notification_user FOREIGN KEY (user_id)
        REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT fk_notification_actor FOREIGN KEY (actor_user_id)
        REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT fk_notification_match FOREIGN KEY (match_id)
        REFERENCES trade_matches(id) ON DELETE CASCADE,
    CONSTRAINT fk_notification_trade FOREIGN KEY (trade_id)
        REFERENCES trades(id) ON DELETE CASCADE,
    CONSTRAINT fk_notification_message FOREIGN KEY (message_id)
        REFERENCES messages(id) ON DELETE CASCADE,
    UNIQUE INDEX idx_notifications_dedupe (user_id, dedupe_key),
    INDEX idx_notifications_inbox (user_id, read_at, created_at),
    INDEX idx_notifications_trade (trade_id),
    INDEX idx_notifications_message (message_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
