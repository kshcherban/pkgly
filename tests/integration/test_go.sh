#!/bin/bash
# Go integration tests
# Tests hosted and proxy Go repositories end-to-end

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# Go-specific configuration
GO_HOSTED_REPO="${TEST_STORAGE}/go-hosted"
GO_PROXY_REPO="${TEST_STORAGE}/go-proxy"
FIXTURE_DIR="/fixtures/go/test-module"
MODULE_NAME="pkgly.test/test-module"
VERSION_1="v1.0.0"
VERSION_2="v1.0.1"

print_section "Go Integration Tests"

WORKSPACE=$(create_workspace "go")
cd "$WORKSPACE"

HOST_NO_SCHEME="${PKGLY_URL#http://}"
HOST_NO_SCHEME="${HOST_NO_SCHEME#https://}"

# Setup Go environment
export GOPATH="$WORKSPACE/gopath"
export GOPROXY="direct"
export GOSUMDB="off"
export GONOSUMDB="*"
export GOINSECURE="$HOST_NO_SCHEME"
export GOFLAGS="-mod=mod"
mkdir -p "$GOPATH"

cat > "$HOME/.netrc" <<EOF
machine $HOST_NO_SCHEME
  login ${TEST_USER}
  password ${TEST_PASSWORD}
EOF
chmod 600 "$HOME/.netrc"

# Copy fixture
cp -r "$FIXTURE_DIR" "$WORKSPACE/test-module"

create_module_zip() {
    local version="$1"
    local staging_root="$WORKSPACE/pkg"
    local staging_dir="${staging_root}/${MODULE_NAME}@${version}"
    mkdir -p "$staging_root"
    rm -rf "$staging_dir"
    mkdir -p "$staging_dir"
    cp -R "$WORKSPACE/test-module/." "$staging_dir/"
    (cd "$staging_root" && run_cmd zip -qr "$WORKSPACE/${MODULE_NAME//\//-}-${version}.zip" "${MODULE_NAME}@${version}") || return 1
}

# Test 1: Create Go module archive
print_test "Create Go module archive (${VERSION_1})"
if create_module_zip "${VERSION_1}"; then
    pass
else
    fail "Failed to create module archive"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

# Test 2: Upload module to hosted repository
print_test "Upload module to go-hosted"
MODULE_PATH="${MODULE_NAME}/@v/${VERSION_1}.zip"
UPLOAD_PATH="/repositories/${GO_HOSTED_REPO}/${MODULE_PATH}"

STATUS=$(get_http_status "${PKGLY_URL}${UPLOAD_PATH}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/${MODULE_NAME//\//-}-${VERSION_1}.zip")

if assert_http_status "201" "$STATUS"; then
    pass
else
    fail "Expected 201, got $STATUS"
fi

