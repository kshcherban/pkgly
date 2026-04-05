# Python Package Repository HTTP Routes

Pkgly implements a PyPI-compatible API for Python packages for both hosted and proxy repositories. This reference details every HTTP route, its purpose, and example usage. Replace placeholders with your own hostname, storage name, repository name, package name, and authentication credentials.

- `<host>` – Pkgly base URL (e.g. `pkgly.example.com`)
- `<storage>` – Storage identifier that backs the repository
- `<repository>` – Repository name
- `<package>` – Python package name (e.g. `my-package`, `my_company_package`)
- `<version>` – Package version (e.g. `1.0.0`, `1.0.0rc1`)
- `<filename>` – Package distribution filename
- `<username>` – Username for basic authentication
- `<password>` – Password for basic authentication
- `<token>` – Bearer token for API authentication

## Repository Information

### Root Endpoint
Get repository information:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/"
```
**Response:**
```json
{
  "repository": "python-repo",
  "title": "Python Package Repository",
  "description": "Private Python package repository",
  "packages_count": 234,
  "downloads_count": 5678,
  "last_updated": "2023-12-15T10:00:00Z"
}
```

## Simple Package Index

### Simple Index Root
Get the simple package index root:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/simple/"
```
**Response:**
```html
<!DOCTYPE html>
<html>
<head>
  <title>Simple Index</title>
</head>
<body>
  <h1>Simple Index</h1>
  <a href="my-package/">my-package</a><br>
  <a href="my_company_package/">my_company_package</a><br>
  <a href="another-package/">another-package</a><br>
</body>
</html>
```

### Package Versions List
List available versions for a specific package:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/simple/<package>/"
```
**Response:**
```html
<!DOCTYPE html>
<html>
<head>
  <title>Links for my-package</title>
</head>
<body>
  <h1>Links for my-package</h1>
  <a href="https://<host>/repositories/<storage>/<repository>/packages/my-package-1.0.0.tar.gz#sha256=abc123...">my-package-1.0.0.tar.gz</a><br>
  <a href="https://<host>/repositories/<storage>/<repository>/packages/my-package-1.0.0-py3-none-any.whl#sha256=def456...">my-package-1.0.0-py3-none-any.whl</a><br>
  <a href="https://<host>/repositories/<storage>/<repository>/packages/my-package-1.1.0.tar.gz#sha256=ghi789...">my-package-1.1.0.tar.gz</a><br>
</body>
</html>
```

## Package Metadata Operations

### Get Package Metadata (JSON API)
Retrieve detailed package information in JSON format:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/pypi/<package>/<version>/json"
```
**Response:**
```json
{
  "info": {
    "name": "my-package",
    "version": "1.0.0",
    "author": "Author Name",
    "author_email": "author@example.com",
    "maintainer": "Maintainer Name",
    "maintainer_email": "maintainer@example.com",
    "home_page": "https://github.com/username/my-package",
    "license": "MIT",
    "summary": "My awesome Python package",
    "description": "Long description of the package with markdown...",
    "keywords": "awesome,package,python",
    "platform": ["any"],
    "requires_python": ">=3.7",
    "yanked": false,
    "classifiers": [
      "Development Status :: 5 - Production/Stable",
      "Intended Audience :: Developers",
      "License :: OSI Approved :: MIT License",
      "Programming Language :: Python :: 3",
      "Programming Language :: Python :: 3.7",
      "Programming Language :: Python :: 3.8",
      "Programming Language :: Python :: 3.9",
      "Programming Language :: Python :: 3.10",
      "Programming Language :: Python :: 3.11"
    ],
    "requires_dist": [
      "requests>=2.25.0",
      "click>=8.0.0",
      "pydantic>=1.8.0"
    ],
    "provides_extra": ["dev", "test"],
    "project_urls": {
      "Bug Reports": "https://github.com/username/my-package/issues",
      "Source": "https://github.com/username/my-package",
      "Documentation": "https://my-package.readthedocs.io/"
    }
  },
  "last_serial": 123456,
  "releases": {
    "1.0.0": [
      {
        "filename": "my-package-1.0.0.tar.gz",
        "python_version": "source",
        "packagetype": "sdist",
        "comment_text": "",
        "digests": {
          "md5": "abc123...",
          "sha256": "def456...",
          "blake2b_256": "ghi789..."
        },
        "downloads": -1,
        "upload_time": "2023-12-15T10:00:00",
        "upload_time_iso_8601": "2023-12-15T10:00:00.000000Z",
        "url": "https://<host>/repositories/<storage>/<repository>/packages/my-package-1.0.0.tar.gz",
        "yanked": false,
        "yanked_reason": null
      },
      {
        "filename": "my_package-1.0.0-py3-none-any.whl",
        "python_version": "py3",
        "packagetype": "bdist_wheel",
        "comment_text": "",
        "digests": {
          "md5": "jkl012...",
          "sha256": "mno345...",
          "blake2b_256": "pqr678..."
        },
        "downloads": -1,
        "upload_time": "2023-12-15T10:05:00",
        "upload_time_iso_8601": "2023-12-15T10:05:00.000000Z",
        "url": "https://<host>/repositories/<storage>/<repository>/packages/my_package-1.0.0-py3-none-any.whl",
        "yanked": false,
        "yanked_reason": null
      }
    ]
  },
  "urls": [
    {
      "filename": "my-package-1.0.0.tar.gz",
      "python_version": "source",
      "packagetype": "sdist",
      "url": "https://<host>/repositories/<storage>/<repository>/packages/my-package-1.0.0.tar.gz",
      "digests": {
        "sha256": "def456..."
      },
      "yanked": false
    },
    {
      "filename": "my_package-1.0.0-py3-none-any.whl",
      "python_version": "py3",
      "packagetype": "bdist_wheel",
      "url": "https://<host>/repositories/<storage>/<repository>/packages/my_package-1.0.0-py3-none-any.whl",
      "digests": {
        "sha256": "mno345..."
      },
      "yanked": false
    }
  ]
}
```

