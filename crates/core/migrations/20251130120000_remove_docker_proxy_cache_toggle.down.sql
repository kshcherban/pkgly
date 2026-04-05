-- Reintroduce cache_enabled flag with default true for docker proxy configs
UPDATE repository_configs
SET value = jsonb_set(value::jsonb, '{cache_enabled}', 'true'::jsonb, true)::json
WHERE key = 'docker/proxy'
  AND NOT (value::jsonb ? 'cache_enabled');
