#!/bin/bash
# Common utilities for integration tests
# Mission-critical test infrastructure - every function must be reliable

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
export PKGLY_URL="${PKGLY_URL:-http://pkgly:8888}"
export TEST_TOKEN="${TEST_TOKEN:-NPDxeLFM8ehXKteIHW7DFy1chf2QaYdf}"
export TEST_USER="admin"
export TEST_PASSWORD="TestAdmin"
export TEST_STORAGE="test-storage"
export LOG_DIR="${LOG_DIR:-/results/logs}"
mkdir -p "${LOG_DIR}"
LAST_CMD_LOG=""

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

#######################################
# Print colored message
# Arguments:
#   $1: Color code
#   $2: Message
#######################################
print_color() {
    local color="$1"
    local message="$2"
    echo -e "${color}${message}${NC}"
}

#######################################
# Print test section header
# Arguments:
#   $1: Section name
#######################################
print_section() {
    local section="$1"
    echo ""
    print_color "$BLUE" "============================================"
    print_color "$BLUE" " $section"
    print_color "$BLUE" "============================================"
}

#######################################
# Print test name
# Arguments:
#   $1: Test name
#######################################
print_test() {
    local test_name="$1"
    TESTS_RUN=$((TESTS_RUN + 1))
    echo -n "  Testing: $test_name ... "
}

#######################################
# Mark test as passed
#######################################
pass() {
    TESTS_PASSED=$((TESTS_PASSED + 1))
    print_color "$GREEN" "✓ PASS"
}

#######################################
# Mark test as failed
# Arguments:
#   $1: Error message
#######################################
fail() {
    local message="${1:-Unknown error}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
    print_color "$RED" "✗ FAIL"
    print_color "$RED" "    Error: $message"
    if [[ -n "$LAST_CMD_LOG" && -f "$LAST_CMD_LOG" ]]; then
        print_color "$YELLOW" "    Command output (stored in $LAST_CMD_LOG):"
        sed 's/^/      | /' "$LAST_CMD_LOG"
        LAST_CMD_LOG=""
    fi
}

#######################################
# Run command capturing output to temp log
# Arguments:
#   $@: command with arguments
# Returns:
#   0 on success, 1 otherwise (log saved in LAST_CMD_LOG)
#######################################
run_cmd() {
    local log_file
    log_file=$(mktemp -p "${LOG_DIR}" pkgly-cmd-XXXX.log)
    LAST_CMD_LOG="$log_file"
    echo -e "\n    $@"
    if "$@" >"$log_file" 2>&1; then
        clear_last_log
        return 0
    else
        return 1
    fi
}

#######################################
# Record arbitrary output for next failure message
# Arguments:
#   $1: string to record
#######################################
record_output() {
    local content="$1"
    clear_last_log
    local log_file
    log_file=$(mktemp -p "${LOG_DIR}" pkgly-output-XXXX.log)
    printf "%s" "$content" >"$log_file"
    LAST_CMD_LOG="$log_file"
}

#######################################
# Clear and delete the last command log
#######################################
clear_last_log() {
    if [[ -n "$LAST_CMD_LOG" ]]; then
        if [ -f "$LAST_CMD_LOG" ]; then
            rm -f "$LAST_CMD_LOG"
        fi
        LAST_CMD_LOG=""
    fi
}


#######################################
# Wait for server to be healthy
# Arguments:
#   $1: Maximum wait time in seconds (default: 60)
# Returns:
#   0 on success, 1 on timeout
#######################################
wait_for_server() {
    local max_wait="${1:-60}"
    local elapsed=0

    print_color "$YELLOW" "Waiting for Pkgly server at $PKGLY_URL..."

    while [ $elapsed -lt $max_wait ]; do
        if curl -sf "${PKGLY_URL}/" > /dev/null 2>&1; then
            print_color "$GREEN" "Server is ready!"
            return 0
        fi
        sleep 1
        ((elapsed+=1))
        if [ $elapsed -gt 0 ] && [ $((elapsed % 10)) -eq 0 ]; then
            echo "  Still waiting... (${elapsed}s / ${max_wait}s)"
        fi
    done

    print_color "$RED" "Server did not become ready within ${max_wait} seconds"
    return 1
}

#######################################
# Assert HTTP status code
# Arguments:
#   $1: Expected status code
#   $2: Actual status code
#   $3: Optional message
# Returns:
#   0 if match, 1 if mismatch
#######################################
assert_http_status() {
    local expected="$1"
    local actual="$2"
    local message="${3:-Expected status $expected, got $actual}"

    if [ "$expected" = "$actual" ]; then
        return 0
    else
        echo "$message" >&2
        return 1
    fi
}

#######################################
# Assert string contains substring
# Arguments:
#   $1: Haystack
#   $2: Needle
#   $3: Optional message
# Returns:
#   0 if found, 1 if not found
#######################################
assert_contains() {
    local haystack="$1"
    local needle="$2"
    local message="${3:-Expected to find '$needle' in output}"

    if echo "$haystack" | grep -q "$needle"; then
        return 0
    else
        echo "$message" >&2
        return 1
    fi
}

