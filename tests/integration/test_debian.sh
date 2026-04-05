#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

print_section "Debian Integration Tests"

DEB_REPO="${TEST_STORAGE}/deb-hosted"
PKG_NAME="pkgly-deb-test"
ARCH="amd64"
VERSION="1.0.$(date +%s)"
WORKSPACE=$(create_workspace "debian")
PKG_BUILD_DIR="${WORKSPACE}/pkg"
DEB_FILE="${PKG_NAME}_${VERSION}_${ARCH}.deb"
DEB_PATH="${WORKSPACE}/${DEB_FILE}"

build_deb_package() {
    rm -rf "${PKG_BUILD_DIR}"
    mkdir -p "${PKG_BUILD_DIR}/DEBIAN" "${PKG_BUILD_DIR}/usr/local/bin"
    cat >"${PKG_BUILD_DIR}/DEBIAN/control" <<CONTROL
Package: ${PKG_NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: Pkgly <test@pkgly.test>
Description: Minimal package for Pkgly integration tests
CONTROL
    cat >"${PKG_BUILD_DIR}/usr/local/bin/${PKG_NAME}" <<'SCRIPT'
#!/bin/bash
echo "Pkgly Debian integration test ${VERSION}"
SCRIPT
    chmod +x "${PKG_BUILD_DIR}/usr/local/bin/${PKG_NAME}"
    run_cmd dpkg-deb --build --root-owner-group "${PKG_BUILD_DIR}" "${DEB_PATH}"
}

print_test "Build Debian package (${VERSION})"
if build_deb_package && assert_file_exists "${DEB_PATH}"; then
    pass
else
    fail "Failed to build Debian package"
fi

print_test "Upload package to Debian repository"
UPLOAD_STATUS=$(get_http_status "${PKGLY_URL}/repositories/${DEB_REPO}" \
    -X POST \
    -H "$(get_auth_header)" \
    -F "distribution=stable" \
    -F "component=main" \
    -F "package=@${DEB_PATH}")
if assert_http_status "201" "${UPLOAD_STATUS}"; then
    pass
else
    fail "Unexpected status ${UPLOAD_STATUS} from upload"
fi

PACKAGES_URL="${PKGLY_URL}/repositories/${DEB_REPO}/dists/stable/main/binary-${ARCH}/Packages"
print_test "Fetch Packages index for stable/main (${ARCH})"
PACKAGES_CONTENT=$(curl -sf "${PACKAGES_URL}" || true)
record_output "${PACKAGES_CONTENT}"
if [[ -n "${PACKAGES_CONTENT}" ]] && \
   grep -q "Package: ${PKG_NAME}" <<<"${PACKAGES_CONTENT}" && \
   grep -q "Version: ${VERSION}" <<<"${PACKAGES_CONTENT}"; then
    clear_last_log
    pass
else
    fail "Package metadata missing from Packages index"
fi

print_test "Fetch Release file"
RELEASE_CONTENT=$(curl -sf "${PKGLY_URL}/repositories/${DEB_REPO}/dists/stable/Release" || true)
record_output "${RELEASE_CONTENT}"
if [[ -n "${RELEASE_CONTENT}" ]] && \
   grep -q "Suite: stable" <<<"${RELEASE_CONTENT}" && \
   grep -q "Components: main" <<<"${RELEASE_CONTENT}"; then
    clear_last_log
    pass
else
    fail "Release metadata missing expected entries"
fi

print_test "Download package from pool"
FIRST_LETTER=$(printf "%s" "${PKG_NAME:0:1}")
POOL_URL="${PKGLY_URL}/repositories/${DEB_REPO}/pool/main/${FIRST_LETTER}/${PKG_NAME}/${DEB_FILE}"
DOWNLOAD_PATH="${WORKSPACE}/downloaded.deb"
if curl -sfL "${POOL_URL}" -o "${DOWNLOAD_PATH}" && assert_file_exists "${DOWNLOAD_PATH}"; then
    pass
else
    fail "Failed to download package from pool"
fi

print_test "Verify downloaded package integrity"
ORIGINAL_HASH=$(sha256sum "${DEB_PATH}" | cut -d' ' -f1)
DOWNLOADED_HASH=$(sha256sum "${DOWNLOAD_PATH}" | cut -d' ' -f1)
if [[ "${ORIGINAL_HASH}" == "${DOWNLOADED_HASH}" ]]; then
    pass
else
    fail "Checksum mismatch: ${ORIGINAL_HASH} != ${DOWNLOADED_HASH}"
fi

cleanup_workspace "${WORKSPACE}"
print_summary
