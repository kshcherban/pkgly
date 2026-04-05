# NPM Registry HTTP Routes

Pkgly implements a CouchDB-compatible NPM registry API for both hosted and proxy repositories. This reference details every HTTP route, its purpose, and example usage. Replace placeholders with your own hostname, storage name, repository name, package name, and authentication credentials.

- `<host>` – Pkgly base URL (e.g. `pkgly.example.com`)
- `<storage>` – Storage identifier that backs the repository
- `<repository>` – Repository name
- `<package>` – NPM package name (including scope: `@mycompany/package`)
- `<version>` – Package version (e.g. `1.0.0`)
- `<revision>` – Package revision number for CouchDB compatibility
- `<username>` – Username for authentication
- `<password>` – Password for authentication
- `<token>` – Bearer token for API authentication

## Registry Information

### Root Endpoint
Get registry information and capabilities:
```bash
curl "https://<host>/repositories/<storage>/<repository>/"
```
**Response:**
```json
{
  "db_name": "npm-registry",
  "doc_count": 1234,
  "doc_del_count": 0,
  "update_seq": 5678,
  "disk_size": 1048576,
  "data_size": 524288,
  "instance_start_time": "2023-12-15T10:00:00Z"
}
```

## Package Operations

### Get Package Information
Retrieve complete package metadata including all versions:
```bash
curl "https://<host>/repositories/<storage>/<repository>/package/<package>"
```
**Response:**
```json
{
  "_id": "@mycompany/my-package",
  "_rev": "1-abc123...",
  "name": "@mycompany/my-package",
  "description": "My awesome package",
  "dist-tags": {
    "latest": "1.2.0",
    "beta": "1.3.0-beta.1"
  },
  "versions": {
    "1.0.0": {
      "name": "@mycompany/my-package",
      "version": "1.0.0",
      "description": "Initial release",
      "main": "index.js",
      "scripts": {
        "test": "jest"
      },
      "keywords": ["awesome", "package"],
      "author": {
        "name": "Author Name",
        "email": "author@example.com"
      },
      "license": "MIT",
      "dependencies": {
        "lodash": "^4.17.21"
      },
      "devDependencies": {
        "jest": "^29.0.0"
      },
      "dist": {
        "integrity": "sha512-...",
        "shasum": "abc123...",
        "tarball": "https://<host>/repositories/<storage>/<repository>/package/@mycompany/my-package/1.0.0/my-package-1.0.0.tgz"
      },
      "_npmUser": {
        "name": "username",
        "email": "user@example.com"
      },
      "_npmVersion": "8.19.0",
      "_nodeVersion": "18.12.0"
    }
  },
  "readme": "# My Package\n\nDescription of the package...",
  "_attachments": {
    "my-package-1.0.0.tgz": {
      "content_type": "application/octet-stream",
      "revpos": 1,
      "digest": "md5-abc123...",
      "length": 12345,
      "stub": true
    }
  }
}
```

### Get Specific Version
Retrieve metadata for a specific package version:
```bash
curl "https://<host>/repositories/<storage>/<repository>/package/<package>/<version>"
```
**Response:**
```json
{
  "_id": "@mycompany/my-package@1.0.0",
  "_rev": "1-def456...",
  "name": "@mycompany/my-package",
  "version": "1.0.0",
  "description": "Initial release",
  "main": "index.js",
  "dist": {
    "integrity": "sha512-...",
    "shasum": "abc123...",
    "tarball": "https://<host>/repositories/<storage>/<repository>/package/@mycompany/my-package/1.0.0/my-package-1.0.0.tgz"
  }
}
```

### Download Package Tarball
Download the actual package archive:
```bash
# Download with curl
curl -O "https://<host>/repositories/<storage>/<repository>/package/<package>/<version>/<package>-<version>.tgz"

# Download with authentication (if required)
curl -H "Authorization: Bearer <token>" \
     -O "https://<host>/repositories/<storage>/<repository>/package/<package>/<version>/<package>-<version>.tgz"
```

### Get Package Attachments
List package attachments and metadata:
```bash
curl "https://<host>/repositories/<storage>/<repository>/package/<package>?attachments=true"
```

## Publishing Operations

