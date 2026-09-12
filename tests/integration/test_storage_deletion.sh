#!/bin/bash
# ABOUTME: End-to-end storage deletion covering cascade confirmation and package cleanup.
# ABOUTME: Publishes Helm charts to Local and MinIO storages, then verifies deletion.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

FIXTURE_DIR="/fixtures/helm/test-chart"
CHART_NAME="test-chart"
VERSION="1.0.0"
SUFFIX="$(random_string 6)"
LOCAL_STORAGE_A="storage-del-a-${SUFFIX}"
LOCAL_STORAGE_B="storage-del-b-${SUFFIX}"
S3_STORAGE="storage-del-s3-${SUFFIX}"
REPO_NAME="storage-del-helm"
WORKSPACE=""

print_section "Storage Deletion Integration Tests"
wait_for_server 60

WORKSPACE=$(create_workspace "storage-deletion")
trap 'if [ -n "${WORKSPACE}" ]; then cleanup_workspace "${WORKSPACE}"; fi' EXIT

cd "${WORKSPACE}"
cp -r "${FIXTURE_DIR}" "${WORKSPACE}/test-chart"

create_storage() {
    local storage_type="$1"
    local name="$2"
    local config="$3"
    local payload
    payload=$(jq -n --arg name "$name" --argjson config "$config" \
        '{name: $name, config: $config}')
    curl -sS -o "${WORKSPACE}/${name}-storage.json" -w "%{http_code}" \
        -X POST -H "$(get_auth_header)" -H "Content-Type: application/json" \
        -d "$payload" "${PKGLY_URL}/api/storage/new/${storage_type}"
}

create_helm_repo() {
    local storage_name="$1"
    local payload
    payload=$(jq -n --arg name "$REPO_NAME" --arg storage "$storage_name" \
        '{name: $name, storage_name: $storage, configs: {helm: {mode: "http", overwrite: true}}}')
    curl -sS -o "${WORKSPACE}/${storage_name}-repo.json" -w "%{http_code}" \
        -X POST -H "$(get_auth_header)" -H "Content-Type: application/json" \
        -d "$payload" "${PKGLY_URL}/api/repository/new/helm"
}

upload_chart() {
    local storage_name="$1"
    local chart_package="$2"
    get_http_status "${PKGLY_URL}/repositories/${storage_name}/${REPO_NAME}/${chart_package}" \
        -X PUT -H "$(get_auth_header)" --data-binary "@${chart_package}"
}

delete_storage() {
    local storage_id="$1"
    local cascade="$2"
    local output_file="$3"
    local query=""
    if [ "$cascade" = "true" ]; then
        query="?cascade=true"
    fi
    curl -sS -o "$output_file" -w "%{http_code}" -X DELETE \
        -H "$(get_auth_header)" "${PKGLY_URL}/api/storage/${storage_id}${query}"
}

print_test "Package Helm chart"
if run_cmd helm package test-chart; then
    pass
else
    fail "Failed to package Helm chart"
fi
CHART_PACKAGE="${CHART_NAME}-${VERSION}.tgz"
if [ ! -f "${CHART_PACKAGE}" ]; then
    fail "Chart package not found: ${CHART_PACKAGE}"
    print_summary
    exit $?
fi

print_test "Create two populated Local storages"
STATUS_A=$(create_storage Local "$LOCAL_STORAGE_A" \
    "{\"type\":\"Local\",\"settings\":{\"path\":\"/storage/${LOCAL_STORAGE_A}\"}}")
STATUS_B=$(create_storage Local "$LOCAL_STORAGE_B" \
    "{\"type\":\"Local\",\"settings\":{\"path\":\"/storage/${LOCAL_STORAGE_B}\"}}")
if assert_http_status "201" "$STATUS_A" && assert_http_status "201" "$STATUS_B"; then
    pass
else
    fail "Failed to create storages (A=${STATUS_A}, B=${STATUS_B})"
fi
STORAGE_A_ID=$(jq -r '.id' "${WORKSPACE}/${LOCAL_STORAGE_A}-storage.json")
STORAGE_B_ID=$(jq -r '.id' "${WORKSPACE}/${LOCAL_STORAGE_B}-storage.json")

