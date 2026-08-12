CREATE TABLE privacy_consents (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id INT UNSIGNED NOT NULL,
    policy_version VARCHAR(64) NOT NULL,
    consented_at DATETIME(6) NOT NULL,
    registration_method VARCHAR(16) NOT NULL,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT uq_privacy_consents_user_version UNIQUE (user_id, policy_version),
    CONSTRAINT chk_privacy_consents_method CHECK (
        registration_method IN ('password', 'google', 'github', 'legacy')
    ),
    CONSTRAINT fk_privacy_consents_user FOREIGN KEY (user_id)
        REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

INSERT INTO privacy_consents (user_id, policy_version, consented_at, registration_method)
SELECT id, 'legacy-v1', privacy_consent_at, 'legacy'
FROM users
WHERE privacy_consent_at IS NOT NULL;

CREATE TABLE oauth_identities (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id INT UNSIGNED NOT NULL,
    provider VARCHAR(16) NOT NULL,
    provider_subject VARCHAR(255) NOT NULL,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    last_login_at DATETIME(6) NULL,
    CONSTRAINT uq_oauth_identities_provider_subject UNIQUE (provider, provider_subject),
    CONSTRAINT uq_oauth_identities_user_provider UNIQUE (user_id, provider),
    CONSTRAINT chk_oauth_identities_provider CHECK (provider IN ('google', 'github')),
    CONSTRAINT fk_oauth_identities_user FOREIGN KEY (user_id)
        REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

INSERT INTO oauth_identities (user_id, provider, provider_subject, created_at)
SELECT id, oauth_provider, oauth_id, created_at
FROM users
WHERE oauth_provider IN ('google', 'github')
  AND oauth_id IS NOT NULL
ON DUPLICATE KEY UPDATE provider_subject = VALUES(provider_subject);

ALTER TABLE users
    DROP COLUMN oauth_provider,
    DROP COLUMN oauth_id;

CREATE TABLE oauth_authorization_flows (
    state_hash CHAR(64) NOT NULL PRIMARY KEY,
    browser_binding_hash CHAR(64) NOT NULL,
    provider VARCHAR(16) NOT NULL,
    intent VARCHAR(16) NOT NULL,
    pkce_verifier VARCHAR(128) NOT NULL,
    privacy_policy_version VARCHAR(64) NULL,
    consented_at DATETIME(6) NULL,
    created_at DATETIME(6) NOT NULL,
    expires_at DATETIME(6) NOT NULL,
    consumed_at DATETIME(6) NULL,
    INDEX idx_oauth_flows_expiry (expires_at),
    CONSTRAINT chk_oauth_flows_provider CHECK (provider IN ('google', 'github')),
    CONSTRAINT chk_oauth_flows_intent CHECK (intent IN ('login', 'register')),
    CONSTRAINT chk_oauth_flows_consent CHECK (
        (intent = 'login' AND privacy_policy_version IS NULL AND consented_at IS NULL)
        OR
        (intent = 'register' AND privacy_policy_version IS NOT NULL AND consented_at IS NOT NULL)
    )
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE pending_oauth_links (
    token_hash CHAR(64) NOT NULL PRIMARY KEY,
    provider VARCHAR(16) NOT NULL,
    provider_subject VARCHAR(255) NOT NULL,
    verified_email VARCHAR(255) NOT NULL,
    display_name VARCHAR(100) NOT NULL,
    created_at DATETIME(6) NOT NULL,
    expires_at DATETIME(6) NOT NULL,
    INDEX idx_pending_oauth_links_expiry (expires_at),
    CONSTRAINT chk_pending_oauth_links_provider CHECK (provider IN ('google', 'github'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
