ALTER TABLE users
    ADD COLUMN email_verified BOOLEAN NOT NULL DEFAULT FALSE AFTER oauth_id,
    ADD COLUMN verification_token VARCHAR(255) NULL AFTER email_verified,
    ADD COLUMN verification_token_expires_at TIMESTAMP NULL AFTER verification_token,
    ADD COLUMN privacy_consent_at TIMESTAMP NULL AFTER verification_token_expires_at;

-- Ensure existing demo user is verified
UPDATE users SET email_verified = TRUE WHERE email = 'demo@lilly.app';
