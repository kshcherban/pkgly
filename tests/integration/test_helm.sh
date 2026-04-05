#!/bin/bash
# Helm integration tests
# Tests hosted Helm repositories (HTTP and OCI) end-to-end

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"
export HELM_EXPERIMENTAL_OCI=1

# Helm-specific configuration
HELM_HOSTED_REPO="${TEST_STORAGE}/helm-hosted"
HELM_OCI_REPO="${TEST_STORAGE}/helm-oci"
FIXTURE_DIR="/fixtures/helm/test-chart"
CHART_NAME="test-chart"
VERSION_1="1.0.0"
VERSION_2="1.0.1"

if [[ "${PKGLY_URL}" == *"://"* ]]; then
    PKGLY_REGISTRY_HOST="${PKGLY_URL#*://}"
else
    PKGLY_REGISTRY_HOST="${PKGLY_URL}"
fi
PKGLY_REGISTRY_HOST="${PKGLY_REGISTRY_HOST%%/*}"
PKGLY_REGISTRY_HOST="${PKGLY_REGISTRY_HOST%/}"
HELM_OCI_REPO_URL="oci://${PKGLY_REGISTRY_HOST}/${HELM_OCI_REPO}"
HELM_OCI_CHART_REF="${HELM_OCI_REPO_URL}/${CHART_NAME}"

print_section "Helm Integration Tests"

WORKSPACE=$(create_workspace "helm")
cd "$WORKSPACE"

# Copy fixture
cp -r "$FIXTURE_DIR" "$WORKSPACE/test-chart"
cd "$WORKSPACE"

# Test 1: Package Helm chart
print_test "Package Helm chart (${VERSION_1})"
if run_cmd helm package test-chart; then
    CHART_PACKAGE="${CHART_NAME}-${VERSION_1}.tgz"
    if assert_file_exists "$CHART_PACKAGE"; then
        pass
    else
        fail "Chart package not created"
        cleanup_workspace "$WORKSPACE"
        exit 1
    fi
else
    fail "Failed to package chart"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

# Test 2: Upload chart via HTTP PUT
print_test "Upload chart via HTTP PUT"
UPLOAD_PATH="/repositories/${HELM_HOSTED_REPO}/${CHART_PACKAGE}"

STATUS=$(get_http_status "${PKGLY_URL}${UPLOAD_PATH}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${CHART_PACKAGE}")

if [ "$STATUS" = "201" ] || [ "$STATUS" = "204" ]; then
    pass
else
    fail "Expected 201/204, got $STATUS"
fi

# Test 3: Upload chart via ChartMuseum API
print_test "Upload chart via ChartMuseum API (second version)"

# Update chart version
cd test-chart
sed -i "s/version: ${VERSION_1}/version: ${VERSION_2}/" Chart.yaml
cd ..

if ! run_cmd helm package test-chart; then
    fail "Failed to package chart ${VERSION_2}"
fi
CHART_PACKAGE_V2="${CHART_NAME}-${VERSION_2}.tgz"

STATUS=$(get_http_status "${PKGLY_URL}/repositories/${HELM_HOSTED_REPO}/api/charts" \
    -X POST \
    -H "$(get_auth_header)" \
    -F "chart=@${CHART_PACKAGE_V2}")

if [ "$STATUS" = "201" ] || [ "$STATUS" = "200" ] || [ "$STATUS" = "204" ]; then
    pass
else
    fail "Expected 201/200/204, got $STATUS"
fi

# Test 4: Fetch index.yaml
print_test "Fetch index.yaml"
INDEX_PATH="/repositories/${HELM_HOSTED_REPO}/index.yaml"

INDEX_CONTENT=$(curl -sf "${PKGLY_URL}${INDEX_PATH}" || echo "")
record_output "$INDEX_CONTENT"

if echo "$INDEX_CONTENT" | grep -q "apiVersion: v1" && \
   echo "$INDEX_CONTENT" | grep -q "${CHART_NAME}"; then
    clear_last_log
    pass
else
    fail "index.yaml not generated correctly"
fi

