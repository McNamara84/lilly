-- Add UNIQUE index on verification_token for fast lookups and collision prevention
ALTER TABLE users ADD UNIQUE INDEX idx_users_verification_token (verification_token);

-- Upgrade refresh_tokens.token_hash from non-unique to UNIQUE index
ALTER TABLE refresh_tokens DROP INDEX idx_refresh_tokens_token_hash;
ALTER TABLE refresh_tokens ADD UNIQUE INDEX idx_refresh_tokens_token_hash (token_hash);
