ALTER TABLE repositories
    ADD COLUMN IF NOT EXISTS storage_usage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS storage_usage_updated_at TIMESTAMPTZ;

