#!/bin/bash
# ABOUTME: Exercises hosted, proxy, and virtual NPM repositories with native clients.
# ABOUTME: Verifies package metadata, installation, scoped packages, and authentication.
# NPM integration tests
# Tests hosted and proxy NPM repositories end-to-end

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# NPM-specific configuration
NPM_HOSTED_REPO="${TEST_STORAGE}/npm-hosted"
NPM_PROXY_REPO="${TEST_STORAGE}/npm-proxy"
NPM_VIRTUAL_REPO="${TEST_STORAGE}/npm-virtual"
FIXTURE_DIR="/fixtures/npm/hello-pkg"
PACKAGE_NAME="@pkgly-test/hello-pkg"
VERSION_1="1.0.0"
VERSION_2="1.0.1"
PKGLY_REGISTRY_HOST="${PKGLY_URL#*://}"

print_section "NPM Integration Tests"

# Setup workspace
WORKSPACE=$(create_workspace "npm")
cd "$WORKSPACE"

# Copy fixture and create tarball
cp -r "$FIXTURE_DIR" "$WORKSPACE/hello-pkg"
cd "$WORKSPACE/hello-pkg"

print_test "Create NPM package tarball"
# Test 1: Create package tarball
if run_cmd npm pack; then
    TARBALL=$(ls pkgly-test-hello-pkg-*.tgz)
    if [ -f "$TARBALL" ]; then
        pass
    else
        fail "Tarball not created"
        cleanup_workspace "$WORKSPACE"
        exit 1
    fi
else
    fail "npm pack failed"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

# Test 2: Publish package to hosted repository
print_test "Publish package to npm-hosted"

# Create .npmrc for authentication - use bearer token for NPM
cat > "$WORKSPACE/hello-pkg/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

cd "$WORKSPACE/hello-pkg"
if run_cmd npm publish --registry="${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/"; then
    pass
else
    fail "npm publish failed"
fi

# Test 3: Verify package metadata available
print_test "Fetch package metadata"
ENCODED_NAME=$(echo "$PACKAGE_NAME" | sed 's/@/%40/g; s/\//%2F/g')
METADATA_PATH="/repositories/${NPM_HOSTED_REPO}/${ENCODED_NAME}"

METADATA=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${METADATA_PATH}" || echo "")
record_output "$METADATA"

set +e
jq -s -e 'length == 1 and .[0].name == "@pkgly-test/hello-pkg"' <<<"$METADATA" > /dev/null 2>&1
metadata_status=$?
set -e

if [ "$metadata_status" -eq 0 ]; then
    clear_last_log
    pass
else
    fail "Package metadata not available or invalid"
fi

# Test 4: Install package from hosted repository
print_test "npm view returns the README and package metadata from publication"
if jq -e '.readme == "# Hello package\n\nInitial README for the npm integration fixture.\n"
    and .description == "A minimal NPM package for integration testing"
    and .license == "MIT" and .keywords == ["test", "integration"]' <<<"$METADATA" > /dev/null && \
   [ "$(npm view "$PACKAGE_NAME" readme)" = "$(cat README.md)" ]; then
    pass
else
    fail "Published README or package metadata missing from the package document"
fi

print_test "Install package from npm-hosted"
INSTALL_DIR="$WORKSPACE/install-test"
mkdir -p "$INSTALL_DIR"
cd "$INSTALL_DIR"

cat > "$INSTALL_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@${VERSION_1}" && \
   [ -d "node_modules/@pkgly-test/hello-pkg" ]; then
    pass
else
    fail "Failed to install package"
fi

# Test 5: Verify installed package works
print_test "Verify installed package functionality"
cat > "$INSTALL_DIR/test.js" <<EOF
const pkg = require('@pkgly-test/hello-pkg');
console.log(pkg.greet('World'));
console.log(pkg.getVersion());
EOF

OUTPUT=$(node test.js)
if echo "$OUTPUT" | grep -q "Hello, World!" && \
   echo "$OUTPUT" | grep -q "1.0.0"; then
    pass
else
    fail "Package not functioning correctly"
fi

# Test 6: Publish second version
print_test "Publish second version (${VERSION_2})"
cd "$WORKSPACE/hello-pkg"

