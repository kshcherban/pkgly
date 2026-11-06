# Pkgly Integration Test Suite

Comprehensive end-to-end integration tests for Pkgly, testing all supported package types using native package management tooling.

## Overview

This test suite validates the complete functionality of Pkgly by testing real-world package management workflows using native tools (maven, npm, docker, helm, go, pip, composer) in isolated Docker containers.

## Architecture

```
tests/
├── docker/                          # Docker infrastructure
│   ├── docker-compose.test.yml     # Test environment orchestration
│   ├── Dockerfile.test-runner      # Test runner with all package managers
│   ├── config/
│   │   └── pkgly.test.toml    # Test server configuration
│   └── init-db.sql                 # Database seed data (repos, users, tokens)
├── integration/                     # Test scripts
│   ├── common.sh                   # Shared utilities and functions
│   ├── test_maven.sh               # Maven integration tests
│   ├── test_npm.sh                 # NPM integration tests
│   ├── npm_registry_behavior.sh    # NPM routes, casing, and permissions
│   ├── test_docker.sh              # Docker integration tests
│   ├── test_python.sh              # Python/PyPI integration tests
│   ├── test_python_virtual.sh      # Python virtual repository integration tests
│   ├── test_php.sh                 # PHP/Composer integration tests
│   ├── test_go.sh                  # Go module integration tests
│   ├── test_debian.sh              # Debian repository integration tests
│   ├── test_cargo.sh               # Cargo registry integration tests
│   ├── test_helm.sh                # Helm chart integration tests
│   ├── test_security.sh            # Security hardening integration tests (CORS, traversal, egress, reset poisoning)
├── fixtures/                        # Test packages
│   ├── maven/simple-lib/           # Maven test library
│   ├── npm/hello-pkg/              # NPM test package
│   ├── python/test-pkg/            # Python test package
│   ├── php/sample-lib/             # PHP test library
│   ├── go/test-module/             # Go test module
│   ├── helm/test-chart/            # Helm test chart
│   ├── cargo/pkgly-cargo-test/     # Cargo binary fixture
│   └── docker/Dockerfile.testimg   # Docker test image
├── run_integration_tests.sh         # Main test orchestrator
└── README.md                        # This file
```

## Supported Package Types

| Package Type | Hosted | Proxy | Tests |
|-------------|--------|-------|-------|
| Maven       | ✅     | ✅    | 12    |
| NPM         | ✅     | ✅    | 140   |
| Docker      | ⚠️     | ✅    | 8     |
| Python      | ✅     | ✅    | 11    |
| Python (Virtual) | ✅ | ✅ | 10 |
| PHP         | ✅     | ❌    | 10    |
| Go          | ✅     | ✅    | 16    |
| Debian      | ✅     | ❌    | 6     |
| Cargo       | ✅     | ❌    | 8     |
| Helm        | ✅     | ❌    | 16    |

**Legend:**
- ✅ Fully tested
- ⚠️ Partially tested (API only)
- ❌ Not supported/tested

## Prerequisites

- Docker and Docker Compose installed
- Sufficient disk space (~2GB for images)
- Port 8888 available (or configure different port)

## Quick Start

### Run All Tests

```bash
cd tests
./run_integration_tests.sh
```

### Run Specific Test Suites

```bash
# Single suite
./run_integration_tests.sh maven

# Multiple suites
./run_integration_tests.sh maven npm docker

# All suites explicitly
./run_integration_tests.sh all
```

### Common Options

```bash
# Rebuild Docker images before testing
./run_integration_tests.sh --build

# Clean environment and rebuild
./run_integration_tests.sh --clean --build

# Keep environment running after tests
./run_integration_tests.sh --keep-running maven

# Verbose output
./run_integration_tests.sh --verbose
```

## Test Environment

### Storage deletion failure coverage

Run the Rust storage deletion tests with `cargo test -q -p pkgly app::api::storage::tests`.
They require Docker and start isolated PostgreSQL and RustFS containers using a
pinned RustFS server image; no host installation or filesystem capabilities are
required.

The partial-cleanup test denies `s3:DeleteObject` for one repository through a
RustFS bucket policy. It checks that another repository's objects are removed, the
failed deletion retains database and runtime records, and removing the policy
allows a successful retry. The RustFS container is owned by the fixture, so it and
its bucket policy are removed when the test finishes.

Run the publish/delete end-to-end workflow with
`./tests/run_integration_tests.sh storage_deletion`.

### Services

The test environment consists of three Docker containers:

1. **postgres** - PostgreSQL 17 database with pre-seeded test data
2. **pkgly** - Pkgly server built from local source
3. **test-runner** - Debian Trixie container with all package management tools

### CI image checks

PR builds import the published Pkgly amd64 cache from GHCR. The `Test runner cache`
workflow populates the runner's GitHub Actions cache on `main`, where all PRs can
restore it, and refreshes the tools every Monday.

After building `pkgly-test-runner:ci`, run the image checks before the repository
suites:

```bash
docker run --rm \
  -v "$PWD/tests/docker/test-runner.sh:/test-runner.sh:ro" \
  -v "$PWD/tests/fixtures:/fixtures:ro" \
  pkgly-test-runner:ci bash /test-runner.sh
PKGLY_INTEGRATION_COMPOSE_OVERRIDE="$PWD/tests/docker/docker-compose.ci-cache.yml" \
  ./tests/run_integration_tests.sh
```

Run these commands from the repository root. The image checks verify package tools,
Java discovery, and building and installing a Python wheel in an isolated venv.
See [CI build caching](../docs/docs/ci.md) for cache ownership and refresh behavior.

### Pre-configured Data

The database is initialized with:

- **Test User:** `test` / `test123` (admin with all permissions)
- **Test Token:** `NPDxeLFM8ehXKteIHW7DFy1chf2QaYdf` (never expires, all scopes, token name `test124`)
- **Test Storage:** `test-storage` (local filesystem)
- **Test Repositories:**
  - `maven-hosted` (MavenHosted)
  - `maven-proxy` (MavenProxy → Maven Central)
  - `npm-hosted` (NpmHosted)
  - `npm-proxy` (NpmProxy → npmjs.org)
  - `docker-proxy` (DockerProxy → Docker Hub)
  - `python-hosted` (PythonHosted)
  - `python-proxy` (PythonProxy → PyPI)
  - `php-hosted` (PhpHosted)
  - `go-hosted` (GoHosted)
  - `go-proxy` (GoProxy → proxy.golang.org)
  - `helm-hosted` (HelmHosted)
  - `helm-oci` (Helm OCI)
  - `deb-hosted` (DebianHosted)
  - `cargo-hosted` (CargoHosted)

### Network

All containers communicate on the `test-network` bridge network. Tests run inside the `test-runner` container with access to `pkgly:8888`.

## Test Coverage

### Maven Tests (12 tests)

1. ✅ Build Maven artifact
2. ✅ Upload JAR to hosted repository
3. ✅ Upload POM to hosted repository
4. ✅ Download JAR from hosted repository
5. ✅ Verify JAR integrity (SHA256)
6. ✅ Upload second version
7. ✅ Verify both versions accessible
8. ✅ Maven metadata generation
9. ✅ Maven dependency resolution
10. ✅ Proxy artifact from Maven Central
11. ✅ Proxy caching verification
12. ✅ Authentication and error handling

### NPM Tests

The npm suite uses native npm and Yarn commands, plus direct HTTP requests for inputs that the clients reject or do not send.

1. Publish and install scoped and unscoped packages and specific versions.
2. Preserve prerelease identifiers and build metadata.
3. Publish with beta, latest, multiple tags, omitted tags, and empty tags.
4. Add, resolve, promote, and remove distribution tags through npm and virtual repositories.
5. Preserve distinct tag names and accept npm-compatible punctuation.
6. Decode scoped paths and tag characters once, with either percent-escape case.
7. Reject Unicode tags, malformed bodies, and invalid paths without metadata changes.
8. Update README and manifest metadata through new publications.
9. Deprecate and restore selected versions without changing unrelated metadata.
10. Proxy and cache packages, including scoped Yarn installations.
11. Publish through mixed-case storage and repository URLs.
12. Deny writes with read-only tokens and preserve stored metadata.

`npm_registry_behavior.sh` and `npm_prerelease_lifecycle.sh` run as part of `test_npm.sh` and use its fixtures and result counters.
The npm lifecycle suite covers successive beta publications, fresh and reused client caches, and tag removal from beta-only packages.
Rust HTTP tests also cover rejected publications without partial catalog changes and prerelease downloads through hosted, proxy, and virtual repositories.
The retrieval fixture stops its controlled upstream before repeated downloads to check cache use.
Rust HTTP tests also check tag cleanup after version deletion and tarball HEAD responses.
The mixed cache test checks offline GET and HEAD requests against all three metadata layouts.
It also checks that the new entry takes precedence when all three layouts contain the same document.

Database-backed HTTP handler tests preserve the existing npm read contract and cover tag Read and Write permissions, token scopes, virtual delegation, audit actions, and handler spans.
They cover both GET and HEAD, and check that traces exclude request bodies and credentials.

#### Concurrent tag updates

Run the PostgreSQL concurrency tests with:

```sh
cargo test -q -p pkgly repository::npm::tags::tests::concurrency
```

Each test uses separate database connections for two writers.
A third connection holds a row lock on the package before either write starts.
The test checks `pg_stat_activity` until both writers wait for a database lock, then releases the held lock.
This forces overlapping writes without relying on an arbitrary delay.
Timeouts turn blocked or stalled tests into failures.

The cases cover two tag additions, initialization of legacy tag storage, removal during another addition, and two writes to the same tag.
They assert the complete final tag map, including preservation of `latest` and removal of the deleted tag.
The database suite checks that npm retains the shared case-insensitive catalog schema and that upgrades preserve existing packages.

### Docker Tests (8 tests)

1. ✅ Verify Docker Registry V2 API
2. ✅ Build test Docker image
3. ⚠️ Pull image through proxy
4. ✅ Fetch image manifest via API
5. ✅ Verify blob endpoint
6. ✅ Verify tags list endpoint
7. ✅ Authentication for manifest upload
8. ✅ Verify catalog endpoint

### Python Tests (11 tests)

1. ✅ Build Python distribution package
2. ✅ Upload package using twine
3. ✅ Fetch package via PyPI Simple API
4. ✅ Install package using pip
5. ✅ Verify installed package functionality
6. ✅ Upload second version
7. ✅ Install specific version
8. ✅ Proxy package from PyPI
9. ✅ Proxy caching verification
10. ✅ Authentication required for upload
11. ✅ 404 for non-existent package

### Python Virtual Repository Tests (10 tests)

1. ✅ Create/validate python-virtual repo configuration
2. ✅ Publish package version to member 1 (hosted)
3. ✅ Publish package version to member 2 (hosted)
4. ✅ Merged `/simple/<pkg>/` contains both versions
5. ✅ Install specific version via virtual (member merge) - version 1
6. ✅ Install specific version via virtual (member merge) - version 2
7. ✅ Install proxied package via virtual (proxy member)
8. ✅ Publish via virtual forwards to hosted publish target
9. ✅ Verify publish target contains forwarded version

### PHP Tests (10 tests)

1. ✅ Create PHP package archive
2. ✅ Upload package to hosted repository
3. ✅ Create/update packages.json
4. ✅ Configure Composer repository
5. ✅ Install package using Composer
6. ✅ Verify installed package functionality
7. ✅ Upload second version
8. ✅ Update packages.json with both versions
9. ✅ Install specific version
10. ✅ Authentication and error handling

### Go Tests (16 tests)

1. ✅ Create Go module archive
2. ✅ Upload module .zip file
3. ✅ Upload .info file
4. ✅ Upload .mod file
5. ✅ Fetch version list
6. ✅ Download .info file
7. ✅ Download .mod file
8. ✅ Download .zip file
9. ✅ Install module via GOPROXY
10. ✅ Build and run consumer application
11. ✅ Upload second version
12. ✅ Verify both versions in list
13. ✅ Proxy module from proxy.golang.org
14. ✅ Proxy caching verification
15. ✅ Authentication required for upload
16. ✅ 404 for non-existent module

### Debian Tests (6 tests)

1. ✅ Build `.deb` package with custom control metadata
2. ✅ Upload package via multipart form (`distribution=stable`, `component=main`)
3. ✅ Verify `Packages` index lists the package/version
4. ✅ Verify `Release` file advertises suite/component metadata
5. ✅ Download artifact from `pool/` layout
6. ✅ Compare SHA256 hashes between original and downloaded package

### Cargo Tests (8 tests)

1. ✅ Publish crate via `cargo publish --registry pkgly`
2. ✅ Verify local crate archive creation
3. ✅ Confirm sparse index JSON includes the new version
4. ✅ Fetch `/api/v1/crates/<name>` metadata for published version
5. ✅ Download crate via `/api/v1/crates/<name>/<version>/download`
6. ✅ Match crate SHA256 against local build
7. ✅ Install crate via `cargo install --registry pkgly`
8. ✅ Execute installed binary to validate runtime behavior

### Helm Tests (16 tests)

1. ✅ Package Helm chart
2. ✅ Upload chart via HTTP PUT
3. ✅ Upload chart via ChartMuseum API
4. ✅ Fetch index.yaml
5. ✅ Verify both versions in index
6. ✅ Add Helm repository
7. ✅ Update Helm repository
8. ✅ Search for chart
9. ✅ Pull chart from repository
10. ✅ Verify pulled chart integrity
11. ✅ Pull latest version
12. ✅ Download chart via direct URL
13. ✅ ChartMuseum health endpoint
14. ✅ Authentication required for upload
15. ✅ 404 for non-existent chart
16. ✅ Delete chart via API

## Test Fixtures

Minimal test packages are provided in `tests/fixtures/` for each package type. These are designed to be:

- **Small:** Quick to build and transfer
- **Complete:** Functional packages with dependencies
- **Versioned:** Support multi-version testing
- **Consistent:** All follow similar structure (greet function, version info)

## Writing New Tests

