#!/bin/bash
# ABOUTME: Exercises a real Docker registry backed by the pinned MinIO S3 service.
# ABOUTME: Verifies guarded uploads, manifest reads, cache warming, restart recovery, and cleanup.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

S3_REPOSITORY_NAME="${S3_DOCKER_REPOSITORY:-docker-s3-hosted}"
S3_REPOSITORY_ID="33333333-0000-0000-0000-000000000003"
S3_STORAGE_NAME="${S3_DOCKER_STORAGE:-test-s3-storage}"
S3_REPOSITORY_PATH="${S3_STORAGE_NAME}/${S3_REPOSITORY_NAME}"
FIXTURE_DIR="/fixtures/docker"
IMAGE_NAME="pkgly-test/s3-testimg"
TAG_FILE="/results/s3-image-tag"
IMAGE_TAG=""
DOCKER_REGISTRY_HOST="${PKGLY_DOCKER_HOST:-${PKGLY_URL#http://}}"
REMOTE_IMAGE="${DOCKER_REGISTRY_HOST}/${S3_REPOSITORY_PATH}/${IMAGE_NAME}"
S3_PROXY_REPOSITORY_NAME="docker-s3-proxy-auth"
S3_PROXY_REPOSITORY_ID=""
S3_PROXY_REPOSITORY_PATH="${S3_STORAGE_NAME}/${S3_PROXY_REPOSITORY_NAME}"
WORKSPACE=""

print_section "S3 / MinIO Integration Tests"
wait_for_server 60

if [ "${PKGLY_S3_PHASE:-initial}" = "post_restart" ]; then
    IMAGE_TAG=$(cat "$TAG_FILE")
    print_test "Pull image after Pkgly restart while MinIO is unavailable"
    docker rmi "${IMAGE_NAME}:${IMAGE_TAG}" >/dev/null 2>&1 || true
    if run_cmd docker pull "${REMOTE_IMAGE}:${IMAGE_TAG}"; then
        pass
    else
        fail "Failed to pull the image after restarting Pkgly"
    fi

    print_summary
    exit $?
fi

if [ "${PKGLY_S3_PHASE:-initial}" = "cleanup" ]; then
    print_test "Delete S3-backed repository and its MinIO objects"
    status=$(curl -sS -o /dev/null -w "%{http_code}" -X DELETE \
        "${PKGLY_URL}/api/repository/${S3_REPOSITORY_ID}" \
        -H "$(get_auth_header)")
    if assert_http_status "204" "$status"; then
        pass
    else
        fail "Expected repository deletion to return 204, got ${status}"
    fi

    print_test "Verify deleted S3-backed repository is no longer discoverable"
    status=$(curl -sS -o /dev/null -w "%{http_code}" \
        "${PKGLY_URL}/api/repository/${S3_REPOSITORY_ID}" \
        -H "$(get_auth_header)")
    if assert_http_status "404" "$status"; then
        pass
    else
        fail "Expected deleted S3 repository to return 404, got ${status}"
    fi

    print_summary
    exit $?
fi

IMAGE_TAG="1.0.0-$(random_string 6)"
printf '%s' "$IMAGE_TAG" >"$TAG_FILE"
WORKSPACE=$(create_workspace "s3")
trap 'if [ -n "${WORKSPACE}" ]; then cleanup_workspace "${WORKSPACE}"; fi' EXIT
cp "${FIXTURE_DIR}/Dockerfile.testimg" "${WORKSPACE}/Dockerfile"

print_test "Build test image for MinIO-backed repository"
if run_cmd docker build -t "${IMAGE_NAME}:${IMAGE_TAG}" "${WORKSPACE}"; then
    pass
else
    fail "Failed to build Docker image"
fi

print_test "Login to the S3-backed Docker repository"
if run_cmd bash -lc "printf '%s' \"${TEST_PASSWORD}\" | docker login \"${DOCKER_REGISTRY_HOST}\" --username \"${TEST_USER}\" --password-stdin"; then
    pass
else
    fail "Docker login failed"
fi

