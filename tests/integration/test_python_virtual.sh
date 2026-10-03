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

report_virtual_http_failure() {
    local route_label="$1"
    local member_label="$2"
    local requested_url="$3"
    local status="$4"
    local headers_file="$5"
    local body_file="$6"
    local failure_reason="$7"
    local body_size
    local headers

    body_size=$(stat -c %s "$body_file" 2>/dev/null || printf '0')
    headers=$(cat "$headers_file" 2>/dev/null || printf '<headers unavailable>')
    record_output "$(printf 'route=%s member=%s failure=%s\nrequested_url=%s\nstatus=%s\nbody_size=%s\nheaders:\n%s\n' \
        "$route_label" "$member_label" "$failure_reason" "$requested_url" "$status" "$body_size" "$headers")"
    return 1
}

assert_artifact_2xx() {
    local route_label="$1"
    local member_label="$2"
    local artifact_url="$3"
    local artifact_id="$4"
    local artifact_dir="${WORKSPACE}/artifact-validation"
    local head_headers_file
    local get_headers_file
    local get_body_file
    local head_status
    local get_status
    local body_size

    mkdir -p "$artifact_dir"
    head_headers_file=$(mktemp "${artifact_dir}/${artifact_id}.head.XXXXXX")
    get_headers_file=$(mktemp "${artifact_dir}/${artifact_id}.get.XXXXXX")
    get_body_file=$(mktemp "${artifact_dir}/${artifact_id}.body.XXXXXX")

    if ! head_status=$(curl -sS -L -I -D "$head_headers_file" -o /dev/null -w '%{http_code}' "$artifact_url"); then
        report_virtual_http_failure "$route_label" "$member_label" "$artifact_url" "000" \
            "$head_headers_file" "$get_body_file" "artifact HEAD connectivity"
        return 1
    fi
    if [[ ! "$head_status" =~ ^2[0-9]{2}$ ]]; then
        report_virtual_http_failure "$route_label" "$member_label" "$artifact_url" "$head_status" \
            "$head_headers_file" "$get_body_file" "artifact HEAD status"
        return 1
    fi

    if ! get_status=$(curl -sS -L -D "$get_headers_file" -o "$get_body_file" -w '%{http_code}' "$artifact_url"); then
        report_virtual_http_failure "$route_label" "$member_label" "$artifact_url" "000" \
            "$get_headers_file" "$get_body_file" "artifact GET connectivity"
        return 1
    fi
    body_size=$(stat -c %s "$get_body_file" 2>/dev/null || printf '0')
    if [[ ! "$get_status" =~ ^2[0-9]{2}$ ]]; then
        report_virtual_http_failure "$route_label" "$member_label" "$artifact_url" "$get_status" \
            "$get_headers_file" "$get_body_file" "artifact GET status"
        return 1
    fi
    if [[ ! "$body_size" =~ ^[1-9][0-9]*$ ]]; then
        report_virtual_http_failure "$route_label" "$member_label" "$artifact_url" "$get_status" \
            "$get_headers_file" "$get_body_file" "artifact GET empty body"
        return 1
    fi
}

fetch_virtual_index() {
    local route_label="$1"
    local member_label="$2"
    local index_url="$3"
    local index_body="$4"
    local index_headers="$5"
    local status
    local body_size

    if ! status=$(curl -sS -L -D "$index_headers" -o "$index_body" -w '%{http_code}' "$index_url"); then
        status="000"
    fi
    body_size=$(stat -c %s "$index_body" 2>/dev/null || printf '0')
    if [[ ! "$status" =~ ^2[0-9]{2}$ ]]; then
        report_virtual_http_failure "$route_label" "$member_label" "$index_url" "$status" \
            "$index_headers" "$index_body" "virtual index status"
        return 1
    fi
    if [[ "$body_size" -le 0 ]]; then
        report_virtual_http_failure "$route_label" "$member_label" "$index_url" "$status" "$index_headers" "$index_body" "empty virtual index"
        return 1
    fi
}