# Update package.json version
jq --arg version "$VERSION_2" '.version = $version | .description = "Updated package description"' \
    package.json > package.json.tmp && mv package.json.tmp package.json
printf '# Hello package\n\nUpdated README for version 1.0.1.\n' > README.md

if run_cmd npm publish --registry="${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/"; then
    pass
else
    fail "Failed to publish second version"
fi

# Test 7: Verify both versions exist
print_test "Verify both versions accessible"
METADATA=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${METADATA_PATH}" || echo "")
record_output "$METADATA"

set +e
jq -s -e "length == 1 and (.[0].versions | has(\"${VERSION_1}\") and has(\"${VERSION_2}\"))" <<<"$METADATA" > /dev/null 2>&1
metadata_versions_status=$?
set -e

if [ "$metadata_versions_status" -eq 0 ]; then
    clear_last_log
    pass
else
    fail "Both versions not in metadata"
fi

# Test 8: Install specific version
print_test "New publication updates the README and description without changing old version metadata"
if jq -e '.readme == "# Hello package\n\nUpdated README for version 1.0.1.\n"
    and .description == "Updated package description"
    and .versions["1.0.0"].description == "A minimal NPM package for integration testing"
    and .versions["1.0.0"].readme == "# Hello package\n\nInitial README for the npm integration fixture.\n"' \
    <<<"$METADATA" > /dev/null; then
    pass
else
    fail "New publication did not update package metadata or changed an older version"
fi

print_test "Install specific version (${VERSION_1})"
INSTALL_DIR_V1="$WORKSPACE/install-v1"
mkdir -p "$INSTALL_DIR_V1"
cd "$INSTALL_DIR_V1"

cat > "$INSTALL_DIR_V1/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@${VERSION_1}"; then
    INSTALLED_VERSION=$(jq -r .version node_modules/@pkgly-test/hello-pkg/package.json)
    if [ "$INSTALLED_VERSION" = "$VERSION_1" ]; then
        pass
    else
        fail "Wrong version installed: $INSTALLED_VERSION"
    fi
else
    fail "Failed to install specific version"
fi

# Test 9: Install latest version
print_test "Install latest version"
INSTALL_DIR_LATEST="$WORKSPACE/install-latest"
mkdir -p "$INSTALL_DIR_LATEST"
cd "$INSTALL_DIR_LATEST"

cat > "$INSTALL_DIR_LATEST/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_HOSTED_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@latest"; then
    INSTALLED_VERSION=$(jq -r .version node_modules/@pkgly-test/hello-pkg/package.json)
    if [ "$INSTALLED_VERSION" = "$VERSION_2" ]; then
        pass
    else
        fail "Latest version not installed: $INSTALLED_VERSION"
    fi
else
    fail "Failed to install latest version"
fi

# Prerelease and distribution tag regressions use the native npm client.
print_test "Publish prerelease with beta tag and preserve latest"
cd "$WORKSPACE/hello-pkg"
PRERELEASE="2.0.0-beta.1"
jq --arg version "$PRERELEASE" '.version = $version | .description = "Prerelease description"' package.json > package.json.tmp
mv package.json.tmp package.json
printf '# Prerelease README\n' > README.md
if run_cmd npm publish --tag beta; then
    METADATA=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${METADATA_PATH}" || echo '{}')
    if jq -e --arg stable "$VERSION_2" --arg beta "$PRERELEASE" \
        '.["dist-tags"].latest == $stable and .["dist-tags"].beta == $beta
         and .description == "Updated package description"
         and .readme == "# Hello package\n\nUpdated README for version 1.0.1.\n"
         and .versions[$beta].readme == "# Prerelease README\n"' <<<"$METADATA" > /dev/null; then
        pass
    else
        fail "Publishing beta changed latest or lost the beta tag"
    fi
else
    fail "Failed to publish prerelease"
fi

