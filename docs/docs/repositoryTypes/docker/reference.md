# Docker Registry Quick Reference

## Configuration Templates

### Hosted Repository
```json
{
  "type": "Hosted"
}
```

### Hosted Repository with Push Rules
```json
{
  "type": "Hosted",
  "push_rules": {
    "allow_tag_overwrite": false,
    "must_be_project_member": false,
    "must_use_auth_token_for_push": false,
    "require_content_trust": false
  }
}
```

## Essential Commands

### Login to Registry
```bash
docker login your-pkgly.example.com
# Enter username and password when prompted
```

### Tag and Push Image
```bash
# Tag local image for the registry
docker tag myapp:latest your-pkgly.example.com/my-project/myapp:latest

# Push the image
docker push your-pkgly.example.com/my-project/myapp:latest
```

### Pull Image
```bash
docker pull your-pkgly.example.com/my-project/myapp:latest
```

### List Tags for Repository
```bash
# Requires authentication token
curl -H "Authorization: Bearer $TOKEN" \
     "https://your-pkgly.example.com/v2/my-project/myapp/tags/list"
```

### Delete Image
```bash
# Get manifest digest first
DIGEST=$(curl -I -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
  "https://your-pkgly.example.com/v2/my-project/myapp/manifests/latest" \
  | grep -i docker-content-digest | cut -d' ' -f2 | tr -d '\r')

# Delete by digest
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
     "https://your-pkgly.example.com/v2/my-project/myapp/manifests/$DIGEST"
```

## Publishing Workflows

### Standard Docker Push
```bash
# Build image
docker build -t myapp:1.0.0 .

# Login
docker login your-pkgly.example.com

# Tag and push
docker tag myapp:1.0.0 your-pkgly.example.com/my-project/myapp:1.0.0
docker push your-pkgly.example.com/my-project/myapp:1.0.0
```

### Multi-Platform Build with Buildx
```bash
# Create buildx builder
docker buildx create --use

# Build and push for multiple platforms
docker buildx build --platform linux/amd64,linux/arm64 \
  -t your-pkgly.example.com/my-project/myapp:latest \
  --push .
```

### CI/CD Integration (GitHub Actions)
```yaml
name: Build and Push Docker Image

on:
  push:
    tags: ['v*']

jobs:
  build-and-push:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v2

      - name: Login to Pkgly
        uses: docker/login-action@v2
        with:
          registry: your-pkgly.example.com
          username: ${{ secrets.PKGLY_USERNAME }}
          password: ${{ secrets.PKGLY_PASSWORD }}

      - name: Build and push
        uses: docker/build-push-action@v4
        with:
          context: .
          push: true
          tags: your-pkgly.example.com/my-project/myapp:${{ github.ref_name }}
```

## Common Endpoints

| Description | Endpoint | Example |
|-------------|----------|---------|
| API Version Check | `GET /v2/` | `https://your-pkgly.example.com/v2/` |
| List Tags | `GET /v2/{name}/tags/list` | `https://your-pkgly.example.com/v2/myapp/tags/list` |
| Get Manifest | `GET /v2/{name}/manifests/{ref}` | `https://your-pkgly.example.com/v2/myapp/manifests/latest` |
| Upload Manifest | `PUT /v2/{name}/manifests/{ref}` | `https://your-pkgly.example.com/v2/myapp/manifests/v1.0.0` |
| Get Blob | `GET /v2/{name}/blobs/{digest}` | `https://your-pkgly.example.com/v2/myapp/blobs/sha256:abc...` |
| Upload Blob | `POST /v2/{name}/blobs/uploads/` | `https://your-pkgly.example.com/v2/myapp/blobs/uploads/` |

## Troubleshooting Commands

### Check Registry Connectivity
```bash
curl -I https://your-pkgly.example.com/v2/
# Should return 200 OK with "Docker-Distribution-API-Version: registry/2.0"
```

### Test Authentication
```bash
# Get auth token
TOKEN=$(curl -X POST "https://your-pkgly.example.com/api/auth/token" \
  -u "username:password" \
  -d "service=pkgly" \
  -d "scope=repository:myapp:pull,push" \
  | jq -r '.token')

# Use token to test access
curl -H "Authorization: Bearer $TOKEN" \
     "https://your-pkgly.example.com/v2/_catalog"
```

### Debug Image Pull Issues
```bash
# Verbose pull with debug output
docker --debug pull your-pkgly.example.com/myapp:latest

# Check manifest
curl -v -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
     "https://your-pkgly.example.com/v2/myapp/manifests/latest"
```

## Configuration Options

| Setting | Default | Recommended Values |
|---------|---------|-------------------|
| Allow Tag Overwrite | false | true for development, false for production |
| Project Member Only | false | true for strict access control |
| Auth Token Required | false | true for CI/CD environments |
| Content Trust | false | true for security-sensitive environments |

## Performance Tips

### For Large Images
- Use `.dockerignore` to exclude unnecessary files
- Optimize layer caching with strategic COPY instructions
- Use multi-stage builds to reduce final image size

### For High Throughput
- Enable parallel uploads with Buildx
- Use layered caching to avoid re-uploading unchanged layers
- Monitor storage usage and implement cleanup policies

## Security Checklist

- [ ] Use HTTPS for all registry communications
- [ ] Enable authentication for private repositories
- [ ] Implement content trust for production images
- [ ] Use project-based access control
- [ ] Monitor pull/push logs regularly
- [ ] Implement image scanning workflows
- [ ] Use least privilege principles for CI/CD credentials

## Storage Optimization

### Garbage Collection
```bash
# Access repository management API (requires admin access)
curl -X POST -H "Authorization: Bearer $TOKEN" \
     "https://your-pkgly.example.com/api/repository/{repo-id}/gc"
```

### Usage Monitoring
```bash
# Get repository statistics
curl -H "Authorization: Bearer $TOKEN" \
     "https://your-pkgly.example.com/api/repository/{repo-id}/stats"
```

---

*Quick reference for Docker registry configuration and usage. See [Docker Route Reference](routes.md) for detailed API documentation.*