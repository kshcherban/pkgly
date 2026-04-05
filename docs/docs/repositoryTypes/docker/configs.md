# Docker Repository Configuration

## Docker Push Rules - Options - Hosted Only

Docker repositories provide granular control over push operations and access policies.

### Configuration Options

- **`allow_tag_overwrite`**: Whether or not overwriting tags is allowed. This is a boolean value.
  - `true`: Tags can be overwritten (e.g., pushing to `latest` multiple times)
  - `false`: Once a tag is created, it cannot be changed
  - **Default:** `false` (recommended for production)
  - **Note:** Even when `false`, you can still push new tags for the same image

- **`must_be_project_member`**: Whether or not the user must be a member of the project to push images. This is a boolean value.
  - `true`: Only project members can push
  - `false`: Any authenticated user can push
  - **Default:** `false`
  - **Use Case:** Enable this for team-based workflows where only specific users should be able to push images

- **`must_use_auth_token_for_push`**: If true, the user must use an auth token to push images. This is a boolean value.
  - `true`: Username/password authentication is not allowed for pushes, only tokens
  - `false`: Both password and token authentication are allowed
  - **Default:** `false`
  - **Security Note:** Enable this for enhanced security. Users can still use tokens with any username:
    ```bash
    docker login your-registry.com
    Username: token
    Password: nr_your_auth_token_here
    ```

- **`require_content_trust`**: If true, requires signed images (Docker Content Trust/Notary). This is a boolean value.
  - `true`: Only signed images are accepted
  - `false`: Unsigned images are allowed
  - **Default:** `false`
  - **Status:** This feature is planned for future implementation

## Authentication Configuration

Docker repositories inherit from the general repository authentication settings:

- **`enabled`**: Controls whether authentication is required for operations
  - When **disabled** (`false`):
    - Pull operations (GET/HEAD): Anonymous access allowed
    - Push operations (PUT/POST/DELETE): Authentication required
  - When **enabled** (`true`):
    - All operations require authentication

This follows standard container registry patterns where public registries allow anonymous pulls but require authentication for pushes.

## Example Configuration

### Public Docker Registry (Pull-only)

```json
{
  "type": "docker",
  "config": {
    "type": "Hosted"
  },
  "auth": {
    "enabled": false
  },
  "push_rules": {
    "allow_tag_overwrite": false,
    "must_be_project_member": false,
    "must_use_auth_token_for_push": false,
    "require_content_trust": false
  }
}
```

**Behavior:**
- Anyone can pull images anonymously
- Authenticated users can push new images
- Tags cannot be overwritten

### Private Team Registry

```json
{
  "type": "docker",
  "config": {
    "type": "Hosted"
  },
  "auth": {
    "enabled": true
  },
  "push_rules": {
    "allow_tag_overwrite": false,
    "must_be_project_member": true,
    "must_use_auth_token_for_push": true,
    "require_content_trust": false
  }
}
```

**Behavior:**
- All operations require authentication
- Only project members can push
- Must use auth tokens (not passwords)
- Tags cannot be overwritten

### Development Registry

```json
{
  "type": "docker",
  "config": {
    "type": "Hosted"
  },
  "auth": {
    "enabled": false
  },
  "push_rules": {
    "allow_tag_overwrite": true,
    "must_be_project_member": false,
    "must_use_auth_token_for_push": false,
    "require_content_trust": false
  }
}
```

**Behavior:**
- Anonymous pulls allowed
- Any authenticated user can push
- Tags can be overwritten (useful for `latest`, `dev`, etc.)
- Flexible for development workflows

## Best Practices

### Production Registries

1. **Enable authentication** for all operations
2. **Disable tag overwriting** to ensure immutability
3. **Require project membership** to control who can push
4. **Use auth tokens** for CI/CD pipelines

### Development Registries

1. **Allow anonymous pulls** for convenience
2. **Enable tag overwriting** for iterative development
3. **Keep project membership flexible** for collaboration

### Security Recommendations

1. **Use auth tokens** instead of passwords for automated systems (CI/CD)
2. **Create separate repositories** for production and development
3. **Regularly rotate auth tokens** used in CI/CD
4. **Monitor repository access logs** for suspicious activity
5. **Enable project membership requirements** for sensitive images

## Storage Considerations

Docker images can be large (multi-GB). Consider:

- **Adequate storage capacity** for your registry
- **Garbage collection policies** to remove unused layers (future feature)
- **Layer deduplication** is automatic (same layers shared across images)

## Future Features

The following features are planned for future releases:

- **Proxy Mode**: Cache images from Docker Hub and other registries
- **Content Trust**: Require signed images (Docker Notary)
- **Garbage Collection**: Automatically remove unused layers
- **Vulnerability Scanning**: Scan images for security vulnerabilities
- **Replication**: Replicate images across multiple Pkgly instances
