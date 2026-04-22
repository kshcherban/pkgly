#!/bin/bash
# Python Virtual Repository integration tests
# Tests Python virtual repositories end-to-end (merge semantics, proxy compatibility, publish forwarding)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# Python-specific configuration
PYTHON_HOSTED_REPO="${TEST_STORAGE}/python-hosted"
PYTHON_PROXY_REPO="${TEST_STORAGE}/python-proxy"
PYTHON_HOSTED_2_REPO="${TEST_STORAGE}/python-hosted-2"
PYTHON_VIRTUAL_REPO="${TEST_STORAGE}/python-virtual"
FIXTURE_DIR="/fixtures/python/test-pkg"

print_section "Python Virtual Repository Integration Tests"

WORKSPACE=$(create_workspace "python-virtual")
cd "$WORKSPACE"

RUN_ID="$(date +%s)"
PACKAGE_NAME="pkgly-virtual-test-pkg-${RUN_ID}"
VERSION_1="1.0.0.post${RUN_ID}"
VERSION_2="1.0.1.post${RUN_ID}"
VERSION_3="1.0.2.post${RUN_ID}"

ensure_python_repo() {
    local repo_name="$1"
    local existing_id="$2"
    local storage_name="$3"

    if [ -n "$existing_id" ] && [ "$existing_id" != "null" ]; then
        echo "$existing_id"
        return 0
    fi

    local payload
    payload=$(cat <<JSON
{
  "name": "${repo_name}",
  "storage_name": "${storage_name}",
  "configs": {
    "python": { "type": "Hosted" },
    "auth": { "enabled": false }
  }
}
JSON
)
    local create_response
    create_response=$(api_post "/api/repository/new/python" -H "Content-Type: application/json" -d "$payload")
    echo "$create_response" | jq -r '.id'
}

ensure_python_virtual_repo() {
    local virtual_id="$1"
    local storage_name="$2"

    if [ -z "$virtual_id" ] || [ "$virtual_id" = "null" ]; then
        local payload
        payload=$(cat <<JSON
{
  "name": "python-virtual",
  "storage_name": "${storage_name}",
  "configs": {
    "python": {
      "type": "Virtual",
      "config": {
        "member_repositories": [
          {"repository_name": "python-hosted", "priority": 1, "enabled": true},
          {"repository_name": "python-hosted-2", "priority": 2, "enabled": true},
          {"repository_name": "python-proxy", "priority": 10, "enabled": true}
        ],
        "resolution_order": "Priority",
        "cache_ttl_seconds": 60,
        "publish_to": "python-hosted"
      }
    },
    "auth": { "enabled": false }
  }
}
JSON
)
        local create_response
        create_response=$(api_post "/api/repository/new/python" -H "Content-Type: application/json" -d "$payload")
        virtual_id=$(echo "$create_response" | jq -r '.id')
    fi

    if [ -z "$virtual_id" ] || [ "$virtual_id" = "null" ]; then
        fail "Failed to create or resolve python-virtual repository"
        cleanup_workspace "$WORKSPACE"
        exit 1
    fi

    local update_payload
    update_payload=$(cat <<JSON
{
  "members": [
    {"repository_name": "python-hosted", "priority": 1, "enabled": true},
    {"repository_name": "python-hosted-2", "priority": 2, "enabled": true},
    {"repository_name": "python-proxy", "priority": 10, "enabled": true}
  ],
  "resolution_order": "Priority",
  "cache_ttl_seconds": 60,
  "publish_to": "python-hosted"
}
JSON
)
    api_post "/api/repository/${virtual_id}/virtual/members" -H "Content-Type: application/json" -d "$update_payload" > /dev/null

    echo "$virtual_id"
}

print_test "Ensure python-hosted/python-proxy/python-hosted-2/python-virtual exist"
REPO_LIST=$(api_get "/api/repository/list" || echo "[]")
record_output "$REPO_LIST"