assert_virtual_trailing_slash_redirect() {
    local route_label="$1"
    local route_prefix="$2"
    local package_name="$3"
    local redirect_id="$4"
    local requested_url="${PKGLY_URL}${route_prefix}/simple/${package_name}"
    local expected_location="/repositories/${PYTHON_VIRTUAL_REPO}/simple/${package_name}/"
    local headers_file="${WORKSPACE}/${redirect_id}.headers"
    local status
    local location

    print_test "Virtual: ${route_label} route redirects a package index without a trailing slash to the canonical URL"
    # Check the redirect response itself, so curl must not follow it with -L.
    if ! status=$(curl -sS -D "$headers_file" -o /dev/null -w '%{http_code}' "$requested_url"); then
        status="000"
    fi
    location=$(sed -n -E 's/^[Ll]ocation:[[:space:]]*([^\r]*)\r?$/\1/p' "$headers_file" | tail -n 1)

    if [[ "$status" == "301" && "$location" == "$expected_location" ]]; then
        clear_last_log
        pass
        return 0
    fi

    record_output "$(printf 'route=%s\nrequested_url=%s\nstatus=%s\nlocation=%s\nexpected_location=%s\n' \
        "$route_label" "$requested_url" "$status" "$location" "$expected_location")"
    fail "Virtual ${route_label} route did not return a 301 redirect to ${expected_location}"
}

extract_selected_wheel_href() {
    local route_label="$1"
    local member_label="$2"
    local index_body="$3"
    local canonical_prefix="/repositories/${PYTHON_VIRTUAL_REPO}/"
    local href
    local wheel_hrefs=()

    mapfile -t wheel_hrefs < <(
        grep -Eo 'href="[^"]+\.whl[^"]*"' "$index_body" |
            sed -E 's/^href="([^"]+)".*$/\1/' |
            sort -u || true
    )
    if [ "${#wheel_hrefs[@]}" -eq 0 ]; then
        record_output "route=${route_label} member=${member_label}\nindex_body_size=$(stat -c %s "$index_body" 2>/dev/null || printf '0')\nNo wheel href found in ${index_body}"
        return 1
    fi

    for href in "${wheel_hrefs[@]}"; do
        if [[ "$href" != "$canonical_prefix"* ]]; then
            record_output "route=${route_label} member=${member_label}\nNon-canonical wheel href: ${href}\nRequired prefix: ${canonical_prefix}"
            return 1
        fi
    done
    SELECTED_WHEEL_HREF="${wheel_hrefs[0]}"
}

check_virtual_index_and_artifact() {
    local route_label="$1"
    local member_label="$2"
    local route_prefix="$3"
    local package_name="$4"
    local check_id="$5"
    local index_body="${WORKSPACE}/${check_id}.index.html"
    local index_headers="${WORKSPACE}/${check_id}.index.headers"
    local index_url="${PKGLY_URL}${route_prefix}/simple/${package_name}/"

    print_test "Virtual: ${member_label} member via ${route_label} route validates index and artifact"
    if fetch_virtual_index "$route_label" "$member_label" "$index_url" "$index_body" "$index_headers" &&
        extract_selected_wheel_href "$route_label" "$member_label" "$index_body" &&
        assert_artifact_2xx "$route_label" "$member_label" "${PKGLY_URL}${SELECTED_WHEEL_HREF}" "$check_id"; then
        clear_last_log
        pass
    else
        fail "Virtual ${member_label}/${route_label} index or artifact validation failed"
    fi
}

install_from_virtual_index() {
    local route_label="$1"
    local route_prefix="$2"
    local package_spec="$3"
    local expected_version="$4"
    local venv_name="$5"
    local venv_dir="${WORKSPACE}/${venv_name}"
    local index_url="${PKGLY_URL}${route_prefix}/simple"
    local installed_version

    print_test "Virtual: pip installs ${package_spec} through ${route_label} route"
    if ! python3 -m venv "$venv_dir"; then
        fail "Failed to create virtualenv ${venv_name}"
        return
    fi
    source "${venv_dir}/bin/activate"
    if run_cmd pip install --no-cache-dir --force-reinstall --index-url="$index_url" \
        --trusted-host=pkgly "$package_spec"; then
        if [ -n "$expected_version" ]; then
            cd "$WORKSPACE"
            if installed_version=$(python3 -c "import importlib.metadata as m; print(m.version('${PACKAGE_NAME}'))"); then
                record_output "$installed_version"
                if [ "$installed_version" = "$expected_version" ]; then
                    clear_last_log
                    pass
                else
                    fail "Wrong version installed through ${route_label}: ${installed_version}"
                fi
            else
                fail "Could not read installed version through ${route_label}"
            fi
        else
            pass
        fi
    else
        fail "Failed to install ${package_spec} through ${route_label}"
    fi
    deactivate
}

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
MERGED_INDEX_BODY="${WORKSPACE}/merged.index.html"
MERGED_INDEX_HEADERS="${WORKSPACE}/merged.index.headers"
if fetch_virtual_index "canonical" "hosted" "${PKGLY_URL}${SIMPLE_VIRTUAL_PATH}" \
    "$MERGED_INDEX_BODY" "$MERGED_INDEX_HEADERS"; then
    VIRTUAL_INDEX=$(cat "$MERGED_INDEX_BODY")
    record_output "$VIRTUAL_INDEX"
