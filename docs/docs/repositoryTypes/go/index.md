# Go Repository Quick Reference

## Configuration Templates

### Hosted Repository
```json
{
  "type": "Hosted"
}
```

### Basic Proxy Repository
```json
{
  "type": "Proxy",
  "config": {
    "go_module_cache_ttl": 3600,
    "routes": [
      {
        "url": "https://proxy.golang.org",
        "name": "Go Official Proxy",
        "priority": 10
      }
    ]
  }
}
```

### Multi-Route Proxy Repository
```json
{
  "type": "Proxy",
  "config": {
    "go_module_cache_ttl": 7200,
    "routes": [
      {
        "url": "https://proxy.golang.org",
        "name": "Go Official Proxy",
        "priority": 10
      },
      {
        "url": "https://goproxy.cn",
        "name": "China Proxy",
        "priority": 5
      },
      {
        "url": "https://goproxy.io",
        "name": "Alternative Proxy",
        "priority": 1
      }
    ]
  }
}
```

## Essential Commands

### Set GOPROXY Environment Variable
Use the Pkgly repository URL that includes the storage and repository name:

```bash
export GOPROXY=https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>,https://proxy.golang.org,direct
```

### Add to Shell Profile
```bash
echo 'export GOPROXY=https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>,https://proxy.golang.org,direct' >> ~/.bashrc
source ~/.bashrc
```

### Verify GOPROXY Configuration
```bash
go env GOPROXY
```

### Test Go Repository Access
```bash
go mod init test-project
go get github.com/gin-gonic/gin@v1.9.1
go mod download
```

### Clear Go Module Cache
```bash
go clean -modcache
```

## Publishing to Hosted Repositories

Hosted Go repositories now accept authenticated uploads for `.mod`, `.info`, and `.zip` artifacts using an Athens-compatible multipart `POST` request. Pkgly automatically updates `@v/list`, `@latest`, and `go.mod` aliases after the uploads land.

1. **Authenticate** using an API token (recommended) or username/password.
2. **Upload the module**:
   ```bash
   curl -X POST \
        -H "Authorization: Bearer $PKGLY_TOKEN" \
        -F "module=@widgets-v1.0.0.zip" \
        -F "version=v1.0.0" \
        -F "module_name=github.com/acme/widgets" \
        "https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/upload"
   ```
   The server extracts `go.mod` automatically (you can include an explicit `gomod` field if desired) and accepts an optional `info` part containing `v1.0.0.info`. The same request is also available via the Artipie-style alias:
   ```bash
   curl -X POST \
        -H "Authorization: Bearer $PKGLY_TOKEN" \
        -F "module=@widgets-v1.0.0.zip" \
        -F "version=v1.0.0" \
        -F "module_name=github.com/acme/widgets" \
        "https://your-pkgly.example.com/api/<repository-name>/upload"
   ```

After the upload completes, Pkgly refreshes `@v/list`, `@latest`, and `go.mod` aliases automatically. Clients can immediately download the new version via the Go proxy protocol.

### Cache Maintenance for Proxy Repositories

- Browse to **Admin → Repository → Packages** to inspect items stored under `go-proxy-cache/`.
- Use the search box to locate cached versions (`github.com/acme/widgets/@v/v1.0.0.zip`, etc.).
- Select rows and click **Delete Selected** to invalidate entries; the next `go get` will fetch fresh content from upstream.

## Common Endpoints

| Description | Endpoint | Example |
|-------------|----------|---------|
| Version List | `GET /{module}/@v/list` | `https://your-pkgly.example.com/repositories/<storage>/<repo>/github.com/user/module/@v/list` |
| Version Info | `GET /{module}/@v/{version}.info` | `https://your-pkgly.example.com/repositories/<storage>/<repo>/github.com/user/module/@v/v1.2.3.info` |
| Go Mod File | `GET /{module}/@v/{version}.mod` | `https://your-pkgly.example.com/repositories/<storage>/<repo>/github.com/user/module/@v/v1.2.3.mod` |
| Module Zip | `GET /{module}/@v/{version}.zip` | `https://your-pkgly.example.com/repositories/<storage>/<repo>/github.com/user/module/@v/v1.2.3.zip` |
| Latest Info | `GET /{module}/@latest` | `https://your-pkgly.example.com/repositories/<storage>/<repo>/github.com/user/module/@latest` |

## Troubleshooting Commands

### Check Network Connectivity
```bash
curl -I https://your-pkgly.com/github.com/gin-gonic/gin/@v/list
```

### Test Authentication
```bash
curl -H "Authorization: Bearer $TOKEN" \
     https://your-pkgly.com/api/repository/{id}/stats
```

### Monitor Go Module Downloads
```bash
GOPROXY=https://your-pkgly.com go mod download -x
```

## Configuration Options

| Setting | Default | Recommended Range |
|---------|---------|-------------------|
| Cache TTL | 3600 seconds | 300-86400 seconds |
| Route Priority | 0 | 1-10 (higher = more priority) |
| Max Routes | No limit | 3-5 for reliability |

## Performance Tips

- **High Priority Routes**: Fast, reliable proxies (8-10)
- **Medium Priority Routes**: Regional backups (3-7)
- **Low Priority Routes**: Fallback options (1-2)
- **Cache TTL**: Short for dev (300s), long for prod (7200s+)

## Security Checklist

- [ ] Use HTTPS for all proxy URLs
- [ ] Enable authentication for private repositories
- [ ] Monitor access logs regularly
- [ ] Validate upstream proxy certificates
- [ ] Use API tokens for automation
- [ ] Regular security updates

## Cache Optimization

### Fast-Changing Modules
```json
{
  "go_module_cache_ttl": 300
}
```

### Stable Dependencies
```json
{
  "go_module_cache_ttl": 86400
}
```

### Offline Development
```json
{
  "go_module_cache_ttl": 604800
}
```

---

*Quick reference for Go repository configuration and usage. See [Go Proxy Setup Guide](proxy.md) for detailed documentation.*
