#!/bin/bash
# ABOUTME: Docker E2E coverage for security hardening: CORS, traversal, egress.
# ABOUTME: Verifies reset-link poisoning, traversal block, and egress allowlist behavior.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

MAILPIT_URL="${MAILPIT_URL:-http://mailpit:8025}"

print_section "Security hardening E2E"

# 1. CORS: foreign-origin preflight must receive no CORS headers
print_test "foreign-origin preflight gets no CORS headers"
preflight_headers=$(curl -s -D - -o /dev/null \
    -X OPTIONS "${PKGLY_URL}/api/user/token/create" \
    -H "Origin: https://evil.example" \
    -H "Access-Control-Request-Method: POST" || true)
if echo "${preflight_headers}" | grep -qi "access-control-allow-origin" \
    || echo "${preflight_headers}" | grep -qi "access-control-allow-credentials"; then
    fail "preflight response contains CORS headers: ${preflight_headers}"
else
    pass
fi

# 2. Traversal: raw and encoded path traversal must be rejected with 400
print_test "traversal read returns 400"
status=$(curl -s -o /dev/null -w "%{http_code}" --path-as-is \
    "${PKGLY_URL}/local/security-test/../outside.txt" -H "Accept: */*")
if [ "$status" = "400" ]; then
    pass
else
    fail "traversal read expected 400, got ${status}"
fi

print_test "encoded traversal write returns 400"
status=$(curl -s -o /dev/null -w "%{http_code}" --path-as-is \
    -X PUT --data-binary "evil" \
    "${PKGLY_URL}/local/security-test/%2e%2e/outside.txt" -H "Accept: */*")
if [ "$status" = "400" ]; then
    pass
else
    fail "encoded traversal write expected 400, got ${status}"
fi

print_test "traversal delete returns 400"
status=$(curl -s -o /dev/null -w "%{http_code}" --path-as-is \
    -X DELETE "${PKGLY_URL}/local/security-test/a/../outside.txt" -H "Accept: */*")
if [ "$status" = "400" ]; then
    pass
else
    fail "traversal delete expected 400, got ${status}"
fi

# 3. Egress: webhook create rejects private loopback literals
print_test "webhook to loopback literal blocked on create"
status=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "${PKGLY_URL}/api/system/webhooks" \
    -H "Authorization: Bearer ${TEST_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"name":"blocked","enabled":true,"target_url":"http://127.0.0.1:8080/hook","events":["package.published"],"headers":[]}')
if [ "$status" = "400" ]; then
    pass
else
    fail "loopback webhook create expected 400, got ${status}"
fi

print_test "reserved IPv6 webhook literals are blocked on create"
ipv6_blocked=true
for target in 'http://[::ffff:8.8.8.8]/hook' 'http://[64:ff9b:1::1]/hook' 'http://[3fff::1]/hook'; do
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "${PKGLY_URL}/api/system/webhooks" \
        -H "Authorization: Bearer ${TEST_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{\"name\":\"blocked-ipv6\",\"enabled\":true,\"target_url\":\"${target}\",\"events\":[\"package.published\"],\"headers\":[]}")
    if [ "$status" != "400" ]; then
        ipv6_blocked=false
        fail "reserved IPv6 webhook ${target} expected 400, got ${status}"
        break
    fi
done
if [ "$ipv6_blocked" = true ]; then
    pass
fi

print_test "existing proxy config cannot be updated to loopback"
status=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT "${PKGLY_URL}/api/repository/55555555-0000-0000-0000-000000000002/config/php" \
    -H "Authorization: Bearer ${TEST_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"type":"Proxy","config":{"routes":[{"url":"http://127.0.0.1:8888","name":"blocked"}]}}')
if [ "$status" = "400" ]; then
    pass
else
    fail "loopback proxy update expected 400, got ${status}"
fi

print_test "persisted loopback webhook is blocked at delivery time without retry"
runtime_webhook=""
for attempt in $(seq 1 45); do
    runtime_webhook=$(curl -sf \
        "${PKGLY_URL}/api/system/webhooks/eeeeeeee-0000-0000-0000-000000000001" \
        -H "Authorization: Bearer ${TEST_TOKEN}" || echo '{}')
    if [ "$(echo "${runtime_webhook}" | jq -r '.last_delivery_status // empty')" = "failed" ]; then
        break
    fi
    sleep 1
