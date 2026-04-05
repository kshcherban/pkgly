#!/bin/bash
# PHP (Composer V2) integration tests - hosted repository

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

PHP_HOSTED_REPO="${TEST_STORAGE}/php-hosted"
PHP_PROXY_REPO="${TEST_STORAGE}/php-proxy"
FIXTURE_DIR="/fixtures/php/sample-lib"
if [ ! -d "$FIXTURE_DIR" ]; then
    FIXTURE_DIR="${SCRIPT_DIR}/../fixtures/php/sample-lib"
fi
VENDOR="pkgly-test"
RUN_ID="$(random_string 8)"
PACKAGE="sample-lib-${RUN_ID}"
PACKAGE_NAME="${VENDOR}/${PACKAGE}"
VERSION_1="1.0.0"
VERSION_2="1.0.1"

print_section "PHP Integration Tests (Composer V2 hosted + proxy)"

WORKSPACE=$(create_workspace "php")
cd "$WORKSPACE"

copy_fixture() {
    rm -rf "$WORKSPACE/sample-lib"
    cp -r "$FIXTURE_DIR" "$WORKSPACE/sample-lib"
}

build_zip() {
    local version="$1"
    pushd "$WORKSPACE/sample-lib" >/dev/null
    jq --arg version "${version}" --arg name "${PACKAGE_NAME}" \
        '.version = $version | .name = $name' composer.json > composer.json.tmp && mv composer.json.tmp composer.json
    cat > src/Greeter.php <<EOF
<?php

namespace PkglyTest\\SampleLib;

class Greeter
{
    public static function greet(string \$name): string
    {
        return "Hello, {\$name}!";
    }

    public static function getVersion(): string
    {
        return '${version}';
    }
}
EOF
    run_cmd zip -qr "$WORKSPACE/sample-lib-${version}.zip" .
    popd >/dev/null
}

repo_id_by_name() {
    local repo_name="$1"
    local repos
    repos=$(api_get "/api/repository/list" || echo "[]")
    record_output "$repos"
    local repo_id
    repo_id=$(jq -r --arg name "$repo_name" '.[] | select(.name == $name) | .id' <<<"$repos" | head -n 1)
    if [[ -n "$repo_id" && "$repo_id" != "null" ]]; then
        clear_last_log
        echo "$repo_id"
        return 0
    fi
    return 1
}

composer_install() {
    local cache_dir="$1"
    mkdir -p "$cache_dir"
    run_cmd env COMPOSER_CACHE_DIR="$cache_dir" composer install --no-interaction --no-progress
}

evict_php_proxy_metadata_cache() {
    local proxy_repo_id="$1"
    local path="p2/${VENDOR}/${PACKAGE}.json"
    local payload
    payload=$(printf '{"paths":["%s"]}' "$path")
    local response
    if ! response=$(
        curl -fsS -X DELETE \
            -H "$(get_auth_header)" \
            -H "Content-Type: application/json" \
            "${PKGLY_URL}/api/repository/${proxy_repo_id}/packages" \
            --data "$payload" 2>&1
    ); then
        record_output "$response"
        return 1
    fi
    record_output "$response"
    if jq -e '.deleted >= 0' >/dev/null 2>&1 <<<"$response"; then
        clear_last_log
        return 0
    fi
    return 1
}

fetch_dist_url_from_metadata() {
    local repo="$1"
    local version="$2"
    local meta_path="/repositories/${repo}/p2/${VENDOR}/${PACKAGE}.json"
    local meta
    if ! meta=$(curl -fsS "${PKGLY_URL}${meta_path}" 2>&1); then
        record_output "$meta"
        return 1
    fi
    local dist_url
    if ! dist_url=$(
        jq -r --arg name "$PACKAGE_NAME" --arg ver "$version" \
            '([.packages[$name][]? | select(.version == $ver) | .dist.url] | last) // empty' <<<"$meta"
    ); then
        record_output "$meta"
        return 1
    fi
    record_output "$meta"
    echo "$dist_url"
}

