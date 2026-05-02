#!/bin/bash
# ABOUTME: Orchestrates Docker-backed integration test suites for Pkgly.
# ABOUTME: Starts the shared stack and runs selected repository or web tests.
# Main integration test orchestrator
# Runs all or specific integration tests for Pkgly

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="${SCRIPT_DIR}/docker"
INTEGRATION_DIR="${SCRIPT_DIR}/integration"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_color() {
    local color="$1"
    local message="$2"
    echo -e "${color}${message}${NC}"
}

print_header() {
    echo ""
    print_color "$BLUE" "============================================"
    print_color "$BLUE" " Pkgly Integration Test Suite"
    print_color "$BLUE" "============================================"
    echo ""
}

print_usage() {
    cat <<EOF
Usage: $0 [OPTIONS] [TEST_SUITE...]

Run integration tests for Pkgly package repositories.

OPTIONS:
    -h, --help              Show this help message
    -b, --build             Rebuild Docker images before testing
    -c, --clean             Clean up test environment before starting
    -s, --stop              Stop test environment after completion
    -v, --verbose           Verbose output (set -x)
    -k, --keep-running      Keep test environment running after tests

TEST_SUITES:
    maven       Run Maven integration tests
    npm         Run NPM integration tests
    docker      Run Docker hosted registry integration tests
    docker_proxy Run Docker proxy cache integration tests
    python      Run Python/PyPI integration tests
    python_virtual Run Python virtual repository integration tests
    php         Run PHP/Composer integration tests
    ruby        Run RubyGems integration tests
    go          Run Go integration tests
    debian      Run Debian repository integration tests
    cargo       Run Cargo registry integration tests
    helm        Run Helm integration tests
    nuget       Run NuGet integration tests
    web_refresh Run web route refresh integration tests
    all         Run all test suites (default)

EXAMPLES:
    # Run all tests
    $0

    # Run specific test suites
    $0 maven npm

    # Rebuild images and run tests
    $0 --build --clean

    # Run tests and keep environment running
    $0 --keep-running maven

ENVIRONMENT VARIABLES:
    PKGLY_URL       Pkgly server URL (default: http://pkgly:8888)
    TEST_TOKEN      Authentication token (default: NPDxeLFM8ehXKteIHW7DFy1chf2QaYdf)

EOF
}

# Parse arguments
BUILD=0
CLEAN=0
STOP=1
VERBOSE=0
KEEP_RUNNING=0
TEST_SUITES=()

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            print_usage
            exit 0
            ;;
        -b|--build)
            BUILD=1
            shift
            ;;
        -c|--clean)
            CLEAN=1
            shift
            ;;
        -s|--stop)
            STOP=1
            shift
            ;;
        -v|--verbose)
            VERBOSE=1
            shift
            ;;
        -k|--keep-running)
            KEEP_RUNNING=1
            STOP=0
            shift
            ;;
        maven|npm|docker|docker_proxy|python|python_virtual|php|ruby|go|debian|cargo|helm|nuget|web_refresh)
            TEST_SUITES+=("$1")
            shift
            ;;
        all)
            TEST_SUITES=(maven npm docker docker_proxy python python_virtual php ruby go debian cargo helm nuget web_refresh)
            shift
            ;;
        *)
            print_color "$RED" "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac
done