### Publish New Package Version
Publish a new version with CouchDB-style request:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -H "Npm-Command: publish" \
     -d @package.json \
     "https://<host>/repositories/<storage>/<repository>/package/<package>/<version>"
```

**Request Body (package.json with additional metadata):**
```json
{
  "_id": "@mycompany/my-package@1.0.0",
  "name": "@mycompany/my-package",
  "version": "1.0.0",
  "description": "My awesome package",
  "main": "index.js",
  "scripts": {
    "test": "jest"
  },
  "dependencies": {
    "lodash": "^4.17.21"
  },
  "devDependencies": {
    "jest": "^29.0.0"
  },
  "dist": {
    "integrity": "sha512-...",
    "shasum": "abc123..."
  },
  "_npmUser": {
    "name": "username",
    "email": "user@example.com"
  }
}
```

### Upload Package Tarball
Upload the package archive as an attachment:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @my-package-1.0.0.tgz \
     "https://<host>/repositories/<storage>/<repository>/package/<package>/<version>/<package>-<version>.tgz?rev=<rev>"
```

### Update Package Metadata
Update package information (dist-tags, readme, etc.):
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{
       "dist-tags": {
         "latest": "1.1.0",
         "beta": "1.2.0-beta.1"
       },
       "readme": "# Updated Readme\n\nNew description..."
     }' \
     "https://<host>/repositories/<storage>/<repository>/package/<package>?rev=<current_rev>"
```

### Unpublish Package Version
Remove a specific version:
```bash
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/package/<package>/<version>?rev=<rev>"
```

### Deprecate Package Version
Mark a version as deprecated:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{
       "deprecated": "This version has been deprecated. Please use version 2.0.0 instead."
     }' \
     "https://<host>/repositories/<storage>/<repository>/package/<package>/<version>?rev=<rev>"
```

## User Authentication

### User Registration
Create a new user account:
```bash
curl -X PUT \
     -H "Content-Type: application/json" \
     -d '{
       "_id": "org.couchdb.user:newuser",
       "name": "newuser",
       "password": "securepassword",
       "email": "user@example.com",
       "type": "user",
       "roles": [],
       "date": "2023-12-15T10:00:00.000Z"
     }' \
     "https://<host>/repositories/<storage>/<repository>/-/user/org.couchdb.user:newuser"
```

### User Authentication
Authenticate user and get session:
```bash
curl -X POST \
     -H "Content-Type: application/json" \
     -d '{
       "name": "username",
       "password": "password"
     }' \
     "https://<host>/repositories/<storage>/<repository>/-_session"
```
**Response:**
```json
{
  "ok": true,
  "name": "username",
  "roles": ["user"]
}
```

### Get Current User Information
Retrieve current user details:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/-/whoami"
```
**Response:**
```json
{
  "username": "username",
  "name": "username",
  "email": "user@example.com",
  "roles": ["user"]
}
```

## Search and Discovery

### Search Packages
Search for packages by keyword:
```bash
curl "https://<host>/repositories/<storage>/<repository>/-/v1/search?text=my-search-term&size=20&from=0"
```
**Response:**
```json
{
  "objects": [
    {
      "package": {
        "name": "@mycompany/my-package",
        "version": "1.0.0",
        "description": "My awesome package",
        "keywords": ["awesome", "package"],
        "publisher": {
          "username": "username",
          "email": "user@example.com"
        },
        "maintainers": [
          {
            "username": "username",
            "email": "user@example.com"
          }
        ]
      },
      "score": 1.0,
      "searchScore": 1.0
    }
  ],
  "total": 1,
  "time": "2023-12-15T10:00:00.000Z"
}
```

### Get Popular Packages
List most downloaded packages:
```bash
curl "https://<host>/repositories/<storage>/<repository>/-/v1/search?text=*&size=50&sort=popularity"
```

### Get Recently Updated
List recently updated packages:
```bash
curl "https://<host>/repositories/<storage>/<repository>/-/v1/search?text=*&size=50&sort=modified"
```

## Repository Management API

### List Cached Packages
List cached NPM packages:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/packages?page=1&per_page=50"
```

### Delete Cached Packages
Remove cached packages (requires repository edit permission):
```bash
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"paths": ["@mycompany/my-package/my-package-1.0.0.tgz"]}' \
     "https://<host>/api/repository/<repository-id>/packages"
```

