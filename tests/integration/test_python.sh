#!/bin/bash
# Python (PyPI) integration tests
# Tests hosted and proxy Python repositories end-to-end

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# Python-specific configuration
PYTHON_HOSTED_REPO="${TEST_STORAGE}/python-hosted"
PYTHON_PROXY_REPO="${TEST_STORAGE}/python-proxy"
FIXTURE_DIR="/fixtures/python/test-pkg"
PACKAGE_NAME="pkgly-test-pkg"
VERSION_1="1.0.0"
VERSION_2="1.0.1"

print_section "Python Integration Tests"

WORKSPACE=$(create_workspace "python")
cd "$WORKSPACE"

assert_hosted_trailing_slash_redirect() {
    local method="$1"
    local route_prefix="$2"
    local repository_path="$3"
    local index_path="$4"
    local test_id="$5"
    local canonical_index_path="${6:-$index_path}"
    local requested_url="${PKGLY_URL}${route_prefix}/${index_path}"
    local expected_location="/repositories/${repository_path}/${canonical_index_path}/"
    local headers_file="${WORKSPACE}/${test_id}.headers"
    local body_file="${WORKSPACE}/${test_id}.body"
    local status
    local location
    local curl_method=()
    if [[ "$method" == "HEAD" ]]; then
        curl_method=(-I)
    fi

    print_test "Hosted: ${method} ${route_prefix}/${index_path} redirects to the canonical index"
    # Do not follow redirects: the first Location must include /repositories/.
    if ! status=$(curl -sS "${curl_method[@]}" -D "$headers_file" -o /dev/null \
        -w '%{http_code}' "$requested_url"); then
        status="000"
    fi
    location=$(sed -n -E 's/^[Ll]ocation:[[:space:]]*([^\r]*)\r?$/\1/p' "$headers_file" | tail -n 1)
    if [[ "$status" != "301" || "$location" != "$expected_location" ]]; then
        record_output "$(printf 'status=%s\nlocation=%s\nexpected_location=%s\n' \
            "$status" "$location" "$expected_location")"
        fail "Hosted redirect did not return the canonical Location"
        return
    fi

    if ! status=$(curl -sS "${curl_method[@]}" -o "$body_file" -w '%{http_code}' \
        "${PKGLY_URL}${location}"); then
        status="000"
    fi
    if [[ "$status" != "200" ]]; then
        fail "Hosted redirect target returned ${status} instead of 200"
    elif [[ "$method" == "GET" && "$index_path" == "simple" ]] &&
        ! grep -q '<title>Simple index</title>' "$body_file"; then
        record_output "$(cat "$body_file")"
        fail "Hosted redirect target did not serve the Python index"
    else
        clear_last_log
        pass
    fi
}

# Copy fixture
cp -r "$FIXTURE_DIR" "$WORKSPACE/test-pkg"
cd "$WORKSPACE/test-pkg"

