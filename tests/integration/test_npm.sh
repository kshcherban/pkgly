#!/bin/bash
# NPM integration tests
# Tests hosted and proxy NPM repositories end-to-end

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# NPM-specific configuration
NPM_HOSTED_REPO="${TEST_STORAGE}/npm-hosted"
NPM_PROXY_REPO="${TEST_STORAGE}/npm-proxy"
NPM_VIRTUAL_REPO="${TEST_STORAGE}/npm-virtual"
FIXTURE_DIR="/fixtures/npm/hello-pkg"
PACKAGE_NAME="@pkgly-test/hello-pkg"
VERSION_1="1.0.0"
VERSION_2="1.0.1"
PKGLY_REGISTRY_HOST="${PKGLY_URL#*://}"

print_section "NPM Integration Tests"

# Setup workspace
WORKSPACE=$(create_workspace "npm")
cd "$WORKSPACE"

# Copy fixture and create tarball
cp -r "$FIXTURE_DIR" "$WORKSPACE/hello-pkg"
cd "$WORKSPACE/hello-pkg"

print_test "Create NPM package tarball"
# Test 1: Create package tarball
if run_cmd npm pack; then
    TARBALL=$(ls pkgly-test-hello-pkg-*.tgz)
    if [ -f "$TARBALL" ]; then
        pass
    else
        fail "Tarball not created"
        cleanup_workspace "$WORKSPACE"
        exit 1
    fi
else
    fail "npm pack failed"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

# Test 2: Publish package to hosted repository
print_test "Publish package to npm-hosted"

# Create .npmrc for authentication - use bearer token for NPM
cat > "$WORKSPACE/hello-pkg/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

cd "$WORKSPACE/hello-pkg"
if run_cmd npm publish --registry="${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/"; then
    pass
else
    fail "npm publish failed"
fi

# Test 3: Verify package metadata available
print_test "Fetch package metadata"
ENCODED_NAME=$(echo "$PACKAGE_NAME" | sed 's/@/%40/g; s/\//%2F/g')
METADATA_PATH="/repositories/${NPM_HOSTED_REPO}/${ENCODED_NAME}"

METADATA=$(curl -sf "${PKGLY_URL}${METADATA_PATH}" || echo "")
record_output "$METADATA"

set +e
jq -e '.name == "@pkgly-test/hello-pkg"' <<<"$METADATA" > /dev/null 2>&1
metadata_status=$?
set -e

if [ "$metadata_status" -eq 0 ]; then
    clear_last_log
    pass
else
    fail "Package metadata not available or invalid"
fi

# Test 4: Install package from hosted repository
print_test "Install package from npm-hosted"
INSTALL_DIR="$WORKSPACE/install-test"
mkdir -p "$INSTALL_DIR"
cd "$INSTALL_DIR"

cat > "$INSTALL_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@${VERSION_1}" && \
   [ -d "node_modules/@pkgly-test/hello-pkg" ]; then
    pass
else
    fail "Failed to install package"
fi

# Test 5: Verify installed package works
print_test "Verify installed package functionality"
cat > "$INSTALL_DIR/test.js" <<EOF
const pkg = require('@pkgly-test/hello-pkg');
console.log(pkg.greet('World'));
console.log(pkg.getVersion());
EOF

OUTPUT=$(node test.js)
if echo "$OUTPUT" | grep -q "Hello, World!" && \
   echo "$OUTPUT" | grep -q "1.0.0"; then
    pass
else
    fail "Package not functioning correctly"
fi

# Test 6: Publish second version
print_test "Publish second version (${VERSION_2})"
cd "$WORKSPACE/hello-pkg"

# Update package.json version
jq ".version = \"${VERSION_2}\"" package.json > package.json.tmp && mv package.json.tmp package.json

if run_cmd npm publish --registry="${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/"; then
    pass
else
    fail "Failed to publish second version"
fi

# Test 7: Verify both versions exist
print_test "Verify both versions accessible"
METADATA=$(curl -sf "${PKGLY_URL}${METADATA_PATH}" || echo "")
record_output "$METADATA"