done
runtime_status=$(echo "${runtime_webhook}" | jq -r '.last_delivery_status // empty')
runtime_error=$(echo "${runtime_webhook}" | jq -r '.last_error // empty')
if [ "$runtime_status" != "failed" ]; then
    fail "persisted loopback delivery did not fail: ${runtime_webhook}"
elif [ "$runtime_error" != "Webhook target is blocked by egress policy" ]; then
    fail "persisted loopback delivery was not rejected by egress policy: ${runtime_webhook}"
else
    pass
fi

# 4. Egress: allowlisted hostname exception is accepted
print_test "webhook to allowlisted host accepted"
created=$(curl -s -w "\n%{http_code}" \
    -X POST "${PKGLY_URL}/api/system/webhooks" \
    -H "Authorization: Bearer ${TEST_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"name":"allowed","enabled":true,"target_url":"http://pkgly:8888/hook","events":["package.published"],"headers":[]}')
status=$(echo "${created}" | tail -1)
if [ "$status" != "201" ]; then
    fail "allowlisted webhook create expected 201, got ${status}: ${created}"
    exit 1
fi
webhook_id=$(echo "${created}" | head -1 | jq -r .id)
curl -s -o /dev/null \
    -X DELETE "${PKGLY_URL}/api/system/webhooks/${webhook_id}" \
    -H "Authorization: Bearer ${TEST_TOKEN}" || true
pass

# 5. Password reset poisoning: hostile Origin must not appear in the link
print_test "password reset link ignores hostile Origin header"
previous_message_id=$(curl -sf "${MAILPIT_URL}/api/v1/messages" \
    | jq -r '[.messages[] | select(any(.To[]; .Address == "admin@pkgly.test"))] | sort_by(.Created) | last | .ID // empty' \
    || true)
reset_response=$(curl -s -w "\n%{http_code}" \
    -X POST "${PKGLY_URL}/api/user/password-reset/request" \
    -H "Origin: https://evil.example" \
    -H "Content-Type: application/json" \
    -d '{"email":"admin@pkgly.test"}' || true)
reset_status=$(echo "${reset_response}" | tail -1)
if [ "${reset_status}" != "200" ]; then
    fail "password reset request expected 200, got ${reset_status}: ${reset_response}"
    exit 1
fi
message_id=""
for attempt in $(seq 1 20); do
    messages=$(curl -sf "${MAILPIT_URL}/api/v1/messages" || echo '{"messages":[]}')
    candidate_message_id=$(echo "${messages}" | jq -r \
        '[.messages[] | select(any(.To[]; .Address == "admin@pkgly.test"))] | sort_by(.Created) | last | .ID // empty')
    if [ -n "${candidate_message_id}" ] && [ "${candidate_message_id}" != "${previous_message_id}" ]; then
        message_id="${candidate_message_id}"
        break
    fi
    sleep 1
done
if [ -z "${message_id}" ]; then
    fail "no password reset email arrived at mailpit"
    exit 1
fi
message_body=$(curl -sf "${MAILPIT_URL}/view/${message_id}.txt" || true)
if [ -z "${message_body}" ]; then
    fail "password reset email ${message_id} has no text body"
    exit 1
fi
reset_url=$(echo "${message_body}" | grep -oE 'https?://[^[:space:]]+' | head -1 || true)
if echo "${reset_url}" | grep -q "evil.example"; then
    fail "reset link contains hostile Origin: ${reset_url}"
elif ! echo "${reset_url}" | grep -q "^http://pkgly:8888/reset-password?token="; then
    fail "reset link does not point to site.app_url: ${reset_url}"
else
    pass
fi

print_section "Security hardening E2E complete"
echo ""
echo "Security tests: ${TESTS_RUN} run, ${TESTS_PASSED} passed, ${TESTS_FAILED} failed"
if [ "${TESTS_FAILED}" -gt 0 ]; then
    exit 1
fi