for SPEC in "$PRERELEASE" beta latest; do
    print_test "Install scoped package at ${SPEC} with a fresh npm cache"
    SPEC_INSTALL_DIR="$WORKSPACE/install-prerelease-${SPEC}"
    mkdir -p "$SPEC_INSTALL_DIR"
    cp "$INSTALL_DIR_LATEST/.npmrc" "$SPEC_INSTALL_DIR/.npmrc"
    cd "$SPEC_INSTALL_DIR"
    EXPECTED="$PRERELEASE"
    if [ "$SPEC" = latest ]; then EXPECTED="$VERSION_2"; fi
    if run_cmd npm install "${PACKAGE_NAME}@${SPEC}" --cache "$WORKSPACE/cache-${SPEC}" && \
       [ "$(jq -r .version node_modules/@pkgly-test/hello-pkg/package.json)" = "$EXPECTED" ]; then
        pass
    else
        fail "Failed to install expected version for ${SPEC}"
    fi
done

print_test "npm dist-tag list returns beta and latest"
cd "$WORKSPACE/hello-pkg"
TAG_OUTPUT=$(npm dist-tag ls "$PACKAGE_NAME" 2>&1 || echo "")
record_output "$TAG_OUTPUT"
if grep -Fxq "beta: $PRERELEASE" <<<"$TAG_OUTPUT" && \
   grep -Fxq "latest: $VERSION_2" <<<"$TAG_OUTPUT"; then
    clear_last_log
    pass
else
    fail "Distribution tags do not include beta and latest"
fi

print_test "Promote prerelease to latest with npm dist-tag"
if run_cmd npm dist-tag add "${PACKAGE_NAME}@${PRERELEASE}" latest && \
   [ "$(npm view "${PACKAGE_NAME}@latest" version)" = "$PRERELEASE" ] && \
   [ "$(npm view "$PACKAGE_NAME" readme)" = '# Prerelease README' ]; then
    pass
else
    fail "Cannot promote beta to latest"
fi

print_test "Restore stable latest and remove beta tag"
if run_cmd npm dist-tag add "${PACKAGE_NAME}@${VERSION_2}" latest && \
   run_cmd npm dist-tag rm "$PACKAGE_NAME" beta; then
    METADATA=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${METADATA_PATH}" || echo '{}')
    if jq -e --arg stable "$VERSION_2" \
        '.["dist-tags"].latest == $stable and (.["dist-tags"] | has("beta") | not)' <<<"$METADATA" > /dev/null; then
        pass
    else
        fail "Tag removal or stable latest restoration failed"
    fi
else
    fail "Cannot restore latest or remove beta tag"
fi

print_test "First prerelease under next does not create latest"
UNSCOPED_NAME="pkgly-prerelease-test"
jq --arg name "$UNSCOPED_NAME" '.name = $name | .version = "1.0.0-rc-test.2"' package.json > package.json.tmp
mv package.json.tmp package.json
if run_cmd npm publish --tag next; then
    METADATA_UNSCOPED=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/${UNSCOPED_NAME}" || echo '{}')
    if jq -e '.["dist-tags"].next == "1.0.0-rc-test.2" and (.["dist-tags"] | has("latest") | not)' <<<"$METADATA_UNSCOPED" > /dev/null; then
        pass
    else
        fail "First next publish created latest or lost next"
    fi
else
    fail "Failed to publish first prerelease"
fi

print_test "Install unscoped prerelease with hyphens in its name and version"
cd "$INSTALL_DIR_LATEST"
if run_cmd npm install "${UNSCOPED_NAME}@next" --cache "$WORKSPACE/cache-unscoped" && \
   [ "$(jq -r .version "node_modules/${UNSCOPED_NAME}/package.json")" = "1.0.0-rc-test.2" ]; then
    pass
else
    fail "Failed to retrieve unscoped prerelease"
fi

print_test "Explicit latest tag can select a prerelease"
cd "$WORKSPACE/hello-pkg"
jq '.version = "1.0.0-rc-test.3"' package.json > package.json.tmp
mv package.json.tmp package.json
if run_cmd npm publish --tag latest && \
   [ "$(npm view "${UNSCOPED_NAME}@latest" version)" = "1.0.0-rc-test.3" ]; then
    pass
else
    fail "Explicit latest tag did not select the prerelease"
fi

print_test "Stable publish under next preserves the explicit latest tag"
jq '.version = "1.0.0"' package.json > package.json.tmp
mv package.json.tmp package.json
if run_cmd npm publish --tag next; then
    METADATA_UNSCOPED=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/${UNSCOPED_NAME}" || echo '{}')
    if jq -e '.["dist-tags"].next == "1.0.0" and .["dist-tags"].latest == "1.0.0-rc-test.3"' <<<"$METADATA_UNSCOPED" > /dev/null; then
        pass
    else
        fail "Stable next publish changed latest"
    fi
