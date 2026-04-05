# Helm Repository Routes

Pkgly exposes both the classic Helm chart HTTP endpoints and the OCI distribution-spec routes required by Helm 3. Repositories operate in either HTTP or OCI mode—uploads are not mirrored between the two protocols. The examples below use the following placeholders:

- `<host>` – Pkgly base URL (for example `pkgly.example.com`)
- `<storage>` – Storage name that contains the repository
- `<repository>` – Helm repository name
- `<chart>` – Chart name (for example `webapp`)
- `<version>` – Chart version (for example `1.2.3`)
- `<digest>` – OCI digest (for example `sha256:deadbeef...`)
- `<token>` – Repository access token or session bearer token

When interacting with the OCI API you may address a repository as either:

- `/v2/<storage>/<repository>/…`
- `/v2/repositories/<storage>/<repository>/…`

Pkgly normalises both forms and they are interchangeable.

## HTTP Chart Repository

### Index
```bash
curl -L "https://<host>/repositories/<storage>/<repository>/index.yaml"
```

### Chart Download
```bash
# GET
curl -L \
  "https://<host>/repositories/<storage>/<repository>/charts/<chart>/<chart>-<version>.tgz"

# HEAD
curl -I \
  "https://<host>/repositories/<storage>/<repository>/charts/<chart>/<chart>-<version>.tgz"
```

### Chart Upload (PUT)
```bash
curl -u "<token>:" \
  -T "<chart>-<version>.tgz" \
  "https://<host>/repositories/<storage>/<repository>/<chart>-<version>.tgz"
```

Pkgly also understands the ChartMuseum style API:

```bash
# List all charts
curl "https://<host>/repositories/<storage>/<repository>/api/charts"

# Upload via multipart/form-data
curl -u "<token>:" \
  -F "chart=@<chart>-<version>.tgz" \
  "https://<host>/repositories/<storage>/<repository>/api/charts"
```

## OCI Registry

### Login / Bearer Token
```bash
curl -u "<token>:" \
  "https://<host>/v2/token?service=<host>&scope=repository:<storage>/<repository>/<chart>:pull,push"
```

### Blob Upload Workflow
```bash
# Initiate upload
curl -u "<token>:" -X POST \
  "https://<host>/v2/<storage>/<repository>/<chart>/blobs/uploads/"

# Stream chart bytes (PATCH)
curl -u "<token>:" -X PATCH \
  -H "Content-Type: application/octet-stream" \
  --data-binary "@<chart>-<version>.tgz" \
  "https://<host>/v2/<storage>/<repository>/<chart>/blobs/uploads/<uuid>"

# Complete upload with digest
curl -u "<token>:" -X PUT \
  "https://<host>/v2/<storage>/<repository>/<chart>/blobs/uploads/<uuid>?digest=<digest>"
```

### Manifest Upload
```bash
curl -u "<token>:" -X PUT \
  -H "Content-Type: application/vnd.oci.image.manifest.v1+json" \
  --data-binary "@manifest.json" \
  "https://<host>/v2/<storage>/<repository>/<chart>/manifests/<version>"
```

Helm 3 automates the blob + manifest workflow:

```bash
helm registry login <host> --username <token> --password <secret>
helm push <chart>-<version>.tgz oci://<host>/<storage>/<repository>
```

### Manifest / Blob Retrieval
```bash
# Manifest by tag
curl -H "Accept: application/vnd.oci.image.manifest.v1+json" \
  "https://<host>/v2/<storage>/<repository>/<chart>/manifests/<version>"

# Manifest by digest
curl -H "Accept: application/vnd.oci.image.manifest.v1+json" \
  "https://<host>/v2/<storage>/<repository>/<chart>/manifests/<digest>"

# Blob download
curl -L \
  "https://<host>/v2/<storage>/<repository>/<chart>/blobs/<digest>"
```

### Deleting a Chart (OCI)
```bash
# Delete manifest by tag (removes the associated manifest and orphaned blobs)
curl -u "<token>:" -X DELETE \
  "https://<host>/v2/<storage>/<repository>/<chart>/manifests/<version>"
```

## Admin API – Packages

The Admin UI consumes the same API you can call directly:

```bash
# List chart packages
curl -u "<token>:" \
  "https://<host>/api/repository/<repository-id>/packages"

# Delete chart versions by canonical path
curl -u "<token>:" -X DELETE \
  -H "Content-Type: application/json" \
  -d '{"paths":["charts/webapp/webapp-1.2.3.tgz"]}' \
  "https://<host>/api/repository/<repository-id>/packages"
```

These endpoints return JSON payloads consumed by the Admin Packages view and respect repository permissions.