# Default to all tests if none specified
if [ ${#TEST_SUITES[@]} -eq 0 ]; then
    TEST_SUITES=(maven npm docker docker_proxy python python_virtual php ruby go debian cargo helm nuget web_refresh)
fi

# Enable verbose mode
if [ $VERBOSE -eq 1 ]; then
    set -x
fi

print_header

# Clean up if requested
if [ $CLEAN -eq 1 ]; then
    print_color "$YELLOW" "Cleaning up test environment..."
    docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" down -v > /dev/null 2>&1 || true
    print_color "$GREEN" "✓ Cleanup complete"
    echo ""
fi

# Build images if requested
if [ $BUILD -eq 1 ]; then
    print_color "$YELLOW" "Building Docker images..."
    docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" build
    print_color "$GREEN" "✓ Build complete"
    echo ""
fi

REQUIRED_SERVICES=(postgres pkgly test-runner docker)
RUNNING_SERVICES=$(docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" ps --status running --services 2>/dev/null || true)
ALL_REQUIRED_RUNNING=1
for svc in "${REQUIRED_SERVICES[@]}"; do
    if ! grep -qx "$svc" <<<"$RUNNING_SERVICES"; then
        ALL_REQUIRED_RUNNING=0
        break
    fi
done

REUSE_ENV=0
if [ $ALL_REQUIRED_RUNNING -eq 1 ] && [ $CLEAN -eq 0 ] && [ $BUILD -eq 0 ]; then
    REUSE_ENV=1
fi

if [ $REUSE_ENV -eq 1 ]; then
    print_color "$YELLOW" "Test environment already running; reusing existing containers"
    echo ""
else
    # Start test environment
    print_color "$YELLOW" "Starting test environment..."
    docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" up -d

    # Wait for services to be healthy
    print_color "$YELLOW" "Waiting for services to be ready..."
    MAX_WAIT=120
    ELAPSED=0

    while [ $ELAPSED -lt $MAX_WAIT ]; do
        if docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" ps | grep -q "healthy"; then
            POSTGRES_HEALTHY=$(docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" ps postgres | grep -q "healthy" && echo 1 || echo 0)
            if [ "$POSTGRES_HEALTHY" -eq 1 ]; then
                print_color "$GREEN" "✓ All services are healthy"
                break
            fi
        fi

        sleep 2
        ((ELAPSED+=2))

        if [ $((ELAPSED % 10)) -eq 0 ]; then
            echo "  Still waiting... (${ELAPSED}s / ${MAX_WAIT}s)"
        fi
    done

    if [ $ELAPSED -ge $MAX_WAIT ]; then
        print_color "$RED" "✗ Services did not become healthy within ${MAX_WAIT} seconds"
        docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" logs
        docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" down
        exit 1
    fi

    # Restart pkgly to load seeded repositories
    print_color "$YELLOW" "Restarting Pkgly to load seeded repositories..."
    docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" restart pkgly
    sleep 2  # Give it time to restart
    print_color "$GREEN" "✓ Pkgly restarted"

    echo ""
fi

# Run test suites
TOTAL_SUITES=${#TEST_SUITES[@]}
PASSED_SUITES=0
FAILED_SUITES=0
declare -a FAILED_SUITE_NAMES

for suite in "${TEST_SUITES[@]}"; do
    TEST_SCRIPT="${INTEGRATION_DIR}/test_${suite}.sh"

    if [ ! -f "$TEST_SCRIPT" ]; then
        print_color "$RED" "✗ Test script not found: $TEST_SCRIPT"
        ((FAILED_SUITES+=1))
        FAILED_SUITE_NAMES+=("$suite")
        continue
    fi

    print_color "$BLUE" "Running ${suite} integration tests..."
    echo ""

    # Make script executable
    chmod +x "$TEST_SCRIPT"

    # Run test in test-runner container
    if docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" exec -T test-runner \
       bash "/tests/test_${suite}.sh"; then
        print_color "$GREEN" "✓ ${suite} tests PASSED"
        ((PASSED_SUITES+=1))
    else
        print_color "$RED" "✗ ${suite} tests FAILED"
        ((FAILED_SUITES+=1))
        FAILED_SUITE_NAMES+=("$suite")
    fi

    echo ""
done

# Print final summary
echo ""
print_color "$BLUE" "============================================"
print_color "$BLUE" " Final Test Summary"
print_color "$BLUE" "============================================"
echo "  Total test suites:  $TOTAL_SUITES"
print_color "$GREEN" "  Suites passed:      $PASSED_SUITES"

if [ $FAILED_SUITES -gt 0 ]; then
    print_color "$RED" "  Suites failed:      $FAILED_SUITES"
    echo "  Failed suites:      ${FAILED_SUITE_NAMES[*]}"
else
    echo "  Suites failed:      $FAILED_SUITES"
fi

echo ""

# Stop environment if requested
if [ $STOP -eq 1 ] && [ $KEEP_RUNNING -eq 0 ]; then
    print_color "$YELLOW" "Stopping test environment..."
    docker compose -f "${DOCKER_DIR}/docker-compose.test.yml" down
    print_color "$GREEN" "✓ Environment stopped"
else
    print_color "$YELLOW" "Test environment is still running"
    echo "  To view logs:  docker compose -f ${DOCKER_DIR}/docker-compose.test.yml logs -f"
    echo "  To stop:       docker compose -f ${DOCKER_DIR}/docker-compose.test.yml down"
fi

echo ""

# Exit with appropriate code
if [ $FAILED_SUITES -eq 0 ]; then
    print_color "$GREEN" "✓ ALL INTEGRATION TESTS PASSED"
    exit 0
else
    print_color "$RED" "✗ SOME INTEGRATION TESTS FAILED"
    exit 1
fi