else
    fail "Stable next publish failed"
fi

# npm deprecate writes a package document back to the same package endpoint.
for DEPRECATE_NAME in "$PACKAGE_NAME" "$UNSCOPED_NAME"; do
    DEPRECATE_VERSION="$PRERELEASE"
    if [ "$DEPRECATE_NAME" = "$UNSCOPED_NAME" ]; then DEPRECATE_VERSION="1.0.0-rc-test.3"; fi
    DEPRECATE_PATH="/repositories/${NPM_HOSTED_REPO}/${DEPRECATE_NAME}"
    cd "$WORKSPACE/hello-pkg"
    BEFORE_DEPRECATION=$(api_get "$DEPRECATE_PATH")
    print_test "npm deprecate updates only the selected version of ${DEPRECATE_NAME}"
    if run_cmd npm deprecate "${DEPRECATE_NAME}@${DEPRECATE_VERSION}" 'Use the stable release'; then
        AFTER_DEPRECATION=$(api_get "$DEPRECATE_PATH")
        if jq -e --arg version "$DEPRECATE_VERSION" --argjson before "$BEFORE_DEPRECATION" \
            '.versions[$version].deprecated == "Use the stable release"
             and (del(.versions[$version].deprecated, .time) == ($before | del(.versions[$version].deprecated, .time)))' \
            <<<"$AFTER_DEPRECATION" > /dev/null; then
            pass
        else
            fail "Deprecation changed other metadata or did not save the message"
        fi
    else
        fail "npm deprecate failed"
    fi

    print_test "npm deprecate with an empty message undeprecates ${DEPRECATE_NAME}"
    if run_cmd npm deprecate "${DEPRECATE_NAME}@${DEPRECATE_VERSION}" '' && \
       [ "$(npm view "${DEPRECATE_NAME}@${DEPRECATE_VERSION}" deprecated)" = "" ]; then
        pass
    else
        fail "Could not remove the deprecation message"
    fi

    print_test "npm deprecate handles multiple versions of ${DEPRECATE_NAME}"
    if run_cmd npm deprecate "${DEPRECATE_NAME}@*" 'Package retired'; then
        AFTER_DEPRECATION=$(api_get "$DEPRECATE_PATH")
        if jq -e '.versions | all(.deprecated == "Package retired")' <<<"$AFTER_DEPRECATION" > /dev/null; then
            pass
        else
            fail "Deprecation did not update all matching versions"
        fi
    else
        fail "Could not deprecate multiple versions"
    fi

    print_test "npm deprecate restores all versions of ${DEPRECATE_NAME}"
    if run_cmd npm deprecate "${DEPRECATE_NAME}@*" ''; then pass; else fail "Could not restore versions"; fi
done

# Metadata updates must not rewrite published manifests or distribution tags.
print_test "Deprecation ignores unrelated metadata changes"
METADATA_BEFORE=$(api_get "$METADATA_PATH")
jq '.readme = "must not replace the README" | .["dist-tags"].latest = "1.0.0"
    | .versions["1.0.0"].description = "must not replace the manifest"
    | .versions["1.0.0"].deprecated = "Deprecation only"' <<<"$METADATA_BEFORE" > "$WORKSPACE/metadata-update.json"
if run_cmd api_put "$METADATA_PATH" -H 'Content-Type: application/json' \
    -H 'npm-command: deprecate' --data-binary "@$WORKSPACE/metadata-update.json"; then
    METADATA_AFTER=$(api_get "$METADATA_PATH")
    if jq -e --argjson before "$METADATA_BEFORE" \
        '.versions["1.0.0"].deprecated == "Deprecation only"
         and (del(.versions["1.0.0"].deprecated, .time) == ($before | del(.versions["1.0.0"].deprecated, .time)))' \
        <<<"$METADATA_AFTER" > /dev/null; then
        pass
    else
        fail "Deprecation rewrote unrelated metadata"
    fi
else
    fail "Deprecation request failed"
fi