#######################################
# Assert file exists
# Arguments:
#   $1: File path
#   $2: Optional message
# Returns:
#   0 if exists, 1 if not
#######################################
assert_file_exists() {
    local file="$1"
    local message="${2:-Expected file to exist: $file}"

    if [ -f "$file" ]; then
        return 0
    else
        echo "$message" >&2
        return 1
    fi
}

#######################################
# Get authorization header
# Arguments:
#   None
# Returns:
#   Authorization header string
#######################################
get_auth_header() {
    echo "Authorization: Bearer ${TEST_TOKEN}"
}

#######################################
# Make authenticated GET request
# Arguments:
#   $1: URL path (relative to PKGLY_URL)
#   $2+: Additional curl arguments
# Returns:
#   HTTP response
#######################################
api_get() {
    local path="$1"
    shift
    curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${path}" "$@"
}

#######################################
# Make authenticated PUT request
# Arguments:
#   $1: URL path (relative to PKGLY_URL)
#   $2+: Additional curl arguments
# Returns:
#   HTTP response
#######################################
api_put() {
    local path="$1"
    shift
    curl -sf -X PUT -H "$(get_auth_header)" "${PKGLY_URL}${path}" "$@"
}

#######################################
# Make authenticated POST request
# Arguments:
#   $1: URL path (relative to PKGLY_URL)
#   $2+: Additional curl arguments
# Returns:
#   HTTP response
#######################################
api_post() {
    local path="$1"
    shift
    curl -sf -X POST -H "$(get_auth_header)" "${PKGLY_URL}${path}" "$@"
}

#######################################
# Make authenticated DELETE request
# Arguments:
#   $1: URL path (relative to PKGLY_URL)
#   $2+: Additional curl arguments
# Returns:
#   HTTP response
#######################################
api_delete() {
    local path="$1"
    shift
    curl -sf -X DELETE -H "$(get_auth_header)" "${PKGLY_URL}${path}" "$@"
}

#######################################
# Get HTTP status code from response
# Arguments:
#   $1: URL
#   $2+: Additional curl arguments
# Returns:
#   HTTP status code
#######################################
get_http_status() {
    local url="$1"
    shift
    curl -s -o /dev/null -w "%{http_code}" "$url" "$@"
}

#######################################
# Clean up test workspace
# Arguments:
#   $1: Workspace directory
#######################################
cleanup_workspace() {
    local workspace="$1"
    if [ -d "$workspace" ]; then
        rm -rf "$workspace"
    fi
}

#######################################
# Create temporary workspace
# Arguments:
#   $1: Base name
# Returns:
#   Path to workspace directory
#######################################
create_workspace() {
    local base_name="$1"
    local workspace="/tmp/pkgly-test-${base_name}-$$"
    mkdir -p "$workspace"
    echo "$workspace"
}

#######################################
# Print test summary
# Arguments:
#   None
#######################################
print_summary() {
    echo ""
    print_color "$BLUE" "============================================"
    print_color "$BLUE" " Test Summary"
    print_color "$BLUE" "============================================"
    echo "  Tests run:    $TESTS_RUN"
    print_color "$GREEN" "  Tests passed: $TESTS_PASSED"
    if [ $TESTS_FAILED -gt 0 ]; then
        print_color "$RED" "  Tests failed: $TESTS_FAILED"
    else
        echo "  Tests failed: $TESTS_FAILED"
    fi
    echo ""

    if [ $TESTS_FAILED -eq 0 ]; then
        print_color "$GREEN" "✓ ALL TESTS PASSED"
        return 0
    else
        print_color "$RED" "✗ SOME TESTS FAILED"
        return 1
    fi
}

#######################################
# Verify repository exists
# Arguments:
#   $1: Repository name
# Returns:
#   0 if exists, 1 if not
#######################################
verify_repository_exists() {
    local repo_name="$1"

    local repos
    repos=$(api_get "/api/repository/list" || echo "[]")
    record_output "$repos"
    if jq -e ".[] | select(.name == \"$repo_name\")" <<<"$repos" > /dev/null 2>&1; then
        clear_last_log
        return 0
    else
        return 1
    fi
}

#######################################
# Generate random string
# Arguments:
#   $1: Length (default: 8)
# Returns:
#   Random string
#######################################
random_string() {
    local length="${1:-8}"
    python3 - <<PY
import random
alphabet = "abcdefghijklmnopqrstuvwxyz0123456789"
print("".join(random.choice(alphabet) for _ in range($length)))
PY
}

# Export functions for use in test scripts
export -f print_color
export -f print_section
export -f print_test
export -f pass
export -f fail
export -f wait_for_server
export -f assert_http_status
export -f assert_contains
export -f assert_file_exists
export -f get_auth_header
export -f api_get
export -f api_put
export -f api_post
export -f api_delete
export -f get_http_status
export -f cleanup_workspace
export -f create_workspace
export -f print_summary
export -f verify_repository_exists
export -f random_string
export -f run_cmd
export -f record_output
export -f clear_last_log