# Test 5: Verify both versions in index
print_test "Verify both versions in index.yaml"

if echo "$INDEX_CONTENT" | grep -q "${VERSION_1}" && \
   echo "$INDEX_CONTENT" | grep -q "${VERSION_2}"; then
    clear_last_log
    pass
else
    fail "Both versions not in index"
fi

# Test 6: Add Helm repository
print_test "Add Helm repository"
REPO_NAME="pkgly-test-$(random_string 6)"

if run_cmd helm repo add "$REPO_NAME" "${PKGLY_URL}/repositories/${HELM_HOSTED_REPO}"; then
    pass
else
    fail "Failed to add Helm repository"
fi

# Test 7: Update Helm repository
print_test "Update Helm repository"
if run_cmd helm repo update; then
    pass
else
    fail "Failed to update Helm repository"
fi

# Test 8: Search for chart
print_test "Search for chart in repository"
SEARCH_RESULT=$(helm search repo "$REPO_NAME/${CHART_NAME}" || echo "")
record_output "$SEARCH_RESULT"

if echo "$SEARCH_RESULT" | grep -q "${CHART_NAME}"; then
    clear_last_log
    pass
else
    fail "Chart not found in search results"
fi

# Test 9: Pull chart
print_test "Pull chart from repository"
PULL_DIR="$WORKSPACE/pulled"
mkdir -p "$PULL_DIR"
cd "$PULL_DIR"

if run_cmd helm pull "$REPO_NAME/${CHART_NAME}" --version "${VERSION_1}" && \
   assert_file_exists "${CHART_NAME}-${VERSION_1}.tgz"; then
    pass
else
    fail "Failed to pull chart"
fi

# Test 10: Verify pulled chart integrity
print_test "Verify pulled chart integrity"
ORIGINAL_HASH=$(sha256sum "$WORKSPACE/${CHART_NAME}-${VERSION_1}.tgz" | cut -d' ' -f1)
PULLED_HASH=$(sha256sum "${CHART_NAME}-${VERSION_1}.tgz" | cut -d' ' -f1)

if [ "$ORIGINAL_HASH" = "$PULLED_HASH" ]; then
    pass
else
    fail "Hash mismatch: original=$ORIGINAL_HASH, pulled=$PULLED_HASH"
fi

# Test 11: Pull latest version
print_test "Pull latest version"
cd "$WORKSPACE"
PULL_DIR_LATEST="$WORKSPACE/pulled-latest"
mkdir -p "$PULL_DIR_LATEST"
cd "$PULL_DIR_LATEST"

if run_cmd helm pull "$REPO_NAME/${CHART_NAME}" && \
   assert_file_exists "${CHART_NAME}-${VERSION_2}.tgz"; then
    pass
else
    fail "Failed to pull latest version"
fi

# Test 12: Download chart via direct URL
print_test "Download chart via direct URL"
DOWNLOAD_PATH="/repositories/${HELM_HOSTED_REPO}/${CHART_NAME}-${VERSION_1}.tgz"

if curl -sf "${PKGLY_URL}${DOWNLOAD_PATH}" -o "$WORKSPACE/downloaded.tgz" && \
   assert_file_exists "$WORKSPACE/downloaded.tgz"; then
    pass
else
    fail "Failed to download chart via URL"
fi

# Test 13: ChartMuseum health check
print_test "ChartMuseum health endpoint"
HEALTH_PATH="/repositories/${HELM_HOSTED_REPO}/health"

STATUS=$(get_http_status "${PKGLY_URL}${HEALTH_PATH}")

if [ "$STATUS" = "200" ] || [ "$STATUS" = "404" ]; then
    # 404 is acceptable if health endpoint not implemented
    pass
else
    fail "Unexpected status: $STATUS"
fi

# Test 14: Authentication required for upload
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

# Test 15: Not found for non-existent chart
print_test "Verify 404 for non-existent chart"
NONEXISTENT_PATH="/repositories/${HELM_HOSTED_REPO}/nonexistent-chart-1.0.0.tgz"

STATUS=$(get_http_status "${PKGLY_URL}${NONEXISTENT_PATH}")