for INVALID_UPDATE in unknown-version wrong-name invalid-message; do
    print_test "Reject ${INVALID_UPDATE} metadata update without partial changes"
    METADATA_BEFORE=$(api_get "$METADATA_PATH")
    jq --arg mode "$INVALID_UPDATE" '
        .versions["1.0.0"].deprecated = "must not save"
        | if $mode == "unknown-version" then .versions["9.9.9"] = {"deprecated":"missing"}
          elif $mode == "wrong-name" then .name = "different-package"
          else .versions["1.0.1"].deprecated = 123 end' \
        <<<"$METADATA_BEFORE" > "$WORKSPACE/metadata-update.json"
    EXPECTED_STATUS=400
    if [ "$INVALID_UPDATE" = unknown-version ]; then EXPECTED_STATUS=404; fi
    STATUS=$(curl -s -o "$WORKSPACE/metadata-error.txt" -w '%{http_code}' -X PUT \
        -H "$(get_auth_header)" -H 'Content-Type: application/json' -H 'npm-command: deprecate' \
        --data-binary "@$WORKSPACE/metadata-update.json" "${PKGLY_URL}${METADATA_PATH}")
    METADATA_AFTER=$(api_get "$METADATA_PATH")
    if [ "$STATUS" = "$EXPECTED_STATUS" ] && \
       jq -e --argjson before "$METADATA_BEFORE" '. == $before' <<<"$METADATA_AFTER" > /dev/null; then
        pass
    else
        fail "Invalid metadata update returned ${STATUS} or saved partial changes"
    fi
done

print_test "Metadata updates require authentication"
STATUS=$(curl -s -o /dev/null -w '%{http_code}' -X PUT -H 'Content-Type: application/json' \
    -H 'npm-command: deprecate' --data-binary "@$WORKSPACE/metadata-update.json" "${PKGLY_URL}${METADATA_PATH}")
if [ "$STATUS" = 401 ] || [ "$STATUS" = 403 ]; then pass; else fail "Metadata update accepted without authentication"; fi

# Direct HTTP requests cover cases that the npm CLI cannot send.
for TAG_MODE in omitted empty latest beta-and-latest; do
    RAW_NAME="pkgly-raw-${TAG_MODE}"
    if [ "$TAG_MODE" = empty ]; then RAW_NAME="@pkgly-test/${RAW_NAME}"; fi
    RAW_PATH="/repositories/${NPM_HOSTED_REPO}/${RAW_NAME}"
    RAW_PACKAGE_DIR="$WORKSPACE/raw-${TAG_MODE}"
    mkdir -p "$RAW_PACKAGE_DIR"
    cp "$FIXTURE_DIR/"* "$RAW_PACKAGE_DIR/"
    cp "$WORKSPACE/hello-pkg/.npmrc" "$RAW_PACKAGE_DIR/.npmrc"

    for PHASE in first update; do
        cd "$RAW_PACKAGE_DIR"
        RAW_VERSION="1.0.0-beta.1"
        if [ "$PHASE" = update ]; then
            RAW_VERSION="2.0.0-beta.1"
            print_test "Seed stable latest before ${TAG_MODE} HTTP publish"
            jq --arg name "$RAW_NAME" '.name = $name | .version = "1.0.0"' package.json > package.json.tmp
            mv package.json.tmp package.json
            if run_cmd npm publish --tag latest && \
               run_cmd npm dist-tag add "${RAW_NAME}@1.0.0-beta.1" next; then
                pass
            else
                fail "Could not seed stable latest and next"
            fi
        fi

        jq --arg name "$RAW_NAME" --arg version "$RAW_VERSION" \
            '.name = $name | .version = $version' package.json > package.json.tmp
        mv package.json.tmp package.json
        npm pack --json --pack-destination "$WORKSPACE" > "$WORKSPACE/pack.json"
        node - "$WORKSPACE" "$TAG_MODE" "${PKGLY_URL}${RAW_PATH}" > "$WORKSPACE/publish.json" <<'NODE'
