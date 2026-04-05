ALTER TABLE repositories
    DROP COLUMN IF EXISTS storage_usage_updated_at,
    DROP COLUMN IF EXISTS storage_usage_bytes;