if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404, got $STATUS"
fi

# Test 16: Delete chart (if supported)
print_test "Delete chart via ChartMuseum API"
DELETE_PATH="/repositories/${HELM_HOSTED_REPO}/api/charts/${CHART_NAME}/${VERSION_1}"

STATUS=$(get_http_status "${PKGLY_URL}${DELETE_PATH}" \
    -X DELETE \
    -H "$(get_auth_header)")

if [ "$STATUS" = "200" ] || [ "$STATUS" = "204" ] || [ "$STATUS" = "404" ]; then
    # 404 is acceptable if delete not implemented
    pass
else
    fail "Unexpected status for delete: $STATUS"
fi

# Test 17: Validate Chart.yaml apiVersion from HTTP pull
print_test "Validate Chart.yaml apiVersion (HTTP repository)"
if CHART_METADATA=$(tar -Oxzf "${PULL_DIR}/${CHART_NAME}-${VERSION_1}.tgz" "${CHART_NAME}/Chart.yaml" 2>/dev/null); then
    record_output "$CHART_METADATA"
    if echo "$CHART_METADATA" | grep -q "apiVersion: v2"; then
        clear_last_log
        pass
    else
        fail "Chart.yaml does not declare apiVersion v2"
    fi
else
    fail "Failed to read Chart.yaml from HTTP chart archive"
fi

# Test 18: Login to Helm OCI registry
print_test "Login to Helm OCI registry"
if run_cmd helm registry login "$PKGLY_REGISTRY_HOST" \
        --username "$TEST_USER" \
        --password "$TEST_PASSWORD" \
        --plain-http; then
    pass
else
    fail "Failed to authenticate against Helm OCI registry"
fi

cd "$WORKSPACE"

# Test 19: Push chart v1 via OCI
print_test "Push chart v1 via OCI"
if run_cmd helm push "${WORKSPACE}/${CHART_PACKAGE}" "$HELM_OCI_REPO_URL" --plain-http; then
    pass
else
    fail "Failed to push chart v1 to OCI repository"
fi

# Test 20: Push chart v2 via OCI
print_test "Push chart v2 via OCI"
if run_cmd helm push "${WORKSPACE}/${CHART_PACKAGE_V2}" "$HELM_OCI_REPO_URL" --plain-http; then
    pass
else
    fail "Failed to push chart v2 to OCI repository"
fi

# Test 21: Pull chart v2 from OCI
print_test "Pull chart v2 via OCI"
OCI_PULL_DIR="$WORKSPACE/oci-pulled"
mkdir -p "$OCI_PULL_DIR"
cd "$OCI_PULL_DIR"
if run_cmd helm pull "$HELM_OCI_CHART_REF" --version "${VERSION_2}" --plain-http && \
   assert_file_exists "${CHART_NAME}-${VERSION_2}.tgz"; then
    pass
else
    fail "Failed to pull chart v2 from OCI repository"
fi

# Test 22: Validate Chart.yaml apiVersion from OCI pull
print_test "Validate Chart.yaml apiVersion (OCI repository)"
if OCI_CHART_METADATA=$(tar -Oxzf "${OCI_PULL_DIR}/${CHART_NAME}-${VERSION_2}.tgz" "${CHART_NAME}/Chart.yaml" 2>/dev/null); then
    record_output "$OCI_CHART_METADATA"
    if echo "$OCI_CHART_METADATA" | grep -q "apiVersion: v2"; then
        clear_last_log
        pass
    else
        fail "OCI Chart.yaml does not declare apiVersion v2"
    fi
else
    fail "Failed to read Chart.yaml from OCI chart archive"
fi

# Test 23: Logout from Helm OCI registry
print_test "Logout from Helm OCI registry"
if run_cmd helm registry logout "$PKGLY_REGISTRY_HOST"; then
    pass
else
    fail "Failed to logout from Helm OCI registry"
fi

# Cleanup
helm repo remove "$REPO_NAME" > /dev/null 2>&1 || true
cd "$SCRIPT_DIR"
cleanup_workspace "$WORKSPACE"

print_summary