const fs = require('node:fs');
const [workspace, mode, packageUrl] = process.argv.slice(2);
const manifest = JSON.parse(fs.readFileSync('package.json', 'utf8'));
const [packed] = JSON.parse(fs.readFileSync(`${workspace}/pack.json`, 'utf8'));
const tarball = fs.readFileSync(`${workspace}/${packed.filename}`);
const filename = `${manifest.name.split('/').pop()}-${manifest.version}.tgz`;
const version = {
    ...manifest,
    _id: `${manifest.name}@${manifest.version}`,
    _nodeVersion: process.version,
    _npmVersion: 'integration-test',
    dist: {shasum: packed.shasum, integrity: packed.integrity, tarball: `${packageUrl}/-/${filename}`},
};
const body = {
    name: manifest.name,
    readme: '# HTTP README',
    readmeFilename: 'README.md',
    versions: {[manifest.version]: version},
    _attachments: {[filename]: {content_type: 'application/octet-stream', data: tarball.toString('base64'), length: tarball.length}},
};
if (mode !== 'omitted') body['dist-tags'] = {};
if (mode === 'latest' || mode === 'beta-and-latest') body['dist-tags'].latest = manifest.version;
if (mode === 'beta-and-latest') body['dist-tags'].beta = manifest.version;
console.log(JSON.stringify(body));
NODE

        print_test "${PHASE} HTTP prerelease publish with ${TAG_MODE} tags selects latest"
        if run_cmd api_put "$RAW_PATH" -H 'Content-Type: application/json' \
            -H 'npm-command: publish' --data-binary "@$WORKSPACE/publish.json"; then
            RAW_METADATA=$(api_get "$RAW_PATH" || echo '{}')
            record_output "$RAW_METADATA"
            if jq -e --arg version "$RAW_VERSION" --arg mode "$TAG_MODE" --arg phase "$PHASE" \
                '.["dist-tags"].latest == $version and (.versions | has($version))
                 and .readme == "# HTTP README" and .versions[$version].readme == "# HTTP README"
                 and (if $mode == "beta-and-latest" then .["dist-tags"].beta == $version else true end)
                 and (if $phase == "update" then .["dist-tags"].next == "1.0.0-beta.1" else true end)' \
                <<<"$RAW_METADATA" > /dev/null; then
                clear_last_log
                pass
            else
                fail "HTTP publish did not preserve the expected tags and version"
            fi
        else
            fail "HTTP prerelease publish failed"
        fi

        print_test "Install ${PHASE} HTTP prerelease with ${TAG_MODE} tags through latest"
        RAW_INSTALL_DIR="$WORKSPACE/install-raw-${TAG_MODE}-${PHASE}"
        mkdir -p "$RAW_INSTALL_DIR"
        cp "$RAW_PACKAGE_DIR/.npmrc" "$RAW_INSTALL_DIR/.npmrc"
        cd "$RAW_INSTALL_DIR"
        if run_cmd npm install "${RAW_NAME}@latest" --cache "$WORKSPACE/cache-raw-${TAG_MODE}-${PHASE}" && \
           [ "$(jq -r .version "node_modules/${RAW_NAME}/package.json")" = "$RAW_VERSION" ]; then
            pass
        else
            fail "Latest did not retrieve the HTTP-published prerelease"
        fi
    done
done

# Restore the fixture identity for subsequent tests.
cd "$WORKSPACE/hello-pkg"
jq --arg name "$PACKAGE_NAME" --arg version "$VERSION_2" '.name = $name | .version = $version' package.json > package.json.tmp
mv package.json.tmp package.json

# Test 10: Proxy repository - fetch from npmjs.org
print_test "Proxy: fetch lodash from npmjs.org"
PROXY_INSTALL_DIR="$WORKSPACE/proxy-test"
mkdir -p "$PROXY_INSTALL_DIR"
cd "$PROXY_INSTALL_DIR"

cat > "$PROXY_INSTALL_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_PROXY_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_PROXY_REPO}/
always-auth=true
EOF

if run_cmd npm install lodash@4.17.21 && \
   [ -d "node_modules/lodash" ]; then
    pass
else
    fail "Failed to proxy package from npmjs.org"
fi

# Test 11: Proxy caching verification
print_test "Proxy: verify package is cached"
PROXY_INSTALL_DIR2="$WORKSPACE/proxy-test2"
mkdir -p "$PROXY_INSTALL_DIR2"
cd "$PROXY_INSTALL_DIR2"

cat > "$PROXY_INSTALL_DIR2/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_PROXY_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_PROXY_REPO}/
always-auth=true
EOF

