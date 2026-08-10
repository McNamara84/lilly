CREATE TABLE IF NOT EXISTS role_change_events (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    target_user_id INT UNSIGNED NULL,
    previous_role ENUM('user', 'admin') NOT NULL,
    new_role ENUM('user', 'admin') NOT NULL,
    method ENUM('admin_email_bootstrap', 'cli') NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_role_change_events_target
        FOREIGN KEY (target_user_id) REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT chk_role_change_events_actual_change
        CHECK (previous_role <> new_role),
    INDEX idx_role_change_events_target (target_user_id, created_at),
    INDEX idx_role_change_events_method (method, created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