### Get All Package Versions
List all available versions for a package:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/pypi/<package>/json"
```

### Get Package Index Data
Get package index information:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/pypi-data/<package>/json"
```

## Package Distribution Operations

### Download Package
Download a specific package distribution file:
```bash
curl -u "<username>:<password>" -O \
     "https://<host>/repositories/<storage>/<repository>/packages/<filename>"
```
**Examples:**
```bash
# Download source distribution
curl -u "username:password" -O \
     "https://pkgly.example.com/repositories/storage/python-repo/packages/my-package-1.0.0.tar.gz"

# Download wheel distribution
curl -u "username:password" -O \
     "https://pkgly.example.com/repositories/storage/python-repo/packages/my_package-1.0.0-py3-none-any.whl"
```

### Check Package Exists
Verify if a package file exists without downloading:
```bash
curl -I -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/packages/<filename>"
```

## Upload Operations

### Upload Package File
Upload a new package distribution:
```bash
curl -X POST \
     -u "<username>:<password>" \
     -F "content=@my-package-1.0.0.tar.gz" \
     -F ":action=file_upload" \
     "https://<host>/repositories/<storage>/<repository>/pypi/<package>/upload/"
```

### Upload Package with Metadata
Upload package with additional metadata:
```bash
curl -X POST \
     -u "<username>:<password>" \
     -F "content=@my_package-1.0.0-py3-none-any.whl" \
     -F ":action=file_upload" \
     -F "metadata_version=2.1" \
     -F "name=my-package" \
     -F "version=1.0.0" \
     -F "filetype=bdist_wheel" \
     -F "pyversion=py3" \
     -F "metadata_2_name=my-package" \
     -F "metadata_2_version=1.0.0" \
     -F "metadata_2_summary=My awesome package" \
     -F "metadata_2_author=Author Name" \
     -F "metadata_2_author_email=author@example.com" \
     -F "metadata_2_license=MIT" \
     -F "metadata_2_home_page=https://github.com/username/my-package" \
     "https://<host>/repositories/<storage>/<repository>/pypi/<package>/upload/"
```