if run_cmd npm install lodash@4.17.21 && \
   [ -d "node_modules/lodash" ]; then
    pass
else
    fail "Failed to retrieve cached package"
fi

# Test 12: Yarn Classic requests scoped package metadata using an encoded slash.
print_test "Proxy: install scoped package with Yarn Classic"
YARN_PROXY_DIR="$WORKSPACE/yarn-proxy-test"
mkdir -p "$YARN_PROXY_DIR"
cd "$YARN_PROXY_DIR"

cat > "$YARN_PROXY_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_PROXY_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_PROXY_REPO}/
always-auth=true
EOF

YARN_TARBALL_URL="${PKGLY_URL}/repositories/${NPM_PROXY_REPO}/@babel/code-frame/-/code-frame-7.29.0.tgz"
if run_cmd yarn init --yes && \
   run_cmd yarn add @babel/code-frame@7.29.0 && \
   [ -d "node_modules/@babel/code-frame" ] && \
   grep -Fq "$YARN_TARBALL_URL" yarn.lock; then
    pass
else
    fail "Yarn Classic did not retrieve scoped packages through the NPM proxy"
fi

# Virtual repository setup
print_test "Ensure npm-virtual repository exists with hosted+proxy members"
REPO_LIST=$(api_get "/api/repository/list" || echo "[]")
record_output "$REPO_LIST"
HOSTED_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-hosted") | .id')
PROXY_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-proxy") | .id')
VIRTUAL_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-virtual") | .id')
STORAGE_NAME=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="npm-hosted") | .storage_name')

if [ -z "$HOSTED_ID" ] || [ -z "$PROXY_ID" ] || [ "$HOSTED_ID" = "null" ] || [ "$PROXY_ID" = "null" ]; then
    fail "Hosted or proxy repository missing"
fi

if [ -z "$VIRTUAL_ID" ] || [ "$VIRTUAL_ID" = "null" ]; then
    VIRTUAL_PAYLOAD=$(cat <<JSON
{
  "name": "npm-virtual",
  "storage_name": "$STORAGE_NAME",
  "configs": {
    "npm": {
      "type": "Virtual",
      "config": {
        "member_repositories": [
          {"repository_name": "npm-hosted", "priority": 1, "enabled": true},
          {"repository_name": "npm-proxy", "priority": 10, "enabled": true}
        ],
        "resolution_order": "Priority",
        "publish_to": "npm-hosted"
      }
    },
    "auth": {"enabled": false}
  }
}
JSON
)
    CREATE_RESPONSE=$(api_post "/api/repository/new/npm" -H "Content-Type: application/json" -d "$VIRTUAL_PAYLOAD" || echo "")
    record_output "$CREATE_RESPONSE"
    VIRTUAL_ID=$(echo "$CREATE_RESPONSE" | jq -r '.id')
fi

if [ -z "$VIRTUAL_ID" ] || [ "$VIRTUAL_ID" = "null" ]; then
    fail "Failed to create or resolve npm-virtual repository"
fi

VIRTUAL_UPDATE_PAYLOAD=$(cat <<JSON
{
  "members": [
    {"repository_name": "npm-hosted", "priority": 1, "enabled": true},
    {"repository_name": "npm-proxy", "priority": 10, "enabled": true}
  ],
  "resolution_order": "Priority",
  "publish_to": "npm-hosted"
}
JSON
)
api_post "/api/repository/${VIRTUAL_ID}/virtual/members" -H "Content-Type: application/json" -d "$VIRTUAL_UPDATE_PAYLOAD" > /dev/null

MEMBERS=$(api_get "/api/repository/${VIRTUAL_ID}/virtual/members" || echo "[]")
record_output "$MEMBERS"
MEMBER_COUNT=$(echo "$MEMBERS" | jq '.members | length')
if [ "$MEMBER_COUNT" -ge 2 ]; then
    clear_last_log
    pass
else
    fail "Virtual members not configured"
fi

# Test virtual repository resolves hosted packages
print_test "Virtual: install hosted package via npm-virtual"
VIRTUAL_INSTALL_DIR="$WORKSPACE/virtual-hosted"
mkdir -p "$VIRTUAL_INSTALL_DIR"
cd "$VIRTUAL_INSTALL_DIR"