fetch_proxy_upstream_url_from_metadata() {
    local version="$1"
    local meta_path="/repositories/${PHP_PROXY_REPO}/p2/${VENDOR}/${PACKAGE}.json"
    local meta
    if ! meta=$(curl -fsS "${PKGLY_URL}${meta_path}" 2>&1); then
        record_output "$meta"
        return 1
    fi
    local upstream_url
    if ! upstream_url=$(
        jq -r --arg name "$PACKAGE_NAME" --arg ver "$version" \
            '([.packages[$name][]? | select(.version == $ver) | .dist["pkgly-upstream-url"]] | last) // empty' <<<"$meta"
    ); then
        record_output "$meta"
        return 1
    fi
    record_output "$meta"
    echo "$upstream_url"
}

finish() {
    cleanup_workspace "$WORKSPACE"
    print_summary
}

# Test 0: ensure php-proxy exists (seeded)
print_test "Verify php-proxy repository exists"
if verify_repository_exists "php-proxy"; then
    pass
else
    fail "php-proxy repository missing from seed data"
    finish
    exit 1
fi

# Test 0b: resolve php-proxy repository id
print_test "Resolve php-proxy repository id"
if PHP_PROXY_ID=$(repo_id_by_name "php-proxy"); then
    pass
else
    fail "Failed to resolve php-proxy repository id"
    finish
    exit 1
fi

# Test 1: create and upload v1
print_test "Create and upload ${VERSION_1} dist"
if ! copy_fixture; then
    fail "Failed to copy PHP fixture"
    finish
    exit 1
fi
if ! build_zip "$VERSION_1"; then
    fail "Failed to create ${VERSION_1} dist zip"
    finish
    exit 1
fi
UPLOAD_PATH_V1="/repositories/${PHP_HOSTED_REPO}/dist/${VENDOR}/${PACKAGE}/${VERSION_1}.zip"
STATUS=$(get_http_status "${PKGLY_URL}${UPLOAD_PATH_V1}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/sample-lib-${VERSION_1}.zip")
if assert_http_status "201" "$STATUS"; then
    pass
else
    fail "Expected 201, got $STATUS"
    finish
    exit 1
fi

# Test 2: metadata contains an absolute dist URL (Composer requires scheme/host)
print_test "Verify metadata dist URL for ${VERSION_1}"
if ! DIST_URL=$(fetch_dist_url_from_metadata "${PHP_HOSTED_REPO}" "${VERSION_1}"); then
    fail "Failed to fetch or parse Composer metadata for ${VERSION_1}"
    finish
    exit 1