set +e
jq -e ".versions | has(\"${VERSION_1}\") and has(\"${VERSION_2}\")" <<<"$METADATA" > /dev/null 2>&1
metadata_versions_status=$?
set -e

if [ "$metadata_versions_status" -eq 0 ]; then
    clear_last_log
    pass
else
    fail "Both versions not in metadata"
fi

# Test 8: Install specific version
print_test "Install specific version (${VERSION_1})"
INSTALL_DIR_V1="$WORKSPACE/install-v1"
mkdir -p "$INSTALL_DIR_V1"
cd "$INSTALL_DIR_V1"

cat > "$INSTALL_DIR_V1/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@${VERSION_1}"; then
    INSTALLED_VERSION=$(jq -r .version node_modules/@pkgly-test/hello-pkg/package.json)
    if [ "$INSTALLED_VERSION" = "$VERSION_1" ]; then
        pass
    else
        fail "Wrong version installed: $INSTALLED_VERSION"
    fi
else
    fail "Failed to install specific version"
fi

# Test 9: Install latest version
print_test "Install latest version"
INSTALL_DIR_LATEST="$WORKSPACE/install-latest"
mkdir -p "$INSTALL_DIR_LATEST"
cd "$INSTALL_DIR_LATEST"

cat > "$INSTALL_DIR_LATEST/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@latest"; then
    INSTALLED_VERSION=$(jq -r .version node_modules/@pkgly-test/hello-pkg/package.json)
    if [ "$INSTALLED_VERSION" = "$VERSION_2" ]; then
        pass
    else
        fail "Latest version not installed: $INSTALLED_VERSION"
    fi
else
    fail "Failed to install latest version"
fi

# Test 10: Proxy repository - fetch from npmjs.org
print_test "Proxy: fetch lodash from npmjs.org"
PROXY_INSTALL_DIR="$WORKSPACE/proxy-test"
mkdir -p "$PROXY_INSTALL_DIR"
cd "$PROXY_INSTALL_DIR"

cat > "$PROXY_INSTALL_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_PROXY_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_PROXY_REPO}/
always-auth=true
EOF

if run_cmd npm install lodash@4.17.21 && \
   [ -d "node_modules/lodash" ]; then
    pass
else
    fail "Failed to proxy package from npmjs.org"
fi

# Test 11: Proxy caching verification
print_test "Proxy: verify package is cached"
PROXY_INSTALL_DIR2="$WORKSPACE/proxy-test2"
mkdir -p "$PROXY_INSTALL_DIR2"
cd "$PROXY_INSTALL_DIR2"

cat > "$PROXY_INSTALL_DIR2/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_PROXY_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_PROXY_REPO}/
always-auth=true
EOF

if run_cmd npm install lodash@4.17.21 && \
   [ -d "node_modules/lodash" ]; then
    pass
else
    fail "Failed to retrieve cached package"
fi

# Virtual repository setup
print_test "Ensure npm-virtual repository exists with hosted+proxy members"
REPO_LIST=$(api_get "/api/repository/list" || echo "[]")
record_output "$REPO_LIST"
HOSTED_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-hosted") | .id')
PROXY_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-proxy") | .id')
VIRTUAL_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-virtual") | .id')
STORAGE_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-hosted") | .storage_id')

if [ -z "$HOSTED_ID" ] || [ -z "$PROXY_ID" ] || [ "$HOSTED_ID" = "null" ] || [ "$PROXY_ID" = "null" ]; then
    fail "Hosted or proxy repository missing"
fi

