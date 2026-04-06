# NuGet Configs

NuGet repositories are configured under the `nuget` config type.

## Hosted

```json
{
  "type": "Hosted"
}
```

## Proxy

```json
{
  "type": "Proxy",
  "config": {
    "upstream_url": "https://api.nuget.org/v3/index.json"
  }
}
```

`upstream_url` should point at a NuGet V3 service index. Pkgly discovers the required resources from that index automatically.

## Virtual

```json
{
  "type": "Virtual",
  "config": {
    "member_repositories": [
      {
        "repository_id": "11111111-1111-1111-1111-111111111111",
        "repository_name": "nuget-hosted",
        "priority": 0,
        "enabled": true
      },
      {
        "repository_id": "22222222-2222-2222-2222-222222222222",
        "repository_name": "nuget-proxy",
        "priority": 10,
        "enabled": true
      }
    ],
    "resolution_order": "Priority",
    "cache_ttl_seconds": 60,
    "publish_to": "11111111-1111-1111-1111-111111111111"
  }
}
```

Notes:
- `member_repositories` must not be empty.
- `publish_to` is optional. When omitted, Pkgly auto-selects the highest-priority enabled hosted member.
- `cache_ttl_seconds` controls virtual member-resolution caching and must be greater than zero.
