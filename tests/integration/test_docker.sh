#!/bin/bash
# Docker integration tests
# Verifies hosted registry push/pull flows and basic auth enforcement

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# Docker-specific configuration
DOCKER_REPOSITORY_NAME="${DOCKER_HOSTED_REPOSITORY:-docker-hosted}"
DOCKER_REPO_PATH="${TEST_STORAGE}/${DOCKER_REPOSITORY_NAME}"
FIXTURE_DIR="/fixtures/docker"
IMAGE_NAME="pkgly-test/testimg"
IMAGE_TAG="1.0.0-$(random_string 6)"

print_section "Docker Integration Tests"

WORKSPACE=$(create_workspace "docker")
cd "$WORKSPACE"

# Test 1: Build test image
print_test "Build test Docker image"
cp "${FIXTURE_DIR}/Dockerfile.testimg" "$WORKSPACE/Dockerfile"

if run_cmd docker build -t "${IMAGE_NAME}:${IMAGE_TAG}" "$WORKSPACE"; then
    pass
else
    fail "Failed to build Docker image"
fi

# Test 2: Docker login (basic auth with admin credentials)
print_test "Docker login to Pkgly"
DOCKER_REGISTRY_HOST="${PKGLY_DOCKER_HOST:-${PKGLY_URL#http://}}"
if run_cmd bash -lc "printf '%s' \"${TEST_PASSWORD}\" | docker login \"${DOCKER_REGISTRY_HOST}\" --username \"${TEST_USER}\" --password-stdin"; then
    pass
else
    fail "Docker login failed"
fi

# Test 3: Push image to hosted repository
print_test "Push Docker image to Pkgly"
REMOTE_IMAGE="${DOCKER_REGISTRY_HOST}/${DOCKER_REPO_PATH}/${IMAGE_NAME}"
run_cmd docker tag "${IMAGE_NAME}:${IMAGE_TAG}" "${REMOTE_IMAGE}:${IMAGE_TAG}"
if run_cmd docker push "${REMOTE_IMAGE}:${IMAGE_TAG}"; then
    pass
else
    fail "Failed to push Docker image"
fi

# Test 4: Pull image back from hosted repository
print_test "Pull Docker image from Pkgly"
run_cmd docker rmi "${IMAGE_NAME}:${IMAGE_TAG}" || true
if run_cmd docker pull "${REMOTE_IMAGE}:${IMAGE_TAG}"; then
    pass
else
    fail "Failed to pull Docker image"
fi

# Test 5: Verify blob endpoint returns 404 for unknown digest
print_test "Verify blob endpoint returns 404 for unknown digest"
BLOB_PATH="/v2/${DOCKER_REPO_PATH}/${IMAGE_NAME}/blobs/sha256:abc123"
STATUS=$(get_http_status "${PKGLY_URL}${BLOB_PATH}" -H "$(get_auth_header)")
if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404 for non-existent blob, got $STATUS"
fi

# Test 6: Authentication required for manifest upload
print_test "Verify authentication required for manifest upload"
UPLOAD_PATH="/v2/${DOCKER_REPO_PATH}/${IMAGE_NAME}/manifests/latest"
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT \
    "${PKGLY_URL}${UPLOAD_PATH}" \
    -H "Content-Type: application/vnd.docker.distribution.manifest.v2+json" \
    --data '{}')
if [ "$STATUS" = "401" ] || [ "$STATUS" = "403" ]; then
    pass
else
    fail "Expected 401/403 without auth, got $STATUS"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"
docker rmi "${REMOTE_IMAGE}:${IMAGE_TAG}" > /dev/null 2>&1 || true

print_summary