if [ -z "$VIRTUAL_ID" ] || [ "$VIRTUAL_ID" = "null" ]; then
    VIRTUAL_PAYLOAD=$(cat <<JSON
{
  "name": "npm-virtual",
  "storage": "$STORAGE_ID",
  "configs": {
    "npm": {
      "type": "Virtual",
      "config": {
        "member_repositories": [
          {"repository_id": "$HOSTED_ID", "repository_name": "npm-hosted", "priority": 1, "enabled": true},
          {"repository_id": "$PROXY_ID", "repository_name": "npm-proxy", "priority": 10, "enabled": true}
        ],
        "resolution_order": "Priority"
      }
    },
    "auth": {"enabled": false}
  }
}
JSON
)
    CREATE_RESPONSE=$(api_post "/api/repository/new/npm" -H "Content-Type: application/json" -d "$VIRTUAL_PAYLOAD" || echo "")
    record_output "$CREATE_RESPONSE"
    VIRTUAL_ID=$(echo "$CREATE_RESPONSE" | jq -r '.id')
fi

if [ -z "$VIRTUAL_ID" ] || [ "$VIRTUAL_ID" = "null" ]; then
    fail "Failed to create or resolve npm-virtual repository"
fi

MEMBERS=$(api_get "/api/repository/${VIRTUAL_ID}/virtual/members" || echo "[]")
record_output "$MEMBERS"
MEMBER_COUNT=$(echo "$MEMBERS" | jq 'length')
if [ "$MEMBER_COUNT" -ge 2 ]; then
    clear_last_log
    pass
else
    fail "Virtual members not configured"
fi

# Test virtual repository resolves hosted packages
print_test "Virtual: install hosted package via npm-virtual"
VIRTUAL_INSTALL_DIR="$WORKSPACE/virtual-hosted"
mkdir -p "$VIRTUAL_INSTALL_DIR"
cd "$VIRTUAL_INSTALL_DIR"

cat > "$VIRTUAL_INSTALL_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_VIRTUAL_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_VIRTUAL_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@${VERSION_2}" && \
   [ -d "node_modules/@pkgly-test/hello-pkg" ]; then
    INSTALLED_VERSION=$(jq -r .version node_modules/@pkgly-test/hello-pkg/package.json)
    if [ "$INSTALLED_VERSION" = "$VERSION_2" ]; then
        pass
    else
        fail "Virtual repo installed wrong version: $INSTALLED_VERSION"
    fi
else
    fail "Virtual repo failed to install hosted package"
fi

# Test virtual repository resolves proxy packages
print_test "Virtual: install proxied package via npm-virtual"
VIRTUAL_PROXY_DIR="$WORKSPACE/virtual-proxy"
mkdir -p "$VIRTUAL_PROXY_DIR"
cd "$VIRTUAL_PROXY_DIR"

cat > "$VIRTUAL_PROXY_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_VIRTUAL_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_VIRTUAL_REPO}/
always-auth=true
EOF

if run_cmd npm install lodash@4.17.21 && \
   [ -d "node_modules/lodash" ]; then
    pass
else
    fail "Virtual repo failed to proxy lodash"
fi

# Test 12: Authentication required for publish
print_test "Verify authentication required for publish"
cd "$WORKSPACE/hello-pkg"

# Try to publish without auth
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT \
    "${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/${ENCODED_NAME}" \
    -H "Content-Type: application/json" \
    --data '{}')

if [ "$STATUS" = "401" ] || [ "$STATUS" = "403" ]; then
    pass
else
    fail "Expected 401/403 without auth, got $STATUS"
fi

# Test 13: Not found for non-existent package
print_test "Verify 404 for non-existent package"
NONEXISTENT_PATH="/repositories/${NPM_HOSTED_REPO}/@nonexistent/package-does-not-exist"

STATUS=$(get_http_status "${PKGLY_URL}${NONEXISTENT_PATH}" -H "$(get_auth_header)")

if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404, got $STATUS"
fi

# Test 14: NPM scoped package support
print_test "Verify scoped package support"
record_output "$METADATA"
set +e
jq -e '.name | startswith("@")' <<<"$METADATA" > /dev/null 2>&1
metadata_scope_status=$?
set -e
if [ "$metadata_scope_status" -eq 0 ]; then
    clear_last_log
    pass
else
    fail "Scoped package not properly supported"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"

print_summary
