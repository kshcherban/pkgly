-- Test data seed script - matches production database format exactly
-- This is executed after migrations have run to populate test data

-- Create test user (admin with all permissions)
-- Using admin credentials: admin/TestAdmin
INSERT INTO users (id, name, username, email, active, password, admin, user_manager, system_manager, default_repository_actions)
VALUES (
    1,
    'Test Admin',
    'admin',
    'admin@pkgly.test',
    true,
    '$argon2id$v=19$m=19456,t=2,p=1$exMiDpgceUGal46GcHYcDQ$nny+rXBoWIVovDV1ddTxNjNTUuaQh0iQmh9g6CfYzcw',
    true,
    true,
    true,
    ARRAY[]::text[]
)
ON CONFLICT (id) DO NOTHING;

-- Create test storage (local filesystem) - must match production format exactly
INSERT INTO storages (id, name, storage_type, active, config)
VALUES (
    '00000000-0000-0000-0000-000000000001'::uuid,
    'test-storage',
    'Local',
    true,
    '{"type": "Local", "settings": {"path": "/storage/test-storage"}}'::jsonb
)
ON CONFLICT (id) DO NOTHING;

-- Maven Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '11111111-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'maven-hosted',
    'maven',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('11111111-0000-0000-0000-000000000001'::uuid, 'maven', '{"type": "Hosted"}'::jsonb),
    ('11111111-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Maven Proxy Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '11111111-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'maven-proxy',
    'maven',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('11111111-0000-0000-0000-000000000002'::uuid, 'maven', '{"type": "Proxy", "config": {"routes": [{"url": "https://repo1.maven.org/maven2/"}]}}'::jsonb),
    ('11111111-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- NPM Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '22222222-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'npm-hosted',
    'npm',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('22222222-0000-0000-0000-000000000001'::uuid, 'npm', '{"type": "Hosted"}'::jsonb),
    ('22222222-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": true}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- NPM Proxy Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '22222222-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'npm-proxy',
    'npm',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('22222222-0000-0000-0000-000000000002'::uuid, 'npm', '{"type": "Proxy", "config": {"routes": [{"url": "https://registry.npmjs.org", "name": "npmjs"}]}}'::jsonb),
    ('22222222-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Docker Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '33333333-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'docker-hosted',
    'docker',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('33333333-0000-0000-0000-000000000002'::uuid, 'docker', '{"type": "Hosted"}'::jsonb),
    ('33333333-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Docker Proxy Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '33333333-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'docker-proxy',
    'docker',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    (
        '33333333-0000-0000-0000-000000000001'::uuid,
        'docker',
        '{
            "type": "Proxy",
            "config": {
                "upstream_url": "https://registry-1.docker.io"
            }
        }'::jsonb
    ),
    ('33333333-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Python Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '44444444-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'python-hosted',
    'python',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('44444444-0000-0000-0000-000000000001'::uuid, 'python', '{"type": "Hosted"}'::jsonb),
    ('44444444-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Python Proxy Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '44444444-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'python-proxy',
    'python',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('44444444-0000-0000-0000-000000000002'::uuid, 'python', '{"type": "Proxy", "config": {"routes": [{"url": "https://pypi.org/simple", "name": "PyPI"}]}}'::jsonb),
    ('44444444-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- PHP Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '55555555-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'php-hosted',
    'php',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('55555555-0000-0000-0000-000000000001'::uuid, 'php', '{"type": "Hosted"}'::jsonb),
    ('55555555-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- PHP Proxy Repository (proxies the local php-hosted repo for deterministic tests)
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '55555555-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'php-proxy',
    'php',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('55555555-0000-0000-0000-000000000002'::uuid, 'php', '{"type": "Proxy", "config": {"routes": [{"url": "http://pkgly:8888/repositories/test-storage/php-hosted", "name": "Hosted"}]}}'::jsonb),
    ('55555555-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Ruby Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '12121212-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'ruby-hosted',
    'ruby',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('12121212-0000-0000-0000-000000000001'::uuid, 'ruby', '{"type": "Hosted"}'::jsonb),
    ('12121212-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Ruby Proxy Repository (proxies the local ruby-hosted repo for deterministic tests)
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '12121212-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'ruby-proxy',
    'ruby',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    (
        '12121212-0000-0000-0000-000000000002'::uuid,
        'ruby',
        '{"type": "Proxy", "config": {"upstream_url": "http://pkgly:8888/repositories/test-storage/ruby-hosted"}}'::jsonb
    ),
    ('12121212-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Go Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '66666666-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'go-hosted',
    'go',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('66666666-0000-0000-0000-000000000001'::uuid, 'go', '{"type": "Hosted"}'::jsonb),
    ('66666666-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Go Proxy Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '66666666-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'go-proxy',
    'go',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('66666666-0000-0000-0000-000000000002'::uuid, 'go', '{"type": "Proxy", "config": {"routes": [{"url": "https://proxy.golang.org", "name": "Go Official Proxy", "priority": 0}], "go_module_cache_ttl": 3600}}'::jsonb),
    ('66666666-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Debian Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '88888888-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'deb-hosted',
    'deb',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('88888888-0000-0000-0000-000000000001'::uuid, 'deb', '{"distributions": ["stable"], "components": ["main"], "architectures": ["amd64", "all"]}'::jsonb),
    ('88888888-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Cargo Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '99999999-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'cargo-hosted',
    'cargo',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('99999999-0000-0000-0000-000000000001'::uuid, 'cargo', '{"type": "Hosted"}'::jsonb),
    ('99999999-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Helm Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '77777777-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'helm-hosted',
    'helm',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('77777777-0000-0000-0000-000000000001'::uuid, 'helm', '{"mode": "http", "overwrite": true, "max_chart_size": 10485760, "max_file_count": 1028, "index_cache_ttl": 300, "public_base_url": null}'::jsonb),
    ('77777777-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Helm OCI Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    '77777777-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'helm-oci',
    'helm',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('77777777-0000-0000-0000-000000000002'::uuid, 'helm', '{"mode": "oci", "overwrite": true, "max_chart_size": 10485760, "max_file_count": 1028, "index_cache_ttl": 300, "public_base_url": null}'::jsonb),
    ('77777777-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- NuGet Hosted Repository
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    'aaaaaaaa-0000-0000-0000-000000000001'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'nuget-hosted',
    'nuget',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    ('aaaaaaaa-0000-0000-0000-000000000001'::uuid, 'nuget', '{"type": "Hosted"}'::jsonb),
    ('aaaaaaaa-0000-0000-0000-000000000001'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- NuGet Proxy Repository (proxies the local hosted repo for deterministic tests)
INSERT INTO repositories (id, storage_id, name, repository_type, visibility, active)
VALUES (
    'aaaaaaaa-0000-0000-0000-000000000002'::uuid,
    '00000000-0000-0000-0000-000000000001'::uuid,
    'nuget-proxy',
    'nuget',
    'Public',
    true
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO repository_configs (repository_id, key, value) VALUES
    (
        'aaaaaaaa-0000-0000-0000-000000000002'::uuid,
        'nuget',
        '{
            "type": "Proxy",
            "config": {
                "upstream_url": "http://pkgly:8888/repositories/test-storage/nuget-hosted/v3/index.json"
            }
        }'::jsonb
    ),
    ('aaaaaaaa-0000-0000-0000-000000000002'::uuid, 'auth', '{"enabled": false}'::jsonb)
ON CONFLICT (repository_id, key) DO NOTHING;

-- Create test auth token (never expires)
-- Token: NPDxeLFM8ehXKteIHW7DFy1chf2QaYdf (encrypted binary data in database)
INSERT INTO user_auth_tokens (id, user_id, name, description, token, active, source, expires_at)
VALUES (
    1,
    1,
    'test124',
    'Integration test authentication token',
    'FrhMryINx75JKQvWDiV59ztOzDjPQRU/Ga+tO0j3inI=',
    true,
    'manual',
    NULL
)
ON CONFLICT (id) DO NOTHING;

-- Grant all scopes to test token
INSERT INTO user_auth_token_scopes (user_auth_token_id, scope)
SELECT 1, scope FROM (
    VALUES
        ('ReadRepository'),
        ('WriteRepository'),
        ('DeleteRepository'),
        ('ManageRepository'),
        ('ReadStorage'),
        ('WriteStorage'),
        ('ManageStorage'),
        ('ManageUsers'),
        ('ManageSystem')
) AS scopes(scope)
ON CONFLICT DO NOTHING;

-- Reset sequences
SELECT setval('users_id_seq', 1, true);
SELECT setval('user_auth_tokens_id_seq', 1, true);
