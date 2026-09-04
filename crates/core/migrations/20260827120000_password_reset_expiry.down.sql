-- ABOUTME: Rolls back password reset token expiry storage.
-- ABOUTME: Restores the schema that predates expiry validation.
ALTER TABLE user_password_reset_tokens
    DROP COLUMN expires_at;
