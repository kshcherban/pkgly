ALTER TABLE project_versions
    DROP CONSTRAINT IF EXISTS unique_repository_path_lower,
    DROP CONSTRAINT IF EXISTS project_versions_repository_fk;

DROP INDEX IF EXISTS idx_project_versions_repository_path_lower;

ALTER TABLE project_versions
    DROP COLUMN IF EXISTS repository_id;