HOSTED_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="python-hosted") | .id')
PROXY_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="python-proxy") | .id')
HOSTED2_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="python-hosted-2") | .id')
VIRTUAL_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="python-virtual") | .id')
STORAGE_NAME=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="python-hosted") | .storage_name')

if [ -z "$HOSTED_ID" ] || [ "$HOSTED_ID" = "null" ] || [ -z "$PROXY_ID" ] || [ "$PROXY_ID" = "null" ]; then
    fail "Seeded python-hosted or python-proxy repository missing"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

HOSTED2_ID=$(ensure_python_repo "python-hosted-2" "$HOSTED2_ID" "$STORAGE_NAME")
VIRTUAL_ID=$(ensure_python_virtual_repo "$VIRTUAL_ID" "$STORAGE_NAME")

VIRTUAL_CFG=$(api_get "/api/repository/${VIRTUAL_ID}/virtual/members" || echo "{}")
record_output "$VIRTUAL_CFG"
MEMBER_COUNT=$(echo "$VIRTUAL_CFG" | jq '.members | length')
if [ "$MEMBER_COUNT" -ge 3 ]; then
    clear_last_log
    pass
else
    fail "Virtual members not configured"
fi

# Copy fixture
cp -r "$FIXTURE_DIR" "$WORKSPACE/test-pkg"
cd "$WORKSPACE/test-pkg"

# Make package name/version unique per run
sed -i "s/name='pkgly-test-pkg'/name='${PACKAGE_NAME}'/" setup.py
sed -i "s/version='1.0.0'/version='${VERSION_1}'/" setup.py
sed -i "s/__version__ = '1.0.0'/__version__ = '${VERSION_1}'/" pkgly_test_pkg/__init__.py

print_test "Build Python distribution package (${PACKAGE_NAME} ${VERSION_1})"
if run_cmd python3 setup.py sdist bdist_wheel; then
    pass
else
    fail "Failed to build package"
fi

print_test "Upload ${VERSION_1} to python-hosted (member 1)"
cat > "$HOME/.pypirc" <<EOF
[distutils]
index-servers =
    pkgly-hosted
    pkgly-hosted-2
    pkgly-virtual

[pkgly-hosted]
repository: ${PKGLY_URL}/repositories/${PYTHON_HOSTED_REPO}
username: ${TEST_USER}
password: ${TEST_PASSWORD}

[pkgly-hosted-2]
repository: ${PKGLY_URL}/repositories/${PYTHON_HOSTED_2_REPO}
username: ${TEST_USER}
password: ${TEST_PASSWORD}

[pkgly-virtual]
repository: ${PKGLY_URL}/repositories/${PYTHON_VIRTUAL_REPO}
username: ${TEST_USER}
password: ${TEST_PASSWORD}
EOF

