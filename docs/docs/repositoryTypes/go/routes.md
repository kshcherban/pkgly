# Go Repository HTTP Routes

Pkgly implements the standard Go module proxy protocol for both hosted and proxy repositories. This reference summarises every HTTP route, its purpose, and example `curl` usage. Replace the placeholders below with your own hostname, storage name, repository name, module path, and version.

- `<host>` – Pkgly base URL (e.g. `pkgly.example.com`)
- `<storage>` – Storage identifier that backs the repository
- `<repository>` – Repository name
- `<module>` – Module path (`github.com/acme/widget`)
- `<version>` – SemVer tag prefixed with `v` (`v1.2.3`)
- `<token>` – API token or basic-auth credential with read/write permission

## Hosted Repository Endpoints

### Upload Artifacts (Requires Write Permission)

Upload the module metadata (`.info`):
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     --data-binary @<module>-<version>.info \
     "https://<host>/repositories/<storage>/<repository>/<module>/@v/<version>.info"
```

Upload the go.mod file:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     --data-binary @go.mod \
     "https://<host>/repositories/<storage>/<repository>/<module>/@v/<version>.mod"
```

Upload the module archive:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     --data-binary @<module>-<version>.zip \
     "https://<host>/repositories/<storage>/<repository>/<module>/@v/<version>.zip"
```

### Download Artifacts

List available versions:
```bash
curl "https://<host>/repositories/<storage>/<repository>/<module>/@v/list"
```

Fetch specific version metadata:
```bash
curl "https://<host>/repositories/<storage>/<repository>/<module>/@v/<version>.info"
```

Fetch `go.mod` for a version:
```bash
curl "https://<host>/repositories/<storage>/<repository>/<module>/@v/<version>.mod"
```

Download the module archive:
```bash
curl -O \
     "https://<host>/repositories/<storage>/<repository>/<module>/@v/<version>.zip"
```

Get the latest version information:
```bash
curl "https://<host>/repositories/<storage>/<repository>/<module>/@latest"
```

Download the repository-level `go.mod` alias (updated automatically after uploads):
```bash
curl "https://<host>/repositories/<storage>/<repository>/<module>/go.mod"
```

### Athens-Compatible Multipart Upload

Upload zipped module, version, and module path in a single request:
```bash
curl -X POST \
     -H "Authorization: Bearer <token>" \
     -F "module=@<module>-<version>.zip" \
     -F "version=<version>" \
     -F "module_name=<module>" \
     "https://<host>/repositories/<storage>/<repository>/upload"
```

Artipie-style alias (repository auto-resolved by name):
```bash
curl -X POST \
     -H "Authorization: Bearer <token>" \
     -F "module=@<module>-<version>.zip" \
     -F "version=<version>" \
     -F "module_name=<module>" \
     "https://<host>/api/<repository>/upload"
```

## Proxy Repository Endpoints

Proxy repositories expose the same `/@v/` endpoints as hosted repositories (GET only) and transparently cache upstream content. In addition, sum database (sumdb) requests are forwarded to `https://sum.golang.org`:

Check if sumdb is supported:
```bash
curl "https://<host>/repositories/<storage>/<repository>/sumdb/sum.golang.org/supported"
```

Request a checksum entry:
```bash
curl "https://<host>/repositories/<storage>/<repository>/sumdb/sum.golang.org/lookup/<module>@<version>"
```

Request a sumdb tile:
```bash
curl "https://<host>/repositories/<storage>/<repository>/sumdb/sum.golang.org/tile/<height>/<hash>"
```

Cached artifacts are stored under `go-proxy-cache/<module>/@v/`. Administrators can list and delete cached files from the “Packages” tab in the Pkgly UI.

## Repository API (Admin)

List cached packages (supports pagination and search):
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/packages?page=1&per_page=50"
```

Delete cached artifacts (requires repository edit permission):
```bash
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"paths": ["go-proxy-cache/github.com/acme/widget/@v/v1.2.3.zip"]}' \
     "https://<host>/api/repository/<repository-id>/packages"
```

Retrieve repository configuration:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/config/go"
```

Update repository configuration (proxy/hosted settings):
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d @config.json \
     "https://<host>/api/repository/<repository-id>/config/go"
```

Pull repository statistics:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/stats"
```

Use these examples as a template for automation or troubleshooting when working with Pkgly Go repositories.
