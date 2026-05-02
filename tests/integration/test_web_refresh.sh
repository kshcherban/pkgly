#!/bin/bash
# ABOUTME: Verifies browser refresh routing and HTTP session cookie behavior.
# ABOUTME: Runs against the real Docker test stack with seeded data and APIs.

set -euo pipefail

source /tests/common.sh

print_section "Web Refresh Routes"

fetch_with_status() {
    local path="$1"
    shift
    curl -sS -o /tmp/pkgly-web-refresh-body -w "%{http_code}" "$@" "${PKGLY_URL}${path}"
}

assert_body_contains() {
    local needle="$1"
    if grep -q "$needle" /tmp/pkgly-web-refresh-body; then
        return 0
    fi
    echo "Expected response body to contain '$needle'" >&2
    return 1
}

assert_body_not_contains() {
    local needle="$1"
    if grep -q "$needle" /tmp/pkgly-web-refresh-body; then
        echo "Expected response body not to contain '$needle'" >&2
        return 1
    fi
    return 0
}

assert_spa_refresh() {
    local path="$1"
    print_test "SPA refresh $path"
    local status
    status=$(fetch_with_status "$path" -H "Accept: text/html")
    if assert_http_status "200" "$status" && grep -iq "<!doctype html" /tmp/pkgly-web-refresh-body; then
        pass
    else
        fail "Expected $path to return SPA index.html with HTTP 200"
    fi
}

wait_for_server 60

assert_spa_refresh "/page/repository/22222222-0000-0000-0000-000000000001"
assert_spa_refresh "/admin/repository/22222222-0000-0000-0000-000000000001"
assert_spa_refresh "/admin/user/1"
assert_spa_refresh "/admin/system/sso"
assert_spa_refresh "/admin/system/webhooks"
assert_spa_refresh "/browse/22222222-0000-0000-0000-000000000001/packages"
assert_spa_refresh "/projects/demo/1.0.0"

print_test "Static asset miss stays plain 404"
status=$(fetch_with_status "/assets/does-not-exist.js" -H "Accept: text/html")
if assert_http_status "404" "$status" && ! [ -s /tmp/pkgly-web-refresh-body ]; then
    pass
else
    fail "Expected static asset miss to return empty HTTP 404"
fi

print_test "Direct package route without HTML accept stays repository handled"
status=$(fetch_with_status "/test-storage/maven-hosted/com/example/missing/1.0/missing-1.0.pom" -H "Accept: application/json")
if assert_body_not_contains "<!DOCTYPE html"; then
    pass
else
    fail "Expected direct package request to avoid SPA index.html, got status $status"
fi

print_test "Repositories prefix stays repository handled"
status=$(fetch_with_status "/repositories/test-storage/maven-hosted/com/example/missing/1.0/missing-1.0.pom" -H "Accept: text/html")
if assert_body_not_contains "<!DOCTYPE html"; then
    pass
else
    fail "Expected /repositories package request to avoid SPA index.html, got status $status"
fi

print_test "Storages prefix stays repository handled"
status=$(fetch_with_status "/storages/test-storage/maven-hosted/com/example/missing/1.0/missing-1.0.pom" -H "Accept: text/html")
if assert_body_not_contains "<!DOCTYPE html"; then
    pass
else
    fail "Expected /storages package request to avoid SPA index.html, got status $status"
fi

print_test "Docker v2 route stays registry handled"
status=$(fetch_with_status "/v2/" -H "Accept: text/html")
if assert_http_status "401" "$status" && assert_body_not_contains "<!DOCTYPE html"; then
    pass
else
    fail "Expected unauthenticated Docker /v2/ to keep registry auth behavior and avoid SPA index.html"
fi

print_test "HTTP login cookie is refresh-safe"
cookie_jar="/tmp/pkgly-web-refresh-cookies.txt"
headers="/tmp/pkgly-web-refresh-headers"
body="/tmp/pkgly-web-refresh-login-body"
login_status=$(curl -sS -D "$headers" -o "$body" -w "%{http_code}" \
    -c "$cookie_jar" \
    -H "Content-Type: application/json" \
    -H "User-Agent: pkgly-web-refresh-test" \
    -X POST \
    -d '{"email_or_username":"admin","password":"TestAdmin"}' \
    "${PKGLY_URL}/api/user/login")
set_cookie=$(grep -i '^set-cookie:' "$headers" || true)
me_status=$(curl -sS -o /tmp/pkgly-web-refresh-me -w "%{http_code}" -b "$cookie_jar" "${PKGLY_URL}/api/user/me")

if assert_http_status "200" "$login_status" \
    && assert_http_status "200" "$me_status" \
    && echo "$set_cookie" | grep -q "HttpOnly" \
    && echo "$set_cookie" | grep -q "Path=/" \
    && echo "$set_cookie" | grep -q "SameSite=Lax" \
    && ! echo "$set_cookie" | grep -q "Secure"; then
    pass
else
    record_output "login status: $login_status
me status: $me_status
set-cookie: $set_cookie
login body:
$(cat "$body")"
    fail "Expected HTTP login session cookie without Secure and with SameSite=Lax"
fi

print_summary