if run_cmd twine upload --repository pkgly-hosted dist/*; then
    pass
else
    fail "twine upload to python-hosted failed"
fi

print_test "Upload ${VERSION_2} to python-hosted-2 (member 2)"
sed -i "s/version='${VERSION_1}'/version='${VERSION_2}'/" setup.py
sed -i "s/__version__ = '${VERSION_1}'/__version__ = '${VERSION_2}'/" pkgly_test_pkg/__init__.py
rm -rf dist/ build/ *.egg-info

if ! run_cmd python3 setup.py sdist bdist_wheel; then
    fail "Failed to rebuild package artifacts"
else
    if run_cmd twine upload --repository pkgly-hosted-2 dist/*; then
        pass
    else
        fail "twine upload to python-hosted-2 failed"
    fi
fi

print_test "Virtual: merged /simple/<pkg>/ includes ${VERSION_1} and ${VERSION_2}"
SIMPLE_VIRTUAL_PATH="/repositories/${PYTHON_VIRTUAL_REPO}/simple/${PACKAGE_NAME}/"
VIRTUAL_INDEX=$(curl -sf "${PKGLY_URL}${SIMPLE_VIRTUAL_PATH}" || echo "")
record_output "$VIRTUAL_INDEX"

if echo "$VIRTUAL_INDEX" | grep -q "${VERSION_1}" && echo "$VIRTUAL_INDEX" | grep -q "${VERSION_2}"; then
    clear_last_log
    pass
else
    fail "Virtual index did not contain both member versions"
fi

print_test "Virtual: install ${VERSION_1} via pip"
VENV_DIR_V1="$WORKSPACE/venv-v1"
python3 -m venv "$VENV_DIR_V1"
source "$VENV_DIR_V1/bin/activate"

if run_cmd pip install --index-url="${PKGLY_URL}/repositories/${PYTHON_VIRTUAL_REPO}/simple" \
   --trusted-host=pkgly \
   "${PACKAGE_NAME}==${VERSION_1}"; then
    cd "$WORKSPACE"
    INSTALLED_VERSION=$(python3 -c "import importlib.metadata as m; print(m.version('${PACKAGE_NAME}'))")
    record_output "$INSTALLED_VERSION"
    if [ "$INSTALLED_VERSION" = "$VERSION_1" ]; then
        clear_last_log
        pass
    else
        fail "Wrong version installed: $INSTALLED_VERSION"
    fi
else
    fail "Failed to install ${VERSION_1} via virtual"
fi

deactivate

print_test "Virtual: install ${VERSION_2} via pip"
VENV_DIR_V2="$WORKSPACE/venv-v2"
python3 -m venv "$VENV_DIR_V2"
source "$VENV_DIR_V2/bin/activate"

if run_cmd pip install --index-url="${PKGLY_URL}/repositories/${PYTHON_VIRTUAL_REPO}/simple" \
   --trusted-host=pkgly \
   "${PACKAGE_NAME}==${VERSION_2}"; then
    cd "$WORKSPACE"
    INSTALLED_VERSION=$(python3 -c "import importlib.metadata as m; print(m.version('${PACKAGE_NAME}'))")
    record_output "$INSTALLED_VERSION"
    if [ "$INSTALLED_VERSION" = "$VERSION_2" ]; then
        clear_last_log
        pass
    else
        fail "Wrong version installed: $INSTALLED_VERSION"
    fi
else
    fail "Failed to install ${VERSION_2} via virtual"
fi

deactivate

print_test "Virtual: proxy member works (requests==2.31.0 install via virtual)"
VENV_DIR_PROXY="$WORKSPACE/venv-proxy"
python3 -m venv "$VENV_DIR_PROXY"
source "$VENV_DIR_PROXY/bin/activate"

if run_cmd pip install --index-url="${PKGLY_URL}/repositories/${PYTHON_VIRTUAL_REPO}/simple" \
   --trusted-host=pkgly \
   "requests==2.31.0"; then
    pass
else
    fail "Failed to install requests via virtual (proxy member)"
fi

deactivate

print_test "Virtual: publish forwards to hosted publish target"
cd "$WORKSPACE/test-pkg"
sed -i "s/version='${VERSION_2}'/version='${VERSION_3}'/" setup.py
sed -i "s/__version__ = '${VERSION_2}'/__version__ = '${VERSION_3}'/" pkgly_test_pkg/__init__.py
rm -rf dist/ build/ *.egg-info

if ! run_cmd python3 setup.py sdist bdist_wheel; then
    fail "Failed to rebuild publish-forwarding artifacts"
else
    if run_cmd twine upload --repository pkgly-virtual dist/*; then
        pass
    else
        fail "twine upload to python-virtual failed"
    fi
fi

print_test "Hosted: /simple/<pkg>/ contains forwarded ${VERSION_3}"
SIMPLE_HOSTED_PATH="/repositories/${PYTHON_HOSTED_REPO}/simple/${PACKAGE_NAME}/"
HOSTED_INDEX=$(curl -sf "${PKGLY_URL}${SIMPLE_HOSTED_PATH}" || echo "")
record_output "$HOSTED_INDEX"

if echo "$HOSTED_INDEX" | grep -q "${VERSION_3}"; then
    clear_last_log
    pass
else
    fail "Hosted publish target did not contain forwarded version"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"
rm -f "$HOME/.pypirc"

print_summary