# Test 3: Upload .info file
print_test "Upload .info file"
cat > "$WORKSPACE/${VERSION_1}.info" <<EOF
{
  "Version": "${VERSION_1}",
  "Time": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

INFO_PATH="/repositories/${GO_HOSTED_REPO}/${MODULE_NAME}/@v/${VERSION_1}.info"

STATUS=$(get_http_status "${PKGLY_URL}${INFO_PATH}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/${VERSION_1}.info")

if assert_http_status "201" "$STATUS"; then
    pass
else
    fail "Expected 201, got $STATUS"
fi

# Test 4: Upload .mod file
print_test "Upload .mod file"
MOD_PATH="/repositories/${GO_HOSTED_REPO}/${MODULE_NAME}/@v/${VERSION_1}.mod"

STATUS=$(get_http_status "${PKGLY_URL}${MOD_PATH}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/test-module/go.mod")

if assert_http_status "201" "$STATUS"; then
    pass
else
    fail "Expected 201, got $STATUS"
fi

# Test 5: Fetch version list
print_test "Fetch version list"
LIST_PATH="/repositories/${GO_HOSTED_REPO}/${MODULE_NAME}/@v/list"

VERSIONS=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${LIST_PATH}" || echo "")
record_output "$VERSIONS"

if echo "$VERSIONS" | grep -q "${VERSION_1}"; then
    clear_last_log
    pass
else
    fail "Version not in list"
fi

# Test 6: Download .info file
print_test "Download .info file"
if curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${INFO_PATH}" -o "$WORKSPACE/downloaded.info" && \
   assert_file_exists "$WORKSPACE/downloaded.info"; then
    pass
else
    fail "Failed to download .info file"
fi

# Test 7: Download .mod file
print_test "Download .mod file"
if curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${MOD_PATH}" -o "$WORKSPACE/downloaded.mod" && \
   assert_file_exists "$WORKSPACE/downloaded.mod"; then
    pass
else
    fail "Failed to download .mod file"
fi

# Test 8: Download .zip file
print_test "Download .zip file"
if curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${UPLOAD_PATH}" -o "$WORKSPACE/downloaded.zip" && \
   assert_file_exists "$WORKSPACE/downloaded.zip"; then
    pass
else
    fail "Failed to download .zip file"
fi

# Test 9: Use module with GOPROXY
print_test "Install module via GOPROXY"
CONSUMER_DIR="$WORKSPACE/consumer"
mkdir -p "$CONSUMER_DIR"
cd "$CONSUMER_DIR"

export GOPROXY="${PKGLY_URL}/repositories/${GO_HOSTED_REPO}"
export GOSUMDB="off"

cat > "$CONSUMER_DIR/go.mod" <<EOF
module test-consumer

go 1.21

require ${MODULE_NAME} ${VERSION_1}
EOF

cat > "$CONSUMER_DIR/main.go" <<EOF
package main

import (
    "fmt"
    testmodule "${MODULE_NAME}"
)

func main() {
    fmt.Println(testmodule.Greet("World"))
    fmt.Println(testmodule.GetVersion())
}
EOF

if run_cmd go mod download; then
    pass
else
    fail "Failed to download module via GOPROXY"
fi

# Test 10: Build and run consumer
print_test "Build and run consumer application"
if run_cmd go build -o consumer && [ -f "consumer" ]; then
    OUTPUT=$(./consumer)
    record_output "$OUTPUT"
    if echo "$OUTPUT" | grep -q "Hello, World!" && echo "$OUTPUT" | grep -q "${VERSION_1}"; then
        clear_last_log
        pass
    else
        fail "Consumer output incorrect"
    fi
else
    fail "Failed to build consumer"
fi

# Test 11: Upload second version
print_test "Upload second version (${VERSION_2})"
cd "$WORKSPACE/test-module"

# Update version in code
sed -i "s/${VERSION_1}/${VERSION_2}/g" greeter.go

if ! create_module_zip "${VERSION_2}"; then
    fail "Failed to create module archive for ${VERSION_2}"
fi

UPLOAD_PATH_V2="/repositories/${GO_HOSTED_REPO}/${MODULE_NAME}/@v/${VERSION_2}.zip"

STATUS=$(get_http_status "${PKGLY_URL}${UPLOAD_PATH_V2}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/${MODULE_NAME//\//-}-${VERSION_2}.zip")

if assert_http_status "201" "$STATUS"; then
    pass
else
    fail "Expected 201, got $STATUS"
fi

# Upload v2 .info
cat > "$WORKSPACE/${VERSION_2}.info" <<EOF
{
  "Version": "${VERSION_2}",
  "Time": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
INFO_PATH_V2="/repositories/${GO_HOSTED_REPO}/${MODULE_NAME}/@v/${VERSION_2}.info"
STATUS=$(get_http_status "${PKGLY_URL}${INFO_PATH_V2}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/${VERSION_2}.info")
if assert_http_status "201" "$STATUS"; then
    :
else
    fail "Expected 201 for info v2, got $STATUS"
fi

# Upload v2 .mod
MOD_PATH_V2="/repositories/${GO_HOSTED_REPO}/${MODULE_NAME}/@v/${VERSION_2}.mod"
STATUS=$(get_http_status "${PKGLY_URL}${MOD_PATH_V2}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/test-module/go.mod")
if assert_http_status "201" "$STATUS"; then
    :
else
    fail "Expected 201 for mod v2, got $STATUS"
fi

# Test 12: Verify both versions in list
print_test "Verify both versions in list"
VERSIONS=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${LIST_PATH}" || echo "")
record_output "$VERSIONS"

if echo "$VERSIONS" | grep -q "${VERSION_1}" && echo "$VERSIONS" | grep -q "${VERSION_2}"; then
    clear_last_log
    pass
else
    fail "Both versions not in list"
fi

# Test 13: Proxy repository - fetch from proxy.golang.org
print_test "Proxy: fetch module from proxy.golang.org"
CONSUMER_PROXY_DIR="$WORKSPACE/consumer-proxy"
mkdir -p "$CONSUMER_PROXY_DIR"
cd "$CONSUMER_PROXY_DIR"

export GOPROXY="${PKGLY_URL}/repositories/${GO_PROXY_REPO}"
export GOSUMDB="off"

cat > "$CONSUMER_PROXY_DIR/go.mod" <<EOF
module test-consumer-proxy

go 1.21

require github.com/fatih/color v1.16.0
EOF

if run_cmd go mod download; then
    pass
else
    fail "Failed to proxy module from golang.org"
fi

# Test 14: Proxy caching verification
print_test "Proxy: verify module is cached"
CONSUMER_PROXY_DIR2="$WORKSPACE/consumer-proxy2"
mkdir -p "$CONSUMER_PROXY_DIR2"
cd "$CONSUMER_PROXY_DIR2"

cat > "$CONSUMER_PROXY_DIR2/go.mod" <<EOF
module test-consumer-proxy2

go 1.21

require github.com/fatih/color v1.16.0
EOF

if run_cmd go mod download; then
    pass
else
    fail "Failed to retrieve cached module"
fi

# Test 15: Authentication required for upload
print_test "Verify authentication required for upload"
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT \
    "${PKGLY_URL}${UPLOAD_PATH}" \
    --data '{}')

if assert_http_status "401" "$STATUS"; then
    pass
else
    fail "Expected 401 without auth, got $STATUS"
fi

# Test 16: Not found for non-existent module
print_test "Verify 404 for non-existent module"
NONEXISTENT_PATH="/repositories/${GO_HOSTED_REPO}/nonexistent.test/module/@v/v1.0.0.zip"

STATUS=$(get_http_status "${PKGLY_URL}${NONEXISTENT_PATH}")

if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404, got $STATUS"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"

print_summary
