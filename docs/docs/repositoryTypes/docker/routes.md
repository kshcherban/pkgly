# Docker Registry HTTP Routes

Pkgly implements the complete Docker Registry V2 API specification for hosted Docker repositories. This reference details every HTTP route, its purpose, and example usage. Replace placeholders with your own hostname, repository name, image name, tag, and authentication token.

- `<host>` – Pkgly base URL (e.g. `pkgly.example.com`)
- `<repository>` – Repository name (e.g. `my-docker-repo`)
- `<image>` – Docker image name (e.g. `myapp`, `myproject/myapp`)
- `<reference>` – Tag name or digest (e.g. `latest`, `v1.0.0`, `sha256:abc123...`)
- `<digest>` – Content digest (e.g. `sha256:abc123...`)
- `<uuid>` – Upload UUID for blob upload operations
- `<token>` – Bearer token for authentication

## Core Registry API

### API Version Check
Verify the registry supports Docker Registry V2 API:
```bash
curl "https://<host>/v2/"
```
**Response Headers:**
```
Docker-Distribution-API-Version: registry/2.0
```

### Repository Catalog
List all repositories (may require admin permissions):
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/v2/_catalog"
```
**Response:**
```json
{
  "repositories": [
    "myapp",
    "myproject/webapp",
    "tools/build-agent"
  ]
}
```

## Image Operations

### List Tags
List all available tags for an image:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/v2/<image>/tags/list"
```
**Response:**
```json
{
  "name": "myapp",
  "tags": [
    "latest",
    "v1.0.0",
    "v1.1.0",
    "develop"
  ]
}
```

### Get Manifest
Retrieve manifest information for a tag or digest:
```bash
# Get manifest by tag
curl -H "Authorization: Bearer <token>" \
     -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
     "https://<host>/v2/<image>/manifests/<reference>"

# Get manifest list (multi-arch)
curl -H "Authorization: Bearer <token>" \
     -H "Accept: application/vnd.docker.distribution.manifest.list.v2+json" \
     "https://<host>/v2/<image>/manifests/<reference>"
```

**Response (Manifest V2):**
```json
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
  "config": {
    "mediaType": "application/vnd.docker.container.image.v1+json",
    "size": 7023,
    "digest": "sha256:b5b2b2c507a0944348a0305746806797b0ec1c6a5c7c8ab5c5c7c8d5d6e7f8a"
  },
  "layers": [
    {
      "mediaType": "application/vnd.docker.image.rootfs.diff.tar.gzip",
      "size": 32654,
      "digest": "sha256:e692418e4cbaf90ca69d05a66403747baa33ee08806650b51fab815ad7fc331f"
    }
  ]
}
```

### Check Manifest Exists
Check if a manifest exists without downloading it:
```bash
curl -I -H "Authorization: Bearer <token>" \
       -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
       "https://<host>/v2/<image>/manifests/<reference>"
```

### Upload Manifest
Upload a new manifest for an image:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/vnd.docker.distribution.manifest.v2+json" \
     --data-binary @manifest.json \
     "https://<host>/v2/<image>/manifests/<reference>"
```

### Delete Manifest
Remove a manifest by tag or digest:
```bash
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     "https://<host>/v2/<image>/manifests/<reference>"
```

## Blob Operations

### Download Blob
Download a layer or config blob by digest:
```bash
curl -L -H "Authorization: Bearer <token>" \
     "https://<host>/v2/<image>/blobs/<digest>"
```

### Check Blob Exists
Check if a blob exists without downloading it:
```bash
curl -I -H "Authorization: Bearer <token>" \
       "https://<host>/v2/<image>/blobs/<digest>"
```

### Initiate Blob Upload
Start a new blob upload session:
```bash
curl -X POST \
     -H "Authorization: Bearer <token>" \
     -H "Content-Length: 0" \
     "https://<host>/v2/<image>/blobs/uploads/"
```
**Response Headers:**
```
Location: /v2/myapp/blobs/uploads/<uuid>
Range: 0-0
```

### Upload Blob Chunk
Upload data to an existing upload session:
```bash
# Upload chunk
curl -X PATCH \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/octet-stream" \
     -H "Content-Range: 0-1023" \
     --data-binary @chunk.bin \
     "https://<host>/v2/<image>/blobs/uploads/<uuid>"