print_test "Create Helm repositories on both storages"
REPO_A_STATUS=$(create_helm_repo "$LOCAL_STORAGE_A")
REPO_B_STATUS=$(create_helm_repo "$LOCAL_STORAGE_B")
if assert_http_status "201" "$REPO_A_STATUS" && assert_http_status "201" "$REPO_B_STATUS"; then
    pass
else
    fail "Failed to create repositories (A=${REPO_A_STATUS}, B=${REPO_B_STATUS})"
fi
REPO_A_ID=$(jq -r '.id' "${WORKSPACE}/${LOCAL_STORAGE_A}-repo.json")
REPO_B_ID=$(jq -r '.id' "${WORKSPACE}/${LOCAL_STORAGE_B}-repo.json")

print_test "Publish a chart to both repositories"
UPLOAD_A=$(upload_chart "$LOCAL_STORAGE_A" "$CHART_PACKAGE")
UPLOAD_B=$(upload_chart "$LOCAL_STORAGE_B" "$CHART_PACKAGE")
if { [ "$UPLOAD_A" = "201" ] || [ "$UPLOAD_A" = "204" ]; } \
    && { [ "$UPLOAD_B" = "201" ] || [ "$UPLOAD_B" = "204" ]; }; then
    pass
else
    fail "Failed to publish charts (A=${UPLOAD_A}, B=${UPLOAD_B})"
fi

print_test "Confirm both repositories expose the published chart"
INDEX_A=$(api_get "/repositories/${LOCAL_STORAGE_A}/${REPO_NAME}/index.yaml" || echo "")
INDEX_B=$(api_get "/repositories/${LOCAL_STORAGE_B}/${REPO_NAME}/index.yaml" || echo "")
if assert_contains "$INDEX_A" "${CHART_PACKAGE}" && assert_contains "$INDEX_B" "${CHART_PACKAGE}"; then
    pass
else
    fail "Published chart missing from a repository index"
fi

print_test "Unconfirmed deletion returns 409 and preserves everything"
CONFLICT_STATUS=$(delete_storage "$STORAGE_A_ID" false "${WORKSPACE}/conflict.json")
CONFLICT_CODE=$(jq -r '.details.code' "${WORKSPACE}/conflict.json" 2>/dev/null || echo "")
CONFLICT_COUNT=$(jq -r '.details.repository_count' "${WORKSPACE}/conflict.json" 2>/dev/null || echo "")
STORAGE_A_AFTER=$(get_http_status "${PKGLY_URL}/api/storage/${STORAGE_A_ID}" -H "$(get_auth_header)")
INDEX_A_AFTER=$(api_get "/repositories/${LOCAL_STORAGE_A}/${REPO_NAME}/index.yaml" || echo "")
if assert_http_status "409" "$CONFLICT_STATUS" \
    && [ "$CONFLICT_CODE" = "storage_not_empty" ] \
    && [ "$CONFLICT_COUNT" = "1" ] \
    && assert_http_status "200" "$STORAGE_A_AFTER" \
    && assert_contains "$INDEX_A_AFTER" "${CHART_PACKAGE}"; then
    pass
else
    fail "Unconfirmed deletion changed state (status=${CONFLICT_STATUS}, code=${CONFLICT_CODE}, count=${CONFLICT_COUNT})"
fi

print_test "Cascade deletion removes the storage, repository, and package"
CASCADE_STATUS=$(delete_storage "$STORAGE_A_ID" true "${WORKSPACE}/cascade.json")
STORAGE_A_GONE=$(get_http_status "${PKGLY_URL}/api/storage/${STORAGE_A_ID}" -H "$(get_auth_header)")
REPO_A_GONE=$(get_http_status "${PKGLY_URL}/api/repository/${REPO_A_ID}" -H "$(get_auth_header)")
INDEX_A_GONE=$(get_http_status "${PKGLY_URL}/repositories/${LOCAL_STORAGE_A}/${REPO_NAME}/index.yaml" -H "$(get_auth_header)")
if assert_http_status "204" "$CASCADE_STATUS" \
    && assert_http_status "404" "$STORAGE_A_GONE" \
    && assert_http_status "404" "$REPO_A_GONE" \
    && assert_http_status "404" "$INDEX_A_GONE"; then
    pass
else
    fail "Cascade deletion left state (status=${CASCADE_STATUS}, storage=${STORAGE_A_GONE}, repo=${REPO_A_GONE}, index=${INDEX_A_GONE})"
fi

