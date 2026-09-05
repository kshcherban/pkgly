-- ABOUTME: Migrates legacy S3 enum region tokens to canonical raw identifiers.
-- ABOUTME: Keeps persisted S3 configuration compatible with string-based regions.
UPDATE storages
SET config = jsonb_set(
    config,
    '{settings,region}',
    to_jsonb(CASE config #>> '{settings,region}'
        WHEN 'UsEast1' THEN 'us-east-1'
        WHEN 'UsEast2' THEN 'us-east-2'
        WHEN 'UsWest1' THEN 'us-west-1'
        WHEN 'UsWest2' THEN 'us-west-2'
        WHEN 'CaCentral1' THEN 'ca-central-1'
        WHEN 'AfSouth1' THEN 'af-south-1'
        WHEN 'ApEast1' THEN 'ap-east-1'
        WHEN 'ApSouth1' THEN 'ap-south-1'
        WHEN 'ApNortheast1' THEN 'ap-northeast-1'
        WHEN 'ApNortheast2' THEN 'ap-northeast-2'
        WHEN 'ApNortheast3' THEN 'ap-northeast-3'
        WHEN 'ApSoutheast1' THEN 'ap-southeast-1'
        WHEN 'ApSoutheast2' THEN 'ap-southeast-2'
        WHEN 'CnNorth1' THEN 'cn-north-1'
        WHEN 'CnNorthwest1' THEN 'cn-northwest-1'
        WHEN 'EuNorth1' THEN 'eu-north-1'
        WHEN 'EuCentral1' THEN 'eu-central-1'
        WHEN 'EuCentral2' THEN 'eu-central-2'
        WHEN 'EuWest1' THEN 'eu-west-1'
        WHEN 'EuWest2' THEN 'eu-west-2'
        WHEN 'EuWest3' THEN 'eu-west-3'
        WHEN 'IlCentral1' THEN 'il-central-1'
        WHEN 'MeSouth1' THEN 'me-south-1'
        WHEN 'SaEast1' THEN 'sa-east-1'
        ELSE config #>> '{settings,region}'
    END),
    true
)
WHERE lower(storage_type) = 's3'
  AND config #>> '{type}' = 'S3'
  AND config #>> '{settings,region}' IN (
      'UsEast1', 'UsEast2', 'UsWest1', 'UsWest2', 'CaCentral1', 'AfSouth1',
      'ApEast1', 'ApSouth1', 'ApNortheast1', 'ApNortheast2', 'ApNortheast3',
      'ApSoutheast1', 'ApSoutheast2', 'CnNorth1', 'CnNorthwest1', 'EuNorth1',
      'EuCentral1', 'EuCentral2', 'EuWest1', 'EuWest2', 'EuWest3', 'IlCentral1',
      'MeSouth1', 'SaEast1'
  );