### Upload with Form Fields
Complete upload with all form fields:
```bash
curl -X POST \
     -u "<username>:<password>" \
     -F ":action=file_upload" \
     -F "content=@my-package-1.0.0.tar.gz" \
     -F "name=my-package" \
     -F "version=1.0.0" \
     -F "filetype=sdist" \
     -F "pyversion=source" \
     -F "metadata_version=2.1" \
     -F "summary=My awesome package" \
     -F "description=Long description of the package..." \
     -F "author=Author Name" \
     -F "author_email=author@example.com" \
     -F "maintainer=Maintainer Name" \
     -F "maintainer_email=maintainer@example.com" \
     -F "license=MIT" \
     -F "home_page=https://github.com/username/my-package" \
     -F "keywords=awesome,package,python" \
     -F "platform=any" \
     -F "requires_python=>=3.7" \
     -F "classifiers=Development Status :: 5 - Production/Stable" \
     -F "classifiers=Intended Audience :: Developers" \
     -F "classifiers=License :: OSI Approved :: MIT License" \
     -F "classifiers=Programming Language :: Python :: 3" \
     -F "requires_dist=requests>=2.25.0" \
     -F "requires_dist=click>=8.0.0" \
     -F "provides_extra=dev" \
     -F "project_url=Bug Reports,https://github.com/username/my-package/issues" \
     -F "project_url=Source,https://github.com/username/my-package" \
     -F "project_url=Documentation,https://my-package.readthedocs.io/" \
     "https://<host>/repositories/<storage>/<repository>/pypi/<package>/upload/"
```

## Search Operations

### Search Packages
Search for packages by name, description, or keywords:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/search?q=my-search-term&page=1&per_page=20"
```
**Response:**
```json
{
  "results": [
    {
      "name": "my-package",
      "version": "1.0.0",
      "description": "My awesome package",
      "author": "Author Name",
      "author_email": "author@example.com",
      "maintainer": "Maintainer Name",
      "maintainer_email": "maintainer@example.com",
      "license": "MIT",
      "keywords": ["awesome", "package", "python"],
      "home_page": "https://github.com/username/my-package",
      "requires_python": ">=3.7",
      "downloads": {
        "last_day": 10,
        "last_week": 50,
        "last_month": 200
      },
      "release_urls": [
        {
          "packagetype": "sdist",
          "filename": "my-package-1.0.0.tar.gz",
          "python_version": "source",
          "url": "https://<host>/repositories/<storage>/<repository>/packages/my-package-1.0.0.tar.gz"
        },
        {
          "packagetype": "bdist_wheel",
          "filename": "my_package-1.0.0-py3-none-any.whl",
          "python_version": "py3",
          "url": "https://<host>/repositories/<storage>/<repository>/packages/my_package-1.0.0-py3-none-any.whl"
        }
      ]
    }
  ],
  "info": {
    "total": 1,
    "page": 1,
    "per_page": 20,
    "pages": 1
  }
}
```

## Complete Upload Workflow

### Python Package Upload with Twine Equivalent
```bash
#!/bin/bash
HOST="https://pkgly.example.com"
STORAGE="storage"
REPO="python-repo"
PACKAGE="my-package"
VERSION="1.0.0"
USER="username"
PASS="password"

# Upload source distribution
echo "Uploading source distribution..."
curl -X POST \
     -u "${USER}:${PASS}" \
     -F ":action=file_upload" \
     -F "content@=dist/${PACKAGE}-${VERSION}.tar.gz" \
     -F "name=${PACKAGE}" \
     -F "version=${VERSION}" \
     -F "filetype=sdist" \
     -F "pyversion=source" \
     -F "metadata_version=2.1" \
     -F "summary=My awesome package" \
     -F "author=Author Name" \
     -F "author_email=author@example.com" \
     -F "license=MIT" \
     -F "requires_python=>=3.7" \
     -F "requires_dist=requests>=2.25.0" \
     "${HOST}/repositories/${STORAGE}/${REPO}/pypi/${PACKAGE}/upload/"

# Upload wheel distribution
echo "Uploading wheel distribution..."
curl -X POST \
     -u "${USER}:${PASS}" \
     -F ":action=file_upload" \
     -F "content@=dist/${PACKAGE//-/_}-${VERSION}-py3-none-any.whl" \
     -F "name=${PACKAGE}" \
     -F "version=${VERSION}" \
     -F "filetype=bdist_wheel" \
     -F "pyversion=py3" \
     -F "metadata_version=2.1" \
     -F "summary=My awesome package" \
     -F "author=Author Name" \
     -F "author_email=author@example.com" \
     -F "license=MIT" \
     -F "requires_python=>=3.7" \
     -F "requires_dist=requests>=2.25.0" \
     "${HOST}/repositories/${STORAGE}/${REPO}/pypi/${PACKAGE}/upload/"

