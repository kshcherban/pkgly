# Docker Repository

Docker Repositories implement the Docker Registry HTTP API V2 specification, providing OCI-compliant container image storage and distribution.

## Repository Modes

Docker repositories support two modes:

- **Hosted** – Store and serve images directly from Pkgly (private or public).
- **Proxy (pull-through cache)** – Read-only cache in front of a public upstream registry. Pkgly fetches manifests and blobs on demand, stores them locally (if caching is enabled), and serves subsequent pulls from the cache. Push, delete, and upload operations are rejected with 405 responses.

## Docker Registry API V2

Pkgly implements the Docker Registry HTTP API V2 specification, which is compatible with:

- Docker CLI (`docker push`, `docker pull`)
- Podman
- Containerd
- Any OCI-compliant container runtime

## Supported Image Formats

The Docker repository supports multiple manifest formats:

- **Docker Image Manifest V2, Schema 2** - Standard Docker image format
- **OCI Image Manifest** - Open Container Initiative image format
- **OCI Image Index** - Multi-platform image manifests (manifest lists)

## URL Structure

Unlike other repository types, Docker repositories use a special URL structure to maintain compatibility with Docker clients:

```
https://your-registry.com/{storage}/{repository}/{image-name}:{tag}
```

For example:
```
docker push app.pkgly.dev/docker/docker-test/my-app:latest
```

Where:
- `docker` - Storage name
- `docker-test` - Repository name
- `my-app` - Image name
- `latest` - Image tag

## Quick Start

### 1. Create a Docker Repository

Create a new Docker repository through the Pkgly web interface or API.

### 2. Authenticate

```bash
docker login your-registry.com
Username: your_username
Password: your_password_or_token
```

### 3. Tag Your Image

```bash
docker tag my-app:latest your-registry.com/storage/repository/my-app:latest
```

### 4. Push Image

```bash
docker push your-registry.com/storage/repository/my-app:latest
```

### 5. Pull Image

```bash
docker pull your-registry.com/storage/repository/my-app:latest
```

### Proxy Quick Start

1. Create a Docker repository and choose **Proxy** as the type. Set the upstream URL (for example, `https://registry-1.docker.io`). Caching is enabled by default.
2. Pull images using the same Docker Registry v2 path scheme as hosted repositories:
   ```bash
   docker pull your-registry.com/storage/repository/library/nginx:latest
   ```
3. On first pull, Pkgly retrieves the manifest and layers from the upstream registry and stores them under `v2/<storage>/<repository>/...`. Subsequent pulls are served from the local cache.

Limitations:
- Proxy repositories are read-only (push, delete, and upload requests return 405).
- Upstream authentication is not yet supported; only public images can be proxied.
- Cached content has no TTL; remove cached manifests/blobs through repository package management if you need to reclaim space.

## Browsing and Management

- The repository browser flattens the internal `v2/.../manifests` layout so you can navigate storages, repositories, and image names without seeing implementation folders. Selecting an image shows all uploaded tags as individual entries.
- The **Admin → Packages** tab lists Docker image manifests with the same paginated view used for other repository types. Administrators can select one or more tags and delete their manifests directly from the UI.
- Pkgly's global search now indexes Docker repositories. Queries match both the repository path (for example `library/nginx`) and individual tags (`latest`, build numbers, digests), returning the underlying manifest metadata.

Deleting a manifest removes the tag immediately. Blobs referenced by other manifests are preserved; garbage collection for unused blobs is handled separately.

## Authentication

Docker repositories support multiple authentication methods:

- **Username + Password** - Standard user authentication
- **Username + Token** - Use an auth token as the password (any username works)
- **Session-based** - For web UI access

See the [Authentication](#authentication-1) section in the standard documentation for more details.
