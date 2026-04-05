# RubyGems Configs

Ruby repositories are configured under the `ruby` config type.

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
    "upstream_url": "https://rubygems.org",
    "revalidation_ttl_seconds": 300
  }
}
```

Notes:
- `revalidation_ttl_seconds` is reserved for future proxy revalidation behavior. Current proxy
  behavior is cache-on-miss with manual eviction via the packages UI/API.

