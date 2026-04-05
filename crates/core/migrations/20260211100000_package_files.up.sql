CREATE TABLE package_files (
    id BIGSERIAL PRIMARY KEY,
    repository_id UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    project_id UUID NULL REFERENCES projects(id) ON DELETE SET NULL,
    project_version_id UUID NULL REFERENCES project_versions(id) ON DELETE CASCADE,
    package TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    path_ci TEXT GENERATED ALWAYS AS (LOWER(path)) STORED,
    size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
    content_digest TEXT NULL,
    upstream_digest TEXT NULL,
    modified_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ NULL
);

CREATE UNIQUE INDEX package_files_repository_path_unique
    ON package_files (repository_id, path_ci);

CREATE INDEX package_files_repository_listing_idx
    ON package_files (repository_id, modified_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX package_files_repository_path_idx
    ON package_files (repository_id, path_ci)
    WHERE deleted_at IS NULL;

CREATE INDEX package_files_repository_package_name_idx
    ON package_files (repository_id, LOWER(package), LOWER(name))
    WHERE deleted_at IS NULL;

CREATE INDEX package_files_repository_digest_idx
    ON package_files (repository_id, content_digest)
    WHERE deleted_at IS NULL;

INSERT INTO package_files (
    repository_id,
    project_id,
    project_version_id,
    package,
    name,
    path,
    size_bytes,
    content_digest,
    upstream_digest,
    modified_at,
    created_at,
    updated_at,
    deleted_at
)
SELECT
    pv.repository_id,
    pv.project_id,
    pv.id,
    p.key,
    pv.version,
    pv.path,
    COALESCE(
        CASE
            WHEN (pv.extra->'extra'->>'size') ~ '^[0-9]+$' THEN (pv.extra->'extra'->>'size')::BIGINT
            ELSE NULL
        END,
        CASE
            WHEN (pv.extra->'extra'->>'crate_size') ~ '^[0-9]+$' THEN (pv.extra->'extra'->>'crate_size')::BIGINT
            ELSE NULL
        END,
        0
    ) AS size_bytes,
    NULLIF(
        COALESCE(
            pv.extra->'extra'->>'sha256',
            pv.extra->'extra'->>'checksum'
        ),
        ''
    ) AS content_digest,
    NULLIF(pv.extra->'extra'->>'upstream_digest', '') AS upstream_digest,
    pv.updated_at,
    NOW(),
    NOW(),
    NULL
FROM project_versions pv
INNER JOIN projects p ON p.id = pv.project_id
ON CONFLICT (repository_id, path_ci)
DO UPDATE SET
    project_id = EXCLUDED.project_id,
    project_version_id = EXCLUDED.project_version_id,
    package = EXCLUDED.package,
    name = EXCLUDED.name,
    size_bytes = EXCLUDED.size_bytes,
    content_digest = EXCLUDED.content_digest,
    upstream_digest = EXCLUDED.upstream_digest,
    modified_at = EXCLUDED.modified_at,
    updated_at = NOW(),
    deleted_at = NULL;