# Upload complete blob (monolithic)
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/octet-stream" \
     -H "Content-Length: 32768" \
     --data-binary @layer.tar.gz \
     "https://<host>/v2/<image>/blobs/uploads/?digest=sha256:<digest>"
```

### Complete Blob Upload
Finalize an upload session with digest:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Length: 0" \
     "https://<host>/v2/<image>/blobs/uploads/<uuid>?digest=sha256:<digest>"
```

### Delete Blob
Remove a blob by digest:
```bash
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     "https://<host>/v2/<image>/blobs/<digest>"
```

## Authentication

### Get Bearer Token
Obtain authentication token for repository access:
```bash
curl -X POST "https://<host>/api/auth/token" \
     -u "username:password" \
     -d "service=pkgly" \
     -d "scope=repository:<image>:pull,push"
```
**Response:**
```json
{
  "token": "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9...",
  "access_token": "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_in": 3600,
  "issued_at": "2023-12-15T10:00:00Z"
}
```

### WWW-Authenticate Challenge
Registry challenges unauthorized requests:
```
WWW-Authenticate: Bearer realm="https://<host>/api/auth/token",service="pkgly",scope="repository:myapp:pull"
```

## Repository Management API

### List Cached Packages
List cached Docker images and layers:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/packages?page=1&per_page=50"
```

### Delete Cached Artifacts
Remove cached images and layers (requires repository edit permission):
```bash
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"paths": ["v2/myapp/blobs/sha256/abc123...", "v2/myapp/manifests/latest"]}' \
     "https://<host>/api/repository/<repository-id>/packages"
```

### Repository Configuration
Get Docker repository configuration:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/config/docker"
```

Update repository configuration:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d @config.json \
     "https://<host>/api/repository/<repository-id>/config/docker"
```

### Repository Statistics
Get repository usage statistics:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/stats"
```
**Response:**
```json
{
  "total_images": 45,
  "total_tags": 128,
  "storage_used": "2.5GB",
  "last_activity": "2023-12-15T15:30:00Z",
  "top_images": [
    {"name": "myapp", "tag_count": 12},
    {"name": "webapp", "tag_count": 8}
  ]
}
```

## Error Responses

### Common Error Codes
- `401 Unauthorized` - Authentication required or invalid credentials
- `403 Forbidden` - Insufficient permissions
- `404 Not Found` - Repository, tag, or blob does not exist
- `409 Conflict` - Tag overwrite not allowed or digest mismatch
- `422 Unprocessable Entity` - Invalid manifest format
- `500 Internal Server Error` - Server-side error

### Error Response Format
```json
{
  "errors": [
    {
      "code": "MANIFEST_UNKNOWN",
      "message": "manifest unknown",
      "detail": "manifest sha256:invalid... not found"
    }
  ]
}
```

## Usage Examples

### Complete Push Workflow
```bash
#!/bin/bash
HOST="https://pkgly.example.com"
IMAGE="myproject/myapp"
TAG="v1.0.0"
USER="username"
PASS="password"

# Get auth token
TOKEN=$(curl -X POST "${HOST}/api/auth/token" \
  -u "${USER}:${PASS}" \
  -d "service=pkgly" \
  -d "scope=repository:${IMAGE}:pull,push" \
  | jq -r '.token')

# Upload layer (example)
LAYER_DIGEST="sha256:abc123..."
curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Length: 0" \
  "${HOST}/v2/${IMAGE}/blobs/uploads/"

# Upload complete image
docker push ${HOST}/${IMAGE}:${TAG}
```

### Pull with Token Authentication
```bash
#!/bin/bash
HOST="https://pkgly.example.com"
IMAGE="myapp"
TAG="latest"
TOKEN="your-bearer-token"

# Get manifest
curl -H "Authorization: Bearer ${TOKEN}" \
     -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
     "${HOST}/v2/${IMAGE}/manifests/${TAG}" | jq .

# Pull image with Docker
docker login ${HOST} # Use username/password or token auth
docker pull ${HOST}/${IMAGE}:${TAG}
```

Use these endpoints as a foundation for Docker client integration, CI/CD pipelines, or custom tooling when working with Pkgly Docker repositories.

---

*Complete reference for Docker registry HTTP routes. See [Docker Quick Reference](reference.md) for usage examples and configuration.*