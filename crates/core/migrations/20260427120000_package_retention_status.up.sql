CREATE TABLE IF NOT EXISTS package_retention_status (
    repository_id UUID PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
    in_progress BOOLEAN NOT NULL DEFAULT FALSE,
    last_started_at TIMESTAMPTZ,
    last_finished_at TIMESTAMPTZ,
    last_success_at TIMESTAMPTZ,
    last_error TEXT,
    last_deleted_count INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS package_retention_status_in_progress_idx
    ON package_retention_status (in_progress);

CREATE INDEX IF NOT EXISTS package_retention_status_last_started_idx
    ON package_retention_status (last_started_at);
