-- ABOUTME: Reverses known S3 raw region identifiers for legacy deployments.
-- ABOUTME: Leaves newer identifiers unchanged because enum-only binaries cannot represent them.
UPDATE storages
SET config = jsonb_set(
    config,
    '{settings,region}',
    to_jsonb(CASE config #>> '{settings,region}'
        WHEN 'us-east-1' THEN 'UsEast1'
        WHEN 'us-east-2' THEN 'UsEast2'
        WHEN 'us-west-1' THEN 'UsWest1'
        WHEN 'us-west-2' THEN 'UsWest2'
        WHEN 'ca-central-1' THEN 'CaCentral1'
        WHEN 'af-south-1' THEN 'AfSouth1'
        WHEN 'ap-east-1' THEN 'ApEast1'
        WHEN 'ap-south-1' THEN 'ApSouth1'
        WHEN 'ap-northeast-1' THEN 'ApNortheast1'
        WHEN 'ap-northeast-2' THEN 'ApNortheast2'
        WHEN 'ap-northeast-3' THEN 'ApNortheast3'
        WHEN 'ap-southeast-1' THEN 'ApSoutheast1'
        WHEN 'ap-southeast-2' THEN 'ApSoutheast2'
        WHEN 'cn-north-1' THEN 'CnNorth1'
        WHEN 'cn-northwest-1' THEN 'CnNorthwest1'
        WHEN 'eu-north-1' THEN 'EuNorth1'
        WHEN 'eu-central-1' THEN 'EuCentral1'
        WHEN 'eu-central-2' THEN 'EuCentral2'
        WHEN 'eu-west-1' THEN 'EuWest1'
        WHEN 'eu-west-2' THEN 'EuWest2'
        WHEN 'eu-west-3' THEN 'EuWest3'
        WHEN 'il-central-1' THEN 'IlCentral1'
        WHEN 'me-south-1' THEN 'MeSouth1'
        WHEN 'sa-east-1' THEN 'SaEast1'
        ELSE config #>> '{settings,region}'
    END),
    true
)
WHERE lower(storage_type) = 's3'
  AND config #>> '{type}' = 'S3'
  AND config #>> '{settings,region}' IN (
      'us-east-1', 'us-east-2', 'us-west-1', 'us-west-2', 'ca-central-1', 'af-south-1',
      'ap-east-1', 'ap-south-1', 'ap-northeast-1', 'ap-northeast-2', 'ap-northeast-3',
      'ap-southeast-1', 'ap-southeast-2', 'cn-north-1', 'cn-northwest-1', 'eu-north-1',
      'eu-central-1', 'eu-central-2', 'eu-west-1', 'eu-west-2', 'eu-west-3', 'il-central-1',
      'me-south-1', 'sa-east-1'
  );