print_test "Create auth-enabled Docker proxy on S3 storage"
S3_PROXY_CONFIG=$(jq -n \
    --arg name "$S3_PROXY_REPOSITORY_NAME" \
    --arg storage_name "$S3_STORAGE_NAME" \
    '{
        name: $name,
        storage_name: $storage_name,
        configs: {
            docker: {
                type: "Proxy",
                config: {
                    upstream_url: "https://registry-1.docker.io"
                }
            },
            auth: {
                enabled: true
            }
        }
    }')
S3_PROXY_CREATE_STATUS=$(curl -sS -o "$WORKSPACE/s3-proxy.json" -w "%{http_code}" \
    -X POST \
    -H "$(get_auth_header)" \
    -H "Content-Type: application/json" \
    "${PKGLY_URL}/api/repository/new/docker" \
    -d "$S3_PROXY_CONFIG")
if assert_http_status "201" "$S3_PROXY_CREATE_STATUS"; then
    S3_PROXY_REPOSITORY_ID=$(jq -r '.id' "$WORKSPACE/s3-proxy.json")
    pass
else
    fail "Expected S3-backed proxy creation to return 201, got ${S3_PROXY_CREATE_STATUS}"
fi

print_test "Read upstream manifest through auth-enabled S3 proxy"
S3_PROXY_MANIFEST_URL="${PKGLY_URL}/v2/${S3_PROXY_REPOSITORY_PATH}/library/alpine/manifests/latest"
if run_cmd curl -sfI -H "$(get_auth_header)" "$S3_PROXY_MANIFEST_URL" >/dev/null; then
    pass
else
    fail "Failed to HEAD a manifest through the auth-enabled S3 proxy"
fi

print_test "Push image through Pkgly into MinIO"
run_cmd docker tag "${IMAGE_NAME}:${IMAGE_TAG}" "${REMOTE_IMAGE}:${IMAGE_TAG}"
if run_cmd docker push "${REMOTE_IMAGE}:${IMAGE_TAG}"; then
    pass
else
    fail "Failed to push image to the S3-backed repository"
fi

print_test "Read pushed manifest through the S3-backed registry"
manifest_status=$(curl -sS -o /dev/null -w "%{http_code}" \
    "${PKGLY_URL}/v2/${S3_REPOSITORY_PATH}/${IMAGE_NAME}/manifests/${IMAGE_TAG}" \
    -H "$(get_auth_header)")
if assert_http_status "200" "$manifest_status"; then
    pass
else
    fail "Expected manifest GET to return 200, got ${manifest_status}"
fi

print_test "Confirm S3 cache sidecars were written"
if find /pkgly-storage/s3-cache -type f -name '*.meta.json' -print -quit 2>/dev/null | grep -q .; then
    pass
else
    fail "Expected at least one cache metadata sidecar"
fi

print_test "Pull image from MinIO-backed repository"
docker rmi "${IMAGE_NAME}:${IMAGE_TAG}" >/dev/null 2>&1 || true
if run_cmd docker pull "${REMOTE_IMAGE}:${IMAGE_TAG}"; then
    pass
else
    fail "Failed to pull image from the S3-backed repository"
fi

docker rmi "${REMOTE_IMAGE}:${IMAGE_TAG}" >/dev/null 2>&1 || true
docker rmi "${IMAGE_NAME}:${IMAGE_TAG}" >/dev/null 2>&1 || true

if [ -n "$S3_PROXY_REPOSITORY_ID" ]; then
    print_test "Delete auth-enabled S3 proxy repository"
    S3_PROXY_DELETE_STATUS=$(curl -sS -o /dev/null -w "%{http_code}" -X DELETE \
        "${PKGLY_URL}/api/repository/${S3_PROXY_REPOSITORY_ID}" \
        -H "$(get_auth_header)")
    if assert_http_status "204" "$S3_PROXY_DELETE_STATUS"; then
        pass
    else
        fail "Expected S3-backed proxy deletion to return 204, got ${S3_PROXY_DELETE_STATUS}"
    fi
fi

print_summary