fi
if [[ -n "$DIST_URL" && "$DIST_URL" =~ ^https?:// ]]; then
    clear_last_log
    pass
else
    fail "Expected metadata dist.url to be an absolute URL, got: ${DIST_URL:-<empty>}"
    finish
    exit 1
fi

# Test 3: dist URL is retrievable
print_test "Verify dist download for ${VERSION_1}"
if run_cmd curl -fsS -o /dev/null "$DIST_URL"; then
    pass
else
    fail "Failed to download dist from metadata dist.url (${DIST_URL})"
    finish
    exit 1
fi

# Test 4: composer install v1
print_test "Composer install ${VERSION_1}"
CONSUMER_DIR="$WORKSPACE/consumer-v1"
mkdir -p "$CONSUMER_DIR"
cat > "$CONSUMER_DIR/composer.json" <<EOF
{
  "name": "test/consumer",
  "repositories": [
    { "type": "composer", "url": "${PKGLY_URL}/repositories/${PHP_HOSTED_REPO}" }
  ],
  "require": { "${PACKAGE_NAME}": "${VERSION_1}" },
  "config": { "secure-http": false }
}
EOF
pushd "$CONSUMER_DIR" >/dev/null
if composer_install "$WORKSPACE/composer-cache-hosted-v1"; then
    pass
else
    fail "Composer install failed"
    popd >/dev/null
    finish
    exit 1
fi

# Test 5: verify library output
print_test "Verify library output for ${VERSION_1}"
cat > test.php <<'EOF'
<?php
require 'vendor/autoload.php';
use PkglyTest\SampleLib\Greeter;
echo Greeter::greet('World') . PHP_EOL;
echo Greeter::getVersion() . PHP_EOL;
EOF
if OUTPUT=$(php test.php 2>&1); then
    record_output "$OUTPUT"
else
    record_output "$OUTPUT"
    fail "PHP execution failed for hosted consumer"
    popd >/dev/null
    finish
    exit 1
fi
if echo "$OUTPUT" | grep -q "Hello, World!" && echo "$OUTPUT" | grep -q "${VERSION_1}"; then
    clear_last_log
    pass
else
    fail "Unexpected output"
fi
popd >/dev/null

# Test 6: composer install via proxy v1
print_test "Composer install ${VERSION_1} via php-proxy"
CONSUMER_PROXY_DIR="$WORKSPACE/consumer-proxy-v1"
mkdir -p "$CONSUMER_PROXY_DIR"
cat > "$CONSUMER_PROXY_DIR/composer.json" <<EOF
{
  "name": "test/consumer-proxy-v1",
  "repositories": [
    { "type": "composer", "url": "${PKGLY_URL}/repositories/${PHP_PROXY_REPO}" }
  ],
  "require": { "${PACKAGE_NAME}": "${VERSION_1}" },
  "config": { "secure-http": false }
}
EOF
pushd "$CONSUMER_PROXY_DIR" >/dev/null
if composer_install "$WORKSPACE/composer-cache-proxy-v1"; then
    pass
else
    fail "Composer install failed via php-proxy"
    popd >/dev/null
    finish
    exit 1
fi

print_test "Verify proxy metadata rewrite for ${VERSION_1}"
if ! PROXY_DIST_URL=$(fetch_dist_url_from_metadata "${PHP_PROXY_REPO}" "${VERSION_1}"); then
    fail "Failed to fetch or parse Composer metadata from php-proxy for ${VERSION_1}"
    popd >/dev/null
    finish
    exit 1
fi
if [[ -n "$PROXY_DIST_URL" && "$PROXY_DIST_URL" =~ ^https?:// && "$PROXY_DIST_URL" == *"/repositories/${PHP_PROXY_REPO}/"* ]]; then
    clear_last_log
    pass
else
    fail "Expected proxy dist.url to point at proxy repo, got: ${PROXY_DIST_URL:-<empty>}"
    popd >/dev/null
    finish
    exit 1
fi

print_test "Verify proxy upstream URL for ${VERSION_1}"
if ! UPSTREAM_URL=$(fetch_proxy_upstream_url_from_metadata "${VERSION_1}"); then
    fail "Failed to read pkgly-upstream-url from proxy metadata for ${VERSION_1}"
    popd >/dev/null
    finish
    exit 1
fi
if [[ -n "$UPSTREAM_URL" && "$UPSTREAM_URL" =~ ^https?:// && "$UPSTREAM_URL" == *"/repositories/${PHP_HOSTED_REPO}/"* ]]; then
    clear_last_log
    pass
else
    fail "Expected pkgly-upstream-url to point at hosted repo, got: ${UPSTREAM_URL:-<empty>}"
    popd >/dev/null
    finish
    exit 1
fi

print_test "Verify proxy dist download for ${VERSION_1}"
if run_cmd curl -fsS -o /dev/null "$PROXY_DIST_URL"; then
    pass
else
    fail "Failed to download dist from proxy dist.url (${PROXY_DIST_URL})"
    popd >/dev/null
    finish
    exit 1
fi

print_test "Verify library output for ${VERSION_1} via php-proxy"
if OUTPUT_PROXY=$(php -r "require 'vendor/autoload.php'; echo PkglyTest\\SampleLib\\Greeter::getVersion();" 2>&1); then
    record_output "$OUTPUT_PROXY"
else
    record_output "$OUTPUT_PROXY"
    fail "PHP execution failed for proxy consumer"
    popd >/dev/null
    finish
    exit 1
fi
if [ "$OUTPUT_PROXY" = "$VERSION_1" ]; then
    clear_last_log
    pass
else
    fail "Expected version ${VERSION_1}, got ${OUTPUT_PROXY}"
    popd >/dev/null
    finish
    exit 1
fi
popd >/dev/null

# Test 6: upload v2
print_test "Create and upload ${VERSION_2} dist"
if ! copy_fixture; then
    fail "Failed to copy PHP fixture"
    finish
    exit 1
fi
if ! build_zip "$VERSION_2"; then
    fail "Failed to create ${VERSION_2} dist zip"
    finish
    exit 1
fi
UPLOAD_PATH_V2="/repositories/${PHP_HOSTED_REPO}/dist/${VENDOR}/${PACKAGE}/${VERSION_2}.zip"
STATUS=$(get_http_status "${PKGLY_URL}${UPLOAD_PATH_V2}" \
    -X PUT \
    -H "$(get_auth_header)" \
    --data-binary "@${WORKSPACE}/sample-lib-${VERSION_2}.zip")
if assert_http_status "201" "$STATUS"; then
    pass
else
    fail "Expected 201, got $STATUS"
    finish
    exit 1
fi

# Test 7: install v2 explicitly
print_test "Composer install ${VERSION_2}"
CONSUMER_DIR_V2="$WORKSPACE/consumer-v2"
mkdir -p "$CONSUMER_DIR_V2"
cat > "$CONSUMER_DIR_V2/composer.json" <<EOF
{
  "name": "test/consumer-v2",
  "repositories": [
    { "type": "composer", "url": "${PKGLY_URL}/repositories/${PHP_HOSTED_REPO}" }
  ],
  "require": { "${PACKAGE_NAME}": "${VERSION_2}" },
  "config": { "secure-http": false }
}
EOF
pushd "$CONSUMER_DIR_V2" >/dev/null
if composer_install "$WORKSPACE/composer-cache-hosted-v2"; then
    pass
else
    fail "Composer install failed for v2"
    popd >/dev/null
    finish
    exit 1
fi

print_test "Verify library output for ${VERSION_2}"
OUTPUT2=$(php -r "require 'vendor/autoload.php'; echo PkglyTest\\SampleLib\\Greeter::getVersion();")
record_output "$OUTPUT2"
if [ "$OUTPUT2" = "$VERSION_2" ]; then
    clear_last_log
    pass
else
    fail "Expected version ${VERSION_2}, got ${OUTPUT2}"
fi
popd >/dev/null

# Test 10: composer install via proxy v2
print_test "Evict php-proxy metadata cache"
if evict_php_proxy_metadata_cache "$PHP_PROXY_ID"; then
    pass
else
    fail "Failed to evict php-proxy metadata cache"
    finish
    exit 1
fi

print_test "Composer install ${VERSION_2} via php-proxy"
CONSUMER_PROXY_DIR_V2="$WORKSPACE/consumer-proxy-v2"
mkdir -p "$CONSUMER_PROXY_DIR_V2"
cat > "$CONSUMER_PROXY_DIR_V2/composer.json" <<EOF
{
  "name": "test/consumer-proxy-v2",
  "repositories": [
    { "type": "composer", "url": "${PKGLY_URL}/repositories/${PHP_PROXY_REPO}" }
  ],
  "require": { "${PACKAGE_NAME}": "${VERSION_2}" },
  "config": { "secure-http": false }
}
EOF
pushd "$CONSUMER_PROXY_DIR_V2" >/dev/null
if composer_install "$WORKSPACE/composer-cache-proxy-v2"; then
    pass
else
    fail "Composer install failed via php-proxy for v2"
    popd >/dev/null
    finish
    exit 1
fi

print_test "Verify library output for ${VERSION_2} via php-proxy"
if OUTPUT_PROXY_V2=$(php -r "require 'vendor/autoload.php'; echo PkglyTest\\SampleLib\\Greeter::getVersion();" 2>&1); then
    record_output "$OUTPUT_PROXY_V2"
else
    record_output "$OUTPUT_PROXY_V2"
    fail "PHP execution failed for proxy consumer v2"
    popd >/dev/null
    finish
    exit 1
fi
if [ "$OUTPUT_PROXY_V2" = "$VERSION_2" ]; then
    clear_last_log
    pass
else
    fail "Expected version ${VERSION_2}, got ${OUTPUT_PROXY_V2}"
    popd >/dev/null
    finish
    exit 1
fi
popd >/dev/null

# Test 8: upload requires auth
print_test "Upload without auth should be rejected"
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT \
    "${PKGLY_URL}${UPLOAD_PATH_V1}" \
    --data-binary "@${WORKSPACE}/sample-lib-${VERSION_1}.zip")
if assert_http_status "401" "$STATUS"; then
    pass
else
    fail "Expected 401 without auth, got $STATUS"
fi

# Test 9: 404 on missing package metadata
print_test "404 for missing package metadata"
MISSING_META="/repositories/${PHP_HOSTED_REPO}/p2/${VENDOR}/does-not-exist.json"
STATUS=$(get_http_status "${PKGLY_URL}${MISSING_META}")
if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404, got $STATUS"
fi

finish