### Repository Configuration
Get NPM repository configuration:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/config/npm"
```

Update repository configuration:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d @config.json \
     "https://<host>/api/repository/<repository-id>/config/npm"
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
  "total_packages": 456,
  "total_versions": 1234,
  "total_downloads": 56789,
  "storage_used": "1.5GB",
  "last_activity": "2023-12-15T15:30:00Z",
  "top_packages": [
    {"name": "@mycompany/core", "download_count": 1234},
    {"name": "@mycompany/utils", "download_count": 987}
  ]
}
```

## Complete Publishing Workflow

### Full Package Publishing Sequence
```bash
#!/bin/bash
HOST="https://pkgly.example.com"
STORAGE="storage"
REPO="npm-repo"
PACKAGE="@mycompany/my-package"
VERSION="1.0.0"
TOKEN="your-bearer-token"

# 1. Get current package info (if exists)
PACKAGE_REV=$(curl -s -H "Authorization: Bearer $TOKEN" \
  "${HOST}/repositories/${STORAGE}/${REPO}/package/${PACKAGE}" \
  | jq -r '._rev // empty')

# 2. Prepare package metadata
cat > package.json << EOF
{
  "_id": "${PACKAGE}@${VERSION}",
  "name": "${PACKAGE}",
  "version": "${VERSION}",
  "description": "My awesome package",
  "main": "index.js",
  "dist": {
    "integrity": "$(sha512sum my-package-1.0.0.tgz | cut -d' ' -f1)",
    "shasum": "$(sha1sum my-package-1.0.0.tgz | cut -d' ' -f1)"
  },
  "_npmUser": {
    "name": "username",
    "email": "user@example.com"
  }
}
EOF

# 3. Publish package metadata
curl -X PUT \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -H "Npm-Command: publish" \
     -d @package.json \
     "${HOST}/repositories/${STORAGE}/${REPO}/package/${PACKAGE}/${VERSION}"

# 4. Upload package tarball
curl -X PUT \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @my-package-1.0.0.tgz \
     "${HOST}/repositories/${STORAGE}/${REPO}/package/${PACKAGE}/${VERSION}/${PACKAGE}-${VERSION}.tgz"

# 5. Update dist-tags (if latest version)
curl -X PUT \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"dist-tags": {"latest": "'${VERSION}'"}}' \
     "${HOST}/repositories/${STORAGE}/${REPO}/package/${PACKAGE}?rev=${PACKAGE_REV}"
```

## Proxy Repository Operations

### Proxy Route Configuration
Configure upstream registries for proxy mode:
```json
{
  "type": "Proxy",
  "proxy": {
    "routes": [
      {
        "url": "https://registry.npmjs.org",
        "name": "NPM Official",
        "priority": 10
      },
      {
        "url": "https://registry.yarnpkg.com",
        "name": "Yarn Registry",
        "priority": 5
      }
    ]
  }
}
```

### Proxy Cache Bypass
Force refresh from upstream repositories:
```bash
curl -H "Authorization: Bearer <token>" \
     -H "Cache-Control: no-cache" \
     "https://<host>/repositories/<storage>/<repository>/package/<package>"
```

## Authentication Methods

### Bearer Token Authentication
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/package/<package>"
```

### Basic Authentication
```bash
curl -u "username:password" \
     "https://<host>/repositories/<storage>/<repository>/package/<package>"
```

## Error Responses

### Common Error Codes
- `401 Unauthorized` - Authentication required or invalid credentials
- `403 Forbidden` - Insufficient permissions
- `404 Not Found` - Package or version does not exist
- `409 Conflict` - Version already exists or revision conflict
- `422 Unprocessable Entity` - Invalid package metadata
- `500 Internal Server Error` - Server-side error during upload

### Error Response Format
```json
{
  "error": "Forbidden",
  "message": "You do not have permission to publish this package",
  "details": {
    "package": "@mycompany/my-package",
    "version": "1.0.0",
    "user": "username"
  }
}
```

Use these endpoints as a foundation for NPM client integration, package managers like Yarn, or custom tooling when working with Pkgly NPM repositories.

---

*Complete reference for NPM registry HTTP routes. See [NPM Quick Reference](reference.md) for usage examples and configuration.*