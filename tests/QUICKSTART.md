# Integration Tests Quick Start

## Run All Tests

```bash
cd tests
./run_integration_tests.sh
```

Expected output:
```
============================================
 Pkgly Integration Test Suite
============================================

Starting test environment...
Waiting for services to be ready...
✓ All services are healthy

Running maven integration tests...
  Testing: Building Maven test artifact ... ✓ PASS
  Testing: Upload JAR to maven-hosted ... ✓ PASS
  [... more tests ...]

✓ maven tests PASSED

[... more test suites ...]

============================================
 Final Test Summary
============================================
  Total test suites:  7
  Suites passed:      7
  Suites failed:      0

✓ ALL INTEGRATION TESTS PASSED
```

## Run Specific Test Suite

```bash
# Maven only
./run_integration_tests.sh maven

# Multiple suites
./run_integration_tests.sh maven npm docker
```

## Common Scenarios

### First Run (Build Everything)

```bash
./run_integration_tests.sh --build --clean
```

### Quick Test During Development

```bash
# Keep environment running between test runs
./run_integration_tests.sh --keep-running maven

# Make changes, then run again without rebuilding
./run_integration_tests.sh maven
```

### Debug Failing Test

```bash
# Keep environment running and enable verbose mode
./run_integration_tests.sh --verbose --keep-running maven

# In another terminal, access test runner
docker compose -f docker/docker-compose.test.yml exec test-runner bash

# Run test manually with debug output
cd /tests
bash -x test_maven.sh
```

### Clean Everything

```bash
# Stop and remove all containers and volumes
docker compose -f docker/docker-compose.test.yml down -v

# Clean rebuild
./run_integration_tests.sh --clean --build
```

## Test Suite Summary

| Suite   | Tests | Focus Areas |
|---------|-------|-------------|
| maven   | 12    | JAR/POM upload, metadata, proxy, dependency resolution |
| npm     | 14    | Package publish, scoped packages, proxy, version management |
| docker  | 8     | Registry V2 API, proxy, manifest/blob operations |
| python  | 11    | PyPI upload, pip install, Simple API, proxy |
| php     | 10    | Composer packages, packages.json, version management |
| go      | 16    | Module upload, GOPROXY, .info/.mod/.zip files, proxy |
| debian  | 6     | dpkg-deb packaging, Packages/Release indexes, pool downloads |
| cargo   | 8     | cargo publish/install, sparse index, crate downloads |
| helm    | 16    | Chart packaging, index.yaml, ChartMuseum API, pull/push |

## Troubleshooting

### "Services did not become healthy"

Check logs:
```bash
docker compose -f docker/docker-compose.test.yml logs
```

Common causes:
- Port 8888 already in use
- Database migration failure
- Insufficient memory

### "Test script not found"

Ensure you're in the `tests/` directory:
```bash
cd /home/insider/repos/pkgly/tests
./run_integration_tests.sh
```

### Tests Pass Locally But Fail in CI

- Check Docker version compatibility
- Ensure sufficient resources (CPU, memory, disk)
- Verify network connectivity for proxy tests

## Environment Variables

```bash
# Override Pkgly URL (default: http://pkgly:8888)
export PKGLY_URL="http://localhost:8888"

# Override test token (default: NPDxeLFM8ehXKteIHW7DFy1chf2QaYdf)
export TEST_TOKEN="your-token-here"

# Run tests
./run_integration_tests.sh
```

## Next Steps

- See [README.md](README.md) for detailed documentation
- Review test scripts in `integration/` directory
- Check test fixtures in `fixtures/` directory
- Examine pre-seeded data in `docker/init-db.sql`

## Quick Command Reference

```bash
# Standard commands
./run_integration_tests.sh                    # Run all tests
./run_integration_tests.sh maven npm          # Run specific suites
./run_integration_tests.sh --help             # Show help

# Build/cleanup options
--build                                        # Rebuild Docker images
--clean                                        # Clean environment first
--keep-running                                 # Don't stop after tests
--verbose                                      # Enable debug output

# Docker commands
docker compose -f docker/docker-compose.test.yml up -d      # Start environment
docker compose -f docker/docker-compose.test.yml down       # Stop environment
docker compose -f docker/docker-compose.test.yml logs -f    # View logs
docker compose -f docker/docker-compose.test.yml ps         # Check status
```
