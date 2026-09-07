-- ABOUTME: Removes database-backed Docker object accounting.
-- ABOUTME: Leaves catalog rows available for storage-based size reconstruction.
DROP FUNCTION docker_referenced_size(UUID, TEXT);
DROP TABLE docker_objects;
