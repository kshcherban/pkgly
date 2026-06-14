#!/usr/bin/env bash
# ABOUTME: Runs Pkgly Helm chart unit, integration, and end-to-end tests.
# ABOUTME: Loads the configured local image into kind before cluster tests.
set -euo pipefail

tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
image_repository="${PKGLY_TEST_IMAGE_REPOSITORY:-pkgly}"
image_tag="${PKGLY_TEST_IMAGE_TAG:-test}"
kind_cluster="${PKGLY_KIND_CLUSTER:-kind}"

"$tests_dir/unit.sh"
kind load docker-image "$image_repository:$image_tag" --name "$kind_cluster"
"$tests_dir/integration.sh"
"$tests_dir/e2e.sh"
