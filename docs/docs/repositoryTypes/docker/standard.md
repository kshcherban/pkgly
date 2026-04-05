# Docker Registry HTTP API V2 - Technical Documentation

This document describes the Docker Registry HTTP API V2 implementation in Pkgly. Use this to understand how Docker clients interact with the registry or to implement custom clients.

## API Endpoints

All Docker Registry endpoints are prefixed with `/v2/`.

### Version Check

**Endpoint:** `GET /v2/`

Returns the Docker Registry API version. This endpoint typically does not require authentication.

**Response:**
```
HTTP/1.1 200 OK
Docker-Distribution-API-Version: registry/2.0
Content-Type: application/json

{}
```

### Upload Initialization

**Endpoint:** `POST /v2/{storage}/{repository}/{name}/blobs/uploads/`

Initiates a blob (layer or config) upload session.

**Request:**
```
POST /v2/docker/docker-test/my-app/blobs/uploads/
Authorization: Basic base64(username:password)
```

**Response:**
```
HTTP/1.1 202 Accepted
Location: /v2/docker/docker-test/my-app/blobs/uploads/{uuid}
Range: 0-0
Docker-Upload-UUID: {uuid}
```

### Blob Upload (Chunked)

**Endpoint:** `PATCH /v2/{storage}/{repository}/{name}/blobs/uploads/{uuid}`

Uploads a chunk of blob data.

**Request:**
```
PATCH /v2/docker/docker-test/my-app/blobs/uploads/{uuid}
Content-Type: application/octet-stream
Content-Range: 0-1023
Content-Length: 1024

[binary data]
```

**Response:**
```
HTTP/1.1 202 Accepted
Location: /v2/docker/docker-test/my-app/blobs/uploads/{uuid}
Range: 0-1023
```

### Blob Upload Completion

**Endpoint:** `PUT /v2/{storage}/{repository}/{name}/blobs/uploads/{uuid}?digest={digest}`

Completes a blob upload and verifies the digest.

**Request:**
```
PUT /v2/docker/docker-test/my-app/blobs/uploads/{uuid}?digest=sha256:abc123...
Content-Length: 0
```

**Response:**
```
HTTP/1.1 201 Created
Location: /v2/docker/docker-test/my-app/blobs/sha256:abc123...
Content-Length: 0
Docker-Content-Digest: sha256:abc123...
```

### Blob Download

**Endpoint:** `GET /v2/{storage}/{repository}/{name}/blobs/{digest}`

Downloads a blob by its digest.

**Request:**
```
GET /v2/docker/docker-test/my-app/blobs/sha256:abc123...
```

**Response:**
```
HTTP/1.1 200 OK
Content-Type: application/octet-stream
Content-Length: 12345
Docker-Content-Digest: sha256:abc123...

[binary data]
```

### Blob Existence Check

**Endpoint:** `HEAD /v2/{storage}/{repository}/{name}/blobs/{digest}`

Checks if a blob exists without downloading it.

**Response:**
```
HTTP/1.1 200 OK
Content-Length: 12345
Docker-Content-Digest: sha256:abc123...
```

### Manifest Upload

**Endpoint:** `PUT /v2/{storage}/{repository}/{name}/manifests/{reference}`

Uploads an image manifest. The reference can be a tag or digest.

**Request:**
```
PUT /v2/docker/docker-test/my-app/manifests/latest
Content-Type: application/vnd.docker.distribution.manifest.v2+json

{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
  "config": {
    "mediaType": "application/vnd.docker.container.image.v1+json",
    "size": 1234,
    "digest": "sha256:config123..."
  },
  "layers": [
    {
      "mediaType": "application/vnd.docker.image.rootfs.diff.tar.gzip",
      "size": 5678,
      "digest": "sha256:layer123..."
    }
  ]
}
```

**Response:**
```
HTTP/1.1 201 Created
Location: /v2/docker/docker-test/my-app/manifests/latest
Docker-Content-Digest: sha256:manifest123...
```

### Manifest Download

**Endpoint:** `GET /v2/{storage}/{repository}/{name}/manifests/{reference}`

Downloads an image manifest by tag or digest.

**Request:**
```
GET /v2/docker/docker-test/my-app/manifests/latest
Accept: application/vnd.docker.distribution.manifest.v2+json
```

