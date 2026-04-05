ALTER TABLE project_versions
    ADD COLUMN IF NOT EXISTS repository_id UUID;

UPDATE project_versions pv
SET repository_id = p.repository_id
FROM projects p
WHERE pv.project_id = p.id
  AND (pv.repository_id IS NULL OR pv.repository_id <> p.repository_id);

ALTER TABLE project_versions
    ALTER COLUMN repository_id SET NOT NULL,
    ADD CONSTRAINT project_versions_repository_fk
        FOREIGN KEY (repository_id) REFERENCES repositories(id)
        ON DELETE CASCADE;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_indexes
        WHERE schemaname = 'public'
          AND indexname = 'idx_project_versions_repository_path_lower'
    ) THEN
        EXECUTE 'CREATE UNIQUE INDEX idx_project_versions_repository_path_lower
                 ON project_versions (repository_id, LOWER(path))';
    END IF;
END
$$ LANGUAGE plpgsql;
