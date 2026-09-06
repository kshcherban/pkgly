-- ABOUTME: Drops the persisted referenced-size column when rolling back the listing change.
ALTER TABLE package_files
DROP COLUMN referenced_size_bytes;