**Response:**
```
HTTP/1.1 200 OK
Content-Type: application/vnd.docker.distribution.manifest.v2+json
Docker-Content-Digest: sha256:manifest123...

{manifest JSON}
```

### Tags List

**Endpoint:** `GET /v2/{storage}/{repository}/{name}/tags/list`

Lists all tags for an image.

**Response:**
```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "name": "my-app",
  "tags": ["latest", "v1.0.0", "v1.0.1"]
}
```

## Authentication

Docker clients use HTTP Basic Authentication. Pkgly supports two authentication methods:

### Username + Password

Standard user authentication:

```bash
docker login your-registry.com
Username: alice
Password: her_password
```

This sends:
```
Authorization: Basic base64(alice:her_password)
```

### Username + Token (Recommended)

Use an auth token as the password. The username can be anything:

```bash
docker login your-registry.com
Username: token
Password: nr_abc123...your_auth_token
```

This sends:
```
Authorization: Basic base64(token:nr_abc123...your_auth_token)
```

When Pkgly receives Basic auth:
1. First attempts to verify as username/password
2. If that fails, treats the password field as an auth token
3. This allows token-based authentication with any username

### Authentication Challenge

When authentication is required, Pkgly issues a Docker-compliant bearer challenge:

```
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="https://your-registry.com/v2/token",service="your-registry.com",scope="repository/{storage}/{repository}:pull,push"
Docker-Distribution-API-Version: registry/2.0
```

The Docker client then requests a bearer token from `/v2/token`, supplying the scope that was provided in the challenge. Pkgly validates the user's credentials (username/password, session, or automation token), issues a short-lived bearer token scoped to the requested repository actions, and the client retries the original request with:

```
Authorization: Bearer <token>
```

This flow matches the Docker Registry specification, allowing standard Docker CLIs and OCI clients to authenticate without additional configuration.

## Authorization Behavior

Pkgly's Docker repositories follow this authorization model:

### When Auth is Disabled

- **Pull (GET/HEAD):** Allowed without authentication
- **Push (PUT/POST/DELETE):** Requires authentication

### When Auth is Enabled

- **All Operations:** Require authentication

This matches standard package repository behavior where public registries allow anonymous pulls but require auth for pushes.

## Content Addressing

Docker uses content-addressable storage with SHA256 digests:

```
sha256:1234567890abcdef...
```

All blobs (layers, configs) and manifests are stored by their digest, ensuring:
- **Immutability:** Content cannot be changed once stored
- **Deduplication:** Identical content is stored only once
- **Integrity:** Content is verified against its digest

## Media Types

Pkgly supports these media types:

### Docker Manifest V2
- `application/vnd.docker.distribution.manifest.v2+json`
- `application/vnd.docker.distribution.manifest.list.v2+json`

### OCI Image Format
- `application/vnd.oci.image.manifest.v1+json`
- `application/vnd.oci.image.index.v1+json`

### Layer Media Types
- `application/vnd.docker.image.rootfs.diff.tar.gzip`
- `application/vnd.oci.image.layer.v1.tar+gzip`

## Error Responses

Errors follow the Docker Registry API V2 error format:

```json
{
  "errors": [
    {
      "code": "MANIFEST_INVALID",
      "message": "manifest invalid",
      "detail": "Invalid manifest JSON"
    }
  ]
}
```

Common error codes:
- `BLOB_UNKNOWN` - Blob not found
- `BLOB_UPLOAD_INVALID` - Invalid upload session
- `MANIFEST_INVALID` - Invalid manifest format
- `MANIFEST_UNKNOWN` - Manifest not found
- `NAME_INVALID` - Invalid repository name
- `TAG_INVALID` - Invalid tag name
- `UNAUTHORIZED` - Authentication required
- `DENIED` - Permission denied

## Path Rewriting

Pkgly uses a special path rewriting mechanism to maintain compatibility with Docker clients while preserving its internal URL structure.

Docker clients expect: `/v2/{name}/...`
Pkgly internally uses: `/repositories/{storage}/{repository}/...`

The router automatically rewrites:
```
/v2/{storage}/{repository}/{*path}
→ /repositories/{storage}/{repository}/v2/{*path}
```

This allows Docker clients to use natural URLs while Pkgly maintains its organized structure.
