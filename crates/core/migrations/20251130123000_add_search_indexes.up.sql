CREATE INDEX IF NOT EXISTS idx_project_versions_repo_updated
    ON project_versions (repository_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_projects_repo_key_lower
    ON projects (repository_id, LOWER(key));

CREATE INDEX IF NOT EXISTS idx_project_versions_path_lower
    ON project_versions (repository_id, LOWER(path));

CREATE INDEX IF NOT EXISTS idx_project_versions_extra_gin
    ON project_versions USING gin (extra);
