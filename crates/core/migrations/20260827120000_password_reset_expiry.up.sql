-- ABOUTME: Adds the expiry timestamp required by password reset token validation.
-- ABOUTME: Backfills existing tokens before enforcing the non-null invariant.
ALTER TABLE user_password_reset_tokens
    ADD COLUMN expires_at TIMESTAMP WITH TIME ZONE;

UPDATE user_password_reset_tokens
SET expires_at = created_at + INTERVAL '1 day'
WHERE expires_at IS NULL;

ALTER TABLE user_password_reset_tokens
    ALTER COLUMN expires_at SET NOT NULL;