### Test Script Template

```bash
#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# Configuration
PACKAGE_REPO="${TEST_STORAGE}/package-hosted"
FIXTURE_DIR="/fixtures/package/sample"

print_section "Package Integration Tests"

WORKSPACE=$(create_workspace "package")
cd "$WORKSPACE"

# Test 1: Description
print_test "Test description"
if [[ test condition ]]; then
    pass
else
    fail "Error message"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"

print_summary
```

### Available Utilities (common.sh)

**Output Functions:**
- `print_section(name)` - Print section header
- `print_test(name)` - Print test name and increment counter
- `pass()` - Mark test as passed
- `fail(message)` - Mark test as failed

**Assertions:**
- `assert_http_status(expected, actual, [message])` - Assert HTTP status code
- `assert_contains(haystack, needle, [message])` - Assert substring exists
- `assert_file_exists(path, [message])` - Assert file exists

**HTTP Functions:**
- `api_get(path, [curl_args])` - Authenticated GET request
- `api_put(path, [curl_args])` - Authenticated PUT request
- `api_post(path, [curl_args])` - Authenticated POST request
- `api_delete(path, [curl_args])` - Authenticated DELETE request
- `get_http_status(url, [curl_args])` - Get HTTP status code

**Utilities:**
- `wait_for_server([max_wait])` - Wait for Pkgly to be healthy
- `create_workspace(name)` - Create temporary workspace directory
- `cleanup_workspace(path)` - Remove workspace directory
- `get_auth_header()` - Get Authorization header
- `verify_repository_exists(name)` - Check if repository exists
- `random_string([length])` - Generate random string

### Test Design Principles

Following the mission-critical mindset from project guidelines:

1. **Test First:** Tests define expected behavior before implementation
2. **Isolation:** Each test creates its own workspace and cleans up
3. **Deterministic:** Tests use fixed versions and checksums
4. **Clear Errors:** Failure messages indicate exactly what went wrong
5. **Native Tools:** Use actual package managers, not just HTTP calls
6. **Complete Cycle:** Test upload → download → verify → use

## Troubleshooting

### Tests Fail to Start

```bash
# Check Docker service
docker ps

# Check if ports are available
netstat -an | grep 8888

# View service logs
docker compose -f tests/docker/docker-compose.test.yml logs
```

### Tests Hang or Timeout

```bash
# Check service health
docker compose -f tests/docker/docker-compose.test.yml ps

# Check Pkgly logs
docker compose -f tests/docker/docker-compose.test.yml logs pkgly

# Check database logs
docker compose -f tests/docker/docker-compose.test.yml logs postgres
```

### Specific Test Fails

```bash
# Run single test with verbose output
./run_integration_tests.sh --verbose --keep-running maven

# Access test-runner container
docker compose -f tests/docker/docker-compose.test.yml exec test-runner bash

# Run test manually
cd /tests
bash -x test_maven.sh
```

### Clean Slate

```bash
# Stop and remove everything
docker compose -f tests/docker/docker-compose.test.yml down -v

# Remove test artifacts
rm -rf /tmp/pkgly-test-*

# Rebuild and run
./run_integration_tests.sh --clean --build
```

## CI/CD Integration

While these tests are designed for local development, they can be integrated into CI pipelines:

```yaml
# Example GitHub Actions workflow
- name: Run Integration Tests
  run: |
    cd tests
    ./run_integration_tests.sh --build --clean
```

## Development Workflow

### Adding a New Package Type

1. Add repository configuration to `tests/docker/init-db.sql`
2. Create test fixture in `tests/fixtures/newtype/`
3. Create test script `tests/integration/test_newtype.sh`
4. Update `run_integration_tests.sh` to include new type
5. Update this README with test coverage

### Debugging Issues

1. Keep environment running: `--keep-running`
2. Access test-runner: `docker compose exec test-runner bash`
3. Inspect files: `/fixtures`, `/tests`, `/workspace`
4. Check Pkgly: `curl http://pkgly:8888/api/health`
5. Check database: `docker compose exec postgres psql -U pkgly -d pkgly_test`

## Performance

Average test suite execution time on typical hardware:

- Maven: ~45 seconds
- NPM: ~60 seconds
- Docker: ~30 seconds
- Python: ~50 seconds
- PHP: ~40 seconds
- Go: ~55 seconds
- Helm: ~50 seconds

**Total:** ~5-6 minutes for all suites

## License

Part of Pkgly project. See main project LICENSE file.

## Contributing

When contributing tests:

1. Follow existing test patterns in `common.sh`
2. Keep tests focused and deterministic
3. Clean up all test artifacts
4. Document any new utilities or patterns
5. Update this README with new test coverage

---

**Mission-Critical Testing:** Every line of code is tested. Every test must pass. No exceptions.
