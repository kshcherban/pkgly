# RubyGems Repository HTTP Routes

All RubyGems endpoints are served **per repository** under:

`/repositories/<storage>/<repo>/...`

- `<host>` – Pkgly base URL (e.g. `pkgly.example.com`)
- `<storage>` – Storage identifier that backs the repository
- `<repo>` – Repository name
- `<gem>` – Gem name (case-insensitive for `/info/<gem>`)
- `<file>` – Gem filename (for example `rack-3.0.0.gem`)
- `<token>` – Pkgly auth token with appropriate permissions

## Compact Index

### Names
```bash
curl "https://<host>/repositories/<storage>/<repo>/names"
```

### Versions
```bash
curl "https://<host>/repositories/<storage>/<repo>/versions"
```

### Gem Info
```bash
curl "https://<host>/repositories/<storage>/<repo>/info/<gem>"
```

## Gem Downloads

```bash
curl -O "https://<host>/repositories/<storage>/<repo>/gems/<file>"
```

## Legacy RubyGems Index (Bundler compatibility)

Bundler and RubyGems may use these “full index” endpoints against custom sources:

```bash
curl -O "https://<host>/repositories/<storage>/<repo>/specs.4.8.gz"
curl -O "https://<host>/repositories/<storage>/<repo>/latest_specs.4.8.gz"
curl -O "https://<host>/repositories/<storage>/<repo>/prerelease_specs.4.8.gz"
```

Quick gemspecs:

```bash
curl -O "https://<host>/repositories/<storage>/<repo>/quick/Marshal.4.8/<gem>-<version>.gemspec.rz"
curl -O "https://<host>/repositories/<storage>/<repo>/quick/Marshal.4.8/<gem>-<version>-<platform>.gemspec.rz"
```

## Publish / Yank (Hosted only)

### Publish
```bash
curl --data-binary @mygem-1.2.3.gem \
  -H "Authorization: <token>" \
  "https://<host>/repositories/<storage>/<repo>/api/v1/gems"
```

### Yank
```bash
curl -X DELETE \
  -H "Authorization: <token>" \
  -d "gem_name=mygem" -d "version=1.2.3" \
  -d "platform=x86_64-linux" \
  "https://<host>/repositories/<storage>/<repo>/api/v1/gems/yank"
```

Notes:
- Proxy repositories are read-only and return `405 Method Not Allowed` for publish/yank requests.
- Ruby proxies support read-only endpoints (Compact Index, legacy RubyGems index, and gem downloads).