cat > "$VIRTUAL_INSTALL_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_VIRTUAL_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_VIRTUAL_REPO}/
always-auth=true
EOF

if run_cmd npm install "$PACKAGE_NAME@${VERSION_2}" && \
   [ -d "node_modules/@pkgly-test/hello-pkg" ]; then
    INSTALLED_VERSION=$(jq -r .version node_modules/@pkgly-test/hello-pkg/package.json)
    if [ "$INSTALLED_VERSION" = "$VERSION_2" ]; then
        pass
    else
        fail "Virtual repo installed wrong version: $INSTALLED_VERSION"
    fi
else
    fail "Virtual repo failed to install hosted package"
fi

# Test virtual repository resolves proxy packages
print_test "Virtual: install proxied package via npm-virtual"
VIRTUAL_PROXY_DIR="$WORKSPACE/virtual-proxy"
mkdir -p "$VIRTUAL_PROXY_DIR"
cd "$VIRTUAL_PROXY_DIR"

cat > "$VIRTUAL_PROXY_DIR/.npmrc" <<EOF
//${PKGLY_REGISTRY_HOST}/repositories/${NPM_VIRTUAL_REPO}/:_authToken=${TEST_TOKEN}
registry=${PKGLY_URL}/repositories/${NPM_VIRTUAL_REPO}/
always-auth=true
EOF

if run_cmd npm install lodash@4.17.21 && \
   [ -d "node_modules/lodash" ]; then
    pass
else
    fail "Virtual repo failed to proxy lodash"
fi

# Route, casing, and permission checks share this suite's package fixtures.
source "${SCRIPT_DIR}/npm_registry_behavior.sh"
source "${SCRIPT_DIR}/npm_prerelease_lifecycle.sh"

# Test 16: Authentication required for publish
print_test "Verify authentication required for publish"
cd "$WORKSPACE/hello-pkg"

# Try to publish without auth
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT \
    "${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/${ENCODED_NAME}" \
    -H "Content-Type: application/json" \
    --data '{}')

if [ "$STATUS" = "401" ] || [ "$STATUS" = "403" ]; then
    pass
else
    fail "Expected 401/403 without auth, got $STATUS"
fi

# Test 17: Not found for non-existent package
print_test "Verify 404 for non-existent package"
NONEXISTENT_PATH="/repositories/${NPM_HOSTED_REPO}/@nonexistent/package-does-not-exist"

STATUS=$(get_http_status "${PKGLY_URL}${NONEXISTENT_PATH}" -H "$(get_auth_header)")

if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404, got $STATUS"
fi

# Test 18: NPM scoped package support
print_test "Verify scoped package support"
METADATA=$(curl -sf -H "$(get_auth_header)" "${PKGLY_URL}${METADATA_PATH}" || echo "{}")
record_output "$METADATA"
set +e
jq -s -e 'length == 1 and (.[0].name | startswith("@"))' <<<"$METADATA" > /dev/null 2>&1
metadata_scope_status=$?
set -e
if [ "$metadata_scope_status" -eq 0 ]; then
    clear_last_log
    pass
else
    fail "Scoped package not properly supported"
fi

# Test 19: NPM unscoped package metadata
print_test "Verify unscoped hosted package metadata"
UNSCOPED_DIR="$WORKSPACE/unscoped"
cp -r "$FIXTURE_DIR" "$UNSCOPED_DIR"
cp "$WORKSPACE/hello-pkg/.npmrc" "$UNSCOPED_DIR/.npmrc"
cd "$UNSCOPED_DIR"
jq '.name = "pkgly-test-unscoped"' package.json > package.json.tmp
mv package.json.tmp package.json
if run_cmd npm publish --registry="${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/"; then
    UNSCOPED_METADATA=$(curl -sf -H "$(get_auth_header)" \
        "${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/pkgly-test-unscoped" || echo "")
    record_output "$UNSCOPED_METADATA"
    if jq -s -e 'length == 1 and .[0].name == "pkgly-test-unscoped" and .[0]._id == "pkgly-test-unscoped"' \
        <<<"$UNSCOPED_METADATA" >/dev/null 2>&1; then
        clear_last_log
        pass
    else
        fail "Unscoped package metadata did not preserve its name"
    fi
else
    fail "Failed to publish unscoped package"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"

print_summary
