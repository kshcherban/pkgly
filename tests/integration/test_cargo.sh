#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

print_section "Cargo Integration Tests"

CARGO_REPO="${TEST_STORAGE}/cargo-hosted"
CRATE_NAME="pkgly-cargo-test"
WORKSPACE=$(create_workspace "cargo")
CRATE_DIR="${WORKSPACE}/${CRATE_NAME}"
cp -R "/fixtures/cargo/${CRATE_NAME}" "${CRATE_DIR}"

PATCH=$(( RANDOM % 9000 + 1000 ))
VERSION="1.0.${PATCH}"
python3 - <<PY
from pathlib import Path
manifest = Path("${CRATE_DIR}/Cargo.toml")
text = manifest.read_text()
text = text.replace('version = "0.1.0"', 'version = "${VERSION}"', 1)
manifest.write_text(text)
PY

if [[ "${PKGLY_URL}" == *"://"* ]]; then
    CARGO_HOST="${PKGLY_URL#*://}"
else
    CARGO_HOST="${PKGLY_URL}"
fi
CARGO_HOST="${CARGO_HOST%%/*}"
CARGO_HOST="${CARGO_HOST%/}"
REGISTRY_BASE="http://${CARGO_HOST}/repositories/${CARGO_REPO}"
SPARSE_INDEX_URL="sparse+${REGISTRY_BASE}/index/"
mkdir -p /root/.cargo
cat > /root/.cargo/config.toml <<CONFIG
[registries.pkgly]
index = "${SPARSE_INDEX_URL}"
CONFIG

print_test "Publish crate ${VERSION} via Cargo CLI"
cd "${CRATE_DIR}"
if CARGO_REGISTRIES_PKGLY_TOKEN="${TEST_TOKEN}" run_cmd cargo publish --registry pkgly --allow-dirty; then
    pass
else
    fail "cargo publish failed"
fi

ARCHIVE_PATH="${CRATE_DIR}/target/package/${CRATE_NAME}-${VERSION}.crate"
print_test "Ensure local crate archive is available"
if [[ ! -f "${ARCHIVE_PATH}" ]]; then
    if ! run_cmd cargo package --allow-dirty --no-verify; then
        fail "Failed to create local crate archive"
    fi
fi

if assert_file_exists "${ARCHIVE_PATH}"; then
    pass
else
    fail "Local crate archive missing"
fi

print_test "Fetch sparse index entry"
INDEX_PATH=$(python3 - "${CRATE_NAME}" <<'PY'
import sys
name = sys.argv[1]
if len(name) == 1:
    path = f"index/1/{name}"
elif len(name) == 2:
    path = f"index/2/{name}"
elif len(name) == 3:
    path = f"index/3/{name[0]}/{name}"
else:
    path = f"index/{name[0:2]}/{name[2:4]}/{name}"
print(path)
PY
)
INDEX_CONTENT=$(curl -sf "${PKGLY_URL}/repositories/${CARGO_REPO}/${INDEX_PATH}" || true)
record_output "${INDEX_CONTENT}"
if [[ -n "${INDEX_CONTENT}" ]] && grep -q "\"vers\":\"${VERSION}\"" <<<"${INDEX_CONTENT}"; then
    clear_last_log
    pass
else
    fail "Version ${VERSION} missing from sparse index"
fi

print_test "Fetch crate metadata API"
CRATE_METADATA=$(curl -sf "${PKGLY_URL}/repositories/${CARGO_REPO}/api/v1/crates/${CRATE_NAME}" || true)
record_output "${CRATE_METADATA}"
if jq -e ".versions[] | select(.num == \"${VERSION}\")" <<<"${CRATE_METADATA}" > /dev/null 2>&1; then
    clear_last_log
    pass
else
    fail "API metadata missing published version"
fi

print_test "Download crate via API"
DOWNLOAD_PATH="${WORKSPACE}/${CRATE_NAME}-${VERSION}.crate"
if curl -sfL "${PKGLY_URL}/repositories/${CARGO_REPO}/api/v1/crates/${CRATE_NAME}/${VERSION}/download" -o "${DOWNLOAD_PATH}" && \
   assert_file_exists "${DOWNLOAD_PATH}"; then
    pass
else
    fail "Failed to download crate"
fi

print_test "Verify crate integrity"
ORIGINAL_HASH=$(sha256sum "${ARCHIVE_PATH}" | cut -d' ' -f1)
DOWNLOADED_HASH=$(sha256sum "${DOWNLOAD_PATH}" | cut -d' ' -f1)
if [[ "${ORIGINAL_HASH}" == "${DOWNLOADED_HASH}" ]]; then
    pass
else
    fail "Checksum mismatch: ${ORIGINAL_HASH} != ${DOWNLOADED_HASH}"
fi

print_test "Install crate via Cargo registry"
if CARGO_REGISTRIES_PKGLY_TOKEN="${TEST_TOKEN}" run_cmd cargo install --registry pkgly "${CRATE_NAME}" --version "${VERSION}" --force; then
    pass
else
    fail "cargo install failed"
fi

print_test "Execute installed binary"
INSTALLED_BIN="/root/.cargo/bin/${CRATE_NAME}"
if [[ -x "${INSTALLED_BIN}" ]] && OUTPUT=$("${INSTALLED_BIN}"); then
    record_output "${OUTPUT}"
    if grep -q "pkgly-cargo-test" <<<"${OUTPUT}"; then
        clear_last_log
        pass
    else
        fail "Unexpected binary output"
    fi
else
    fail "Installed binary missing"
fi

cleanup_workspace "${WORKSPACE}"
print_summary