fi

if [ -s "$MERGED_INDEX_BODY" ] &&
    grep -q "${VERSION_1}" "$MERGED_INDEX_BODY" &&
    grep -q "${VERSION_2}" "$MERGED_INDEX_BODY"; then
    clear_last_log
    pass
else
    fail "Virtual index did not contain both member versions"
fi

assert_virtual_trailing_slash_redirect "canonical" \
    "/repositories/${PYTHON_VIRTUAL_REPO}" "$PACKAGE_NAME" "redirect-canonical"
assert_virtual_trailing_slash_redirect "storages" \
    "/storages/${PYTHON_VIRTUAL_REPO}" "$PACKAGE_NAME" "redirect-storages"
assert_virtual_trailing_slash_redirect "direct" \
    "/${PYTHON_VIRTUAL_REPO}" "$PACKAGE_NAME" "redirect-direct"

check_virtual_index_and_artifact "canonical" "hosted" \
    "/repositories/${PYTHON_VIRTUAL_REPO}" "$PACKAGE_NAME" "hosted-canonical"
check_virtual_index_and_artifact "storages" "hosted" \
    "/storages/${PYTHON_VIRTUAL_REPO}" "$PACKAGE_NAME" "hosted-storages"
check_virtual_index_and_artifact "direct" "hosted" \
    "/${PYTHON_VIRTUAL_REPO}" "$PACKAGE_NAME" "hosted-direct"
check_virtual_index_and_artifact "canonical" "live-pypi" \
    "/repositories/${PYTHON_VIRTUAL_REPO}" "requests" "proxy-canonical"
check_virtual_index_and_artifact "storages" "live-pypi" \
    "/storages/${PYTHON_VIRTUAL_REPO}" "requests" "proxy-storages"
check_virtual_index_and_artifact "direct" "live-pypi" \
    "/${PYTHON_VIRTUAL_REPO}" "requests" "proxy-direct"

install_from_virtual_index "canonical" \
    "/repositories/${PYTHON_VIRTUAL_REPO}" "${PACKAGE_NAME}==${VERSION_1}" \
    "$VERSION_1" "venv-v1"
install_from_virtual_index "direct" \
    "/${PYTHON_VIRTUAL_REPO}" "${PACKAGE_NAME}==${VERSION_2}" \
    "$VERSION_2" "venv-v2"
install_from_virtual_index "canonical" \
    "/repositories/${PYTHON_VIRTUAL_REPO}" "requests==2.31.0" "" "venv-proxy"
install_from_virtual_index "direct" \
    "/${PYTHON_VIRTUAL_REPO}" "requests==2.31.0" "" "venv-proxy-direct"

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
FORWARDED_INDEX_BODY="${WORKSPACE}/forwarded.index.html"
FORWARDED_INDEX_HEADERS="${WORKSPACE}/forwarded.index.headers"
if fetch_virtual_index "canonical" "hosted" "${PKGLY_URL}${SIMPLE_HOSTED_PATH}" \
    "$FORWARDED_INDEX_BODY" "$FORWARDED_INDEX_HEADERS"; then
    HOSTED_INDEX=$(cat "$FORWARDED_INDEX_BODY")
    record_output "$HOSTED_INDEX"
fi

if [ -s "$FORWARDED_INDEX_BODY" ] && grep -q "${VERSION_3}" "$FORWARDED_INDEX_BODY"; then
    clear_last_log
    pass
else
    fail "Hosted publish target did not contain forwarded version"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"
rm -f "$HOME/.pypirc"

print_summary
