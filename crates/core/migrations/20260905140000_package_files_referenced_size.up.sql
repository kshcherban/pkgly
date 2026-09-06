-- ABOUTME: Adds a persisted referenced-size column so Docker package listings avoid storage reads.
-- ABOUTME: Holds the total stored bytes of a manifest and its referenced blobs for the UI.
ALTER TABLE package_files
ADD COLUMN referenced_size_bytes BIGINT;
