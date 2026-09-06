-- ABOUTME: Records stored Docker object sizes and direct manifest references.
-- ABOUTME: Calculates distinct reachable cached bytes without object-store lookups.
CREATE TABLE docker_objects (
    repository_id UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
    references_paths TEXT[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (repository_id, path)
);

CREATE FUNCTION docker_referenced_size(repo UUID, root_path TEXT)
RETURNS BIGINT LANGUAGE SQL STABLE AS $$
    WITH RECURSIVE reachable(path) AS (
        SELECT path FROM docker_objects WHERE repository_id = repo AND path = root_path
        UNION
        SELECT unnest(objects.references_paths)
        FROM docker_objects objects JOIN reachable ON objects.path = reachable.path
        WHERE objects.repository_id = repo
    )
    SELECT SUM(objects.size_bytes)::BIGINT
    FROM reachable JOIN docker_objects objects USING (path)
    WHERE objects.repository_id = repo
$$;

-- Scalar snapshots cannot establish which referenced objects are still stored.
UPDATE package_files SET referenced_size_bytes = NULL;
