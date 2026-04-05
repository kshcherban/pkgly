-- Remove legacy cache_enabled flag from docker proxy configs
UPDATE repository_configs
SET value = (value::jsonb - 'cache_enabled')::json
WHERE key = 'docker/proxy';