echo "Package ${PACKAGE} version ${VERSION} uploaded successfully!"
```

## Repository Management API

### List Cached Packages
List cached Python packages:
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
     -d '{"paths": ["packages/my-package-1.0.0.tar.gz", "packages/my_package-1.0.0-py3-none-any.whl"]}' \
     "https://<host>/api/repository/<repository-id>/packages"
```

### Repository Configuration
Get Python repository configuration:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/config/python"
```

Update repository configuration:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d @config.json \
     "https://<host>/api/repository/<repository-id>/config/python"
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
  "total_packages": 123,
  "total_versions": 456,
  "total_downloads": 7890,
  "storage_used": "1.2GB",
  "last_activity": "2023-12-15T15:30:00Z",
  "top_packages": [
    {"name": "my-package", "download_count": 1234},
    {"name": "my_company_package", "download_count": 987}
  ],
  "package_types": {
    "sdist": 234,
    "bdist_wheel": 567
  }
}
```

## Proxy Repository Operations

### Proxy Route Configuration
Configure upstream PyPI repositories for proxy mode:
```json
{
  "type": "Proxy",
  "proxy": {
    "routes": [
      {
        "url": "https://pypi.org",
        "name": "PyPI Official",
        "priority": 10
      },
      {
        "url": "https://pypi.python.org/simple",
        "name": "PyPI Simple",
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
     "https://<host>/repositories/<storage>/<repository>/pypi/<package>/<version>/json"
```

## pip Client Integration

### Typical pip Workflow
When pip installs from the repository:

1. **GET `/simple/**` - Discover available packages
2. **GET `/simple/<package>/**` - Get list of versions for package
3. **GET `/packages/<filename>**` - Download specific package file
4. **Hash verification** - Verify downloaded file integrity

### pip Configuration
```ini
# ~/.pip/pip.conf
[global]
index-url = https://<host>/repositories/<storage>/<repository>/simple
extra-index-url = https://pypi.org/simple
[install]
trusted-host = <host>
```

### Environment Variables
```bash
export PIP_INDEX_URL="https://<host>/repositories/<storage>/<repository>/simple"
export PIP_EXTRA_INDEX_URL="https://pypi.org/simple"
export PIP_TRUSTED_HOST="<host>"
```

## Authentication

### Basic Authentication
All operations typically require authentication:
```bash
curl -u "username:password" \
     "https://<host>/repositories/<storage>/<repository>/simple/"
```

### API Token Authentication
Management endpoints use Bearer tokens:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/stats"
```

## Error Responses

### Common Error Codes
- `401 Unauthorized` - Authentication required or invalid credentials
- `403 Forbidden` - Insufficient permissions
- `404 Not Found` - Package, version, or file does not exist
- `409 Conflict` - Package version already exists
- `422 Unprocessable Entity` - Invalid package metadata or file
- `500 Internal Server Error` - Server-side error during upload

### Error Response Format
```json
{
  "error": "Not Found",
  "message": "Package 'nonexistent-package' not found",
  "details": {
    "package": "nonexistent-package",
    "repository": "python-repo"
  }
}
```

## Storage Layout

```
repositories/
└── storage/
    └── python-repo/
        ├── simple/
        │   ├── index.html
        │   ├── my-package/
        │   │   └── index.html
        │   └── my_company_package/
        │       └── index.html
        ├── packages/
        │   ├── my-package-1.0.0.tar.gz
        │   ├── my_package-1.0.0-py3-none-any.whl
        │   └── another-package-2.1.0-py3-none-any.whl
        └── pypi/
            └── my-package/
                ├── 1.0.0/
                │   └── json
                └── json
```

## Package Types Supported

### Source Distribution (sdist)
- **File extension**: `.tar.gz`
- **Python version**: `source`
- **Content type**: `application/x-tar`

### Wheel Distribution (bdist_wheel)
- **File extension**: `.whl`
- **Python version**: `py3`, `cp39`, etc.
- **Content type**: `application/zip`

Use these endpoints as a foundation for pip client integration, build tool configuration, or custom tooling when working with Pkgly Python repositories.

---

*Complete reference for Python package repository HTTP routes. See [Python Quick Reference](reference.md) for usage examples and configuration.*