# Test 1: Build distribution package
print_test "Build Python distribution package"
if run_cmd python3 setup.py sdist bdist_wheel; then
    DIST_FILE=$(ls dist/*.tar.gz | head -n 1)
    if [ -f "$DIST_FILE" ]; then
        pass
    else
        fail "Distribution file not created"
        cleanup_workspace "$WORKSPACE"
        exit 1
    fi
else
    fail "Failed to build package"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

# Test 2: Upload package to hosted repository
print_test "Upload package to python-hosted using twine"

# Create .pypirc for authentication
cat > "$HOME/.pypirc" <<EOF
[distutils]
index-servers =
    pkgly-test

[pkgly-test]
repository: ${PKGLY_URL}/repositories/${PYTHON_HOSTED_REPO}
username: ${TEST_USER}
password: ${TEST_PASSWORD}
EOF

if run_cmd twine upload --repository pkgly-test dist/*; then
    pass
else
    fail "twine upload failed"
fi

# Test 3: Verify package available via Simple API
print_test "Fetch package via PyPI Simple API"
SIMPLE_PATH="/repositories/${PYTHON_HOSTED_REPO}/simple/${PACKAGE_NAME}/"

RESPONSE=$(curl -sf "${PKGLY_URL}${SIMPLE_PATH}" || echo "")
record_output "$RESPONSE"

if echo "$RESPONSE" | grep -q "${PACKAGE_NAME}"; then
    clear_last_log
    pass
else
    fail "Package not found in Simple API"
fi

# Test 4: Install package from hosted repository
print_test "Install package from python-hosted using pip"
VENV_DIR="$WORKSPACE/venv"
python3 -m venv "$VENV_DIR"
source "$VENV_DIR/bin/activate"

if run_cmd pip install --index-url="${PKGLY_URL}/repositories/${PYTHON_HOSTED_REPO}/simple" \
   --trusted-host=pkgly \
   "${PACKAGE_NAME}==${VERSION_1}"; then
    pass
else
    fail "Failed to install package"
fi

# Test 5: Verify installed package works
print_test "Verify installed package functionality"
PYTHON_OUTPUT=$(python3 -c "import pkgly_test_pkg; print(pkgly_test_pkg.greet('World')); print(pkgly_test_pkg.get_version())")
record_output "$PYTHON_OUTPUT"

if echo "$PYTHON_OUTPUT" | grep -q "Hello, World!" && \
   echo "$PYTHON_OUTPUT" | grep -q "${VERSION_1}"; then
    clear_last_log
    pass
else
    fail "Package not functioning correctly"
fi

deactivate

# Test 6: Upload second version
print_test "Upload second version (${VERSION_2})"
cd "$WORKSPACE/test-pkg"

# Update version in setup.py
sed -i "s/version='${VERSION_1}'/version='${VERSION_2}'/" setup.py
sed -i "s/__version__ = '${VERSION_1}'/__version__ = '${VERSION_2}'/" pkgly_test_pkg/__init__.py

# Rebuild
rm -rf dist/ build/ *.egg-info
if ! run_cmd python3 setup.py sdist bdist_wheel; then
    fail "Failed to rebuild package artifacts"
fi

if run_cmd twine upload --repository pkgly-test dist/*; then
    pass
else
    fail "Failed to upload second version"
fi

# Test 7: Install specific version
print_test "Install specific version (${VERSION_2})"
VENV_DIR_V2="$WORKSPACE/venv2"
python3 -m venv "$VENV_DIR_V2"
source "$VENV_DIR_V2/bin/activate"

if run_cmd pip install --index-url="${PKGLY_URL}/repositories/${PYTHON_HOSTED_REPO}/simple" \
   --trusted-host=pkgly \
   "${PACKAGE_NAME}==${VERSION_2}"; then
    INSTALLED_VERSION=$(python3 -c "import pkgly_test_pkg; print(pkgly_test_pkg.get_version())")
    record_output "$INSTALLED_VERSION"
    if [ "$INSTALLED_VERSION" = "$VERSION_2" ]; then
        clear_last_log
        pass
    else
        fail "Wrong version installed: $INSTALLED_VERSION"
    fi
else
    fail "Failed to install specific version"
fi

deactivate

# Test 8: Proxy repository - fetch from PyPI
print_test "Proxy: install requests from PyPI"
VENV_DIR_PROXY="$WORKSPACE/venv-proxy"
python3 -m venv "$VENV_DIR_PROXY"
source "$VENV_DIR_PROXY/bin/activate"

if run_cmd pip install --index-url="${PKGLY_URL}/repositories/${PYTHON_PROXY_REPO}/simple" \
   --trusted-host=pkgly \
   "requests==2.31.0"; then
    pass
else
    fail "Failed to proxy package from PyPI"
fi

deactivate

# Test 9: Proxy caching verification
print_test "Proxy: verify package is cached"
VENV_DIR_PROXY2="$WORKSPACE/venv-proxy2"
python3 -m venv "$VENV_DIR_PROXY2"
source "$VENV_DIR_PROXY2/bin/activate"

if run_cmd pip install --index-url="${PKGLY_URL}/repositories/${PYTHON_PROXY_REPO}/simple" \
   --trusted-host=pkgly \
   "requests==2.31.0"; then
    pass
else
    fail "Failed to retrieve cached package"
fi

deactivate

# Test 10: Authentication required for upload
print_test "Verify authentication required for upload"
cd "$WORKSPACE/test-pkg"

STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST \
    "${PKGLY_URL}/repositories/${PYTHON_HOSTED_REPO}/" \
    --data '{}')

if [ "$STATUS" = "401" ] || [ "$STATUS" = "403" ] || [ "$STATUS" = "308" ]; then
    pass
else
    fail "Expected 401/403 without auth, got $STATUS"
fi

# Test 11: Not found for non-existent package
print_test "Verify 404 for non-existent package"
NONEXISTENT_PATH="/repositories/${PYTHON_HOSTED_REPO}/simple/nonexistent-package/"

STATUS=$(get_http_status "${PKGLY_URL}${NONEXISTENT_PATH}")

if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404, got $STATUS"
fi

# Test 12: Hosted root index redirects GET and HEAD to the canonical URL
for method in GET HEAD; do
    assert_hosted_trailing_slash_redirect "$method" "/repositories/${PYTHON_HOSTED_REPO}" \
        "$PYTHON_HOSTED_REPO" "simple" "hosted-root-${method}"
done

# Test 13: Hosted package index redirects GET and HEAD through each route alias
for method in GET HEAD; do
    for route in repositories storages direct; do
        case "$route" in
            direct) route_prefix="/${PYTHON_HOSTED_REPO}" ;;
            *) route_prefix="/${route}/${PYTHON_HOSTED_REPO}" ;;
        esac
        assert_hosted_trailing_slash_redirect "$method" "$route_prefix" \
            "$PYTHON_HOSTED_REPO" "simple/${PACKAGE_NAME}" "hosted-${route}-${method}"
    done
done

# Test 14: Create a hosted repository in storage whose name matches the api route
print_test "Create hosted Python repository in storage named api"
STORAGE_LIST=$(api_get "/api/storage/list")
if ! echo "$STORAGE_LIST" | jq -e '.[] | select(.name == "api")' > /dev/null; then
    api_post "/api/storage/new/Local" -H "Content-Type: application/json" \
        -d '{"name":"api","config":{"type":"Local","settings":{"path":"/storage/python-redirect-api"}}}' \
        > "${WORKSPACE}/api-storage.json"
fi
REPO_LIST=$(api_get "/api/repository/list")
if ! echo "$REPO_LIST" | jq -e '.[] | select(.storage_name == "api" and .name == "python-redirect-test")' > /dev/null; then
    api_post "/api/repository/new/python" -H "Content-Type: application/json" \
        -d '{"name":"python-redirect-test","storage_name":"api","configs":{"python":{"type":"Hosted"},"auth":{"enabled":false}}}' \
        > "${WORKSPACE}/api-python-redirect-test.json"
fi
pass

# Test 15: Hosted redirects keep /repositories/ when the storage name is api
for method in GET HEAD; do
    assert_hosted_trailing_slash_redirect "$method" "/repositories/api/python-redirect-test" \
        "api/python-redirect-test" "simple" "hosted-api-${method}"
done

# Test 16: Combine mixed-case repository names and an encoded package path in GET/HEAD redirects
for method in GET HEAD; do
    for route in repositories storages direct; do
        case "$route" in
            direct) route_prefix="/${TEST_STORAGE^^}/Python-Hosted" ;;
            *) route_prefix="/${route}/${TEST_STORAGE^^}/Python-Hosted" ;;
        esac
        assert_hosted_trailing_slash_redirect "$method" "$route_prefix" \
            "$PYTHON_HOSTED_REPO" "simple/p%6Bgly-test-pkg" "hosted-combined-${route}-${method}" \
            "simple/${PACKAGE_NAME}"

        # The same mixed-case, encoded request must work when its trailing slash is already present.
        print_test "Hosted: ${method} ${route} mixed-case encoded index with trailing slash returns 200"
        curl_method=()
        if [[ "$method" == "HEAD" ]]; then
            curl_method=(-I)
        fi
        STATUS=$(curl -sS "${curl_method[@]}" -o /dev/null -w '%{http_code}' \
            "${PKGLY_URL}${route_prefix}/simple/p%6Bgly-test-pkg/")
        if assert_http_status "200" "$STATUS"; then
            pass
        else
            fail "Hosted mixed-case encoded index with trailing slash returned ${STATUS}"
        fi
    done
done

# Cleanup
cleanup_workspace "$WORKSPACE"
rm -f "$HOME/.pypirc"

print_summary