print_test "Unrelated storage contents remain accessible"
INDEX_B_AFTER=$(api_get "/repositories/${LOCAL_STORAGE_B}/${REPO_NAME}/index.yaml" || echo "")
STORAGE_B_AFTER=$(get_http_status "${PKGLY_URL}/api/storage/${STORAGE_B_ID}" -H "$(get_auth_header)")
if assert_contains "$INDEX_B_AFTER" "${CHART_PACKAGE}" && assert_http_status "200" "$STORAGE_B_AFTER"; then
    pass
else
    fail "Unrelated storage was affected by deletion"
fi

print_test "Empty storage deletes immediately without confirmation"
EMPTY_STORAGE="storage-del-empty-${SUFFIX}"
EMPTY_STATUS=$(create_storage Local "$EMPTY_STORAGE" \
    "{\"type\":\"Local\",\"settings\":{\"path\":\"/storage/${EMPTY_STORAGE}\"}}")
EMPTY_ID=$(jq -r '.id' "${WORKSPACE}/${EMPTY_STORAGE}-storage.json")
EMPTY_DELETE=$(delete_storage "$EMPTY_ID" false "${WORKSPACE}/empty.json")
EMPTY_GONE=$(get_http_status "${PKGLY_URL}/api/storage/${EMPTY_ID}" -H "$(get_auth_header)")
if assert_http_status "201" "$EMPTY_STATUS" \
    && assert_http_status "204" "$EMPTY_DELETE" \
    && assert_http_status "404" "$EMPTY_GONE"; then
    pass
else
    fail "Empty storage deletion failed (create=${EMPTY_STATUS}, delete=${EMPTY_DELETE}, gone=${EMPTY_GONE})"
fi

print_test "Create MinIO-backed storage and publish a chart"
S3_CONFIG="{\"type\":\"S3\",\"settings\":{\"bucket_name\":\"pkgly-test\",\"region\":\"us-east-1\",\"custom_region\":\"minio\",\"endpoint\":\"http://minio:9000\",\"credentials\":{\"access_key\":\"minioadmin\",\"secret_key\":\"minioadmin\"},\"path_style\":true}}"
S3_STATUS=$(create_storage s3 "$S3_STORAGE" "$S3_CONFIG")
S3_REPO_STATUS=$(create_helm_repo "$S3_STORAGE")
S3_REPO_ID=$(jq -r '.id' "${WORKSPACE}/${S3_STORAGE}-repo.json")
S3_UPLOAD=$(upload_chart "$S3_STORAGE" "$CHART_PACKAGE")
if assert_http_status "201" "$S3_STATUS" \
    && assert_http_status "201" "$S3_REPO_STATUS" \
    && { [ "$S3_UPLOAD" = "201" ] || [ "$S3_UPLOAD" = "204" ]; }; then
    pass
else
    fail "Failed to set up S3 storage (storage=${S3_STATUS}, repo=${S3_REPO_STATUS}, upload=${S3_UPLOAD})"
fi

print_test "Cascade deletion removes the S3-backed repository"
S3_STORAGE_ID=$(jq -r '.id' "${WORKSPACE}/${S3_STORAGE}-storage.json")
S3_DELETE=$(delete_storage "$S3_STORAGE_ID" true "${WORKSPACE}/s3-delete.json")
S3_REPO_GONE=$(get_http_status "${PKGLY_URL}/api/repository/${S3_REPO_ID}" -H "$(get_auth_header)")
S3_STORAGE_GONE=$(get_http_status "${PKGLY_URL}/api/storage/${S3_STORAGE_ID}" -H "$(get_auth_header)")
if assert_http_status "204" "$S3_DELETE" \
    && assert_http_status "404" "$S3_REPO_GONE" \
    && assert_http_status "404" "$S3_STORAGE_GONE"; then
    pass
else
    fail "S3 cleanup failed (delete=${S3_DELETE}, repo=${S3_REPO_GONE}, storage=${S3_STORAGE_GONE})"
fi

print_test "Cleanup remaining test storage"
CLEANUP_STATUS=$(delete_storage "$STORAGE_B_ID" true "${WORKSPACE}/cleanup.json")
if assert_http_status "204" "$CLEANUP_STATUS"; then
    pass
else
    fail "Failed to clean up remaining storage (status=${CLEANUP_STATUS})"
fi

print_summary
