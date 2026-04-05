-- Add covering indexes for frequently queried columns

-- Projects lookup by repository + key/path (case insensitive)
CREATE INDEX IF NOT EXISTS idx_projects_repository_key_lower
    ON projects (repository_id, LOWER(key));

CREATE INDEX IF NOT EXISTS idx_projects_repository_path_lower
    ON projects (repository_id, LOWER(path));

-- Project versions lookups by repository + path
CREATE INDEX IF NOT EXISTS idx_project_versions_project_path_lower
    ON project_versions (project_id, LOWER(path));

-- Repository hostnames lookups
DO $$
BEGIN
    IF to_regclass('public.repository_hostnames') IS NOT NULL THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_repository_hostnames_repository ON repository_hostnames (repository_id)';
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_repository_hostnames_storage ON repository_hostnames (storage_id)';
    ELSIF to_regclass('public.hostnames') IS NOT NULL THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_repository_hostnames_repository ON hostnames (repository_id)';
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_repository_hostnames_storage ON hostnames (storage_id)';
    END IF;
END $$;

-- User repository permissions queries by repository
CREATE INDEX IF NOT EXISTS idx_user_repository_permissions_repository
    ON user_repository_permissions (repository_id);

-- User events by user
CREATE INDEX IF NOT EXISTS idx_user_events_user
    ON user_events (user_id);

-- Auth tokens by token / user and their scopes
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_auth_tokens_token
    ON user_auth_tokens (token);

CREATE INDEX IF NOT EXISTS idx_user_auth_tokens_user
    ON user_auth_tokens (user_id);

CREATE INDEX IF NOT EXISTS idx_user_auth_token_scopes_token
    ON user_auth_token_scopes (user_auth_token_id);

CREATE INDEX IF NOT EXISTS idx_user_auth_token_repository_scopes_token_repo
    ON user_auth_token_repository_scopes (user_auth_token_id, repository_id);

-- Stages and stage_files
CREATE INDEX IF NOT EXISTS idx_stages_repository
    ON stages (repository_id);

CREATE INDEX IF NOT EXISTS idx_stage_files_stage
    ON stage_files (stage);

-- Password reset tokens by user
CREATE INDEX IF NOT EXISTS idx_user_password_reset_tokens_user
    ON user_password_reset_tokens (user_id);
