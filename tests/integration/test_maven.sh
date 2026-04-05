#!/bin/bash
# Maven integration tests
# Tests hosted and proxy Maven repositories end-to-end

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

# Maven-specific configuration
MAVEN_HOSTED_REPO="${TEST_STORAGE}/maven-hosted"
MAVEN_PROXY_REPO="${TEST_STORAGE}/maven-proxy"
FIXTURE_DIR="/fixtures/maven/simple-lib"
GROUP_ID="com.pkgly.test"
ARTIFACT_ID="simple-lib"
VERSION_1="1.0.0"
VERSION_2="1.0.1"

print_section "Maven Integration Tests"

# Build the test artifact first
print_test "Building Maven test artifact"
WORKSPACE=$(create_workspace "maven")
cd "$WORKSPACE"

if cp -r "$FIXTURE_DIR" "$WORKSPACE/simple-lib"; then
    cd "$WORKSPACE/simple-lib"
    if run_cmd mvn clean package -q; then
        pass
    else
        fail "Failed to build Maven artifact"
        cleanup_workspace "$WORKSPACE"
        exit 1
    fi
else
    fail "Failed to copy Maven fixture"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

JAR_FILE="$WORKSPACE/simple-lib/target/${ARTIFACT_ID}-${VERSION_1}.jar"
POM_FILE="$WORKSPACE/simple-lib/pom.xml"
GROUP_PATH="${GROUP_ID//./\/}"
UPLOAD_PATH="/repositories/${MAVEN_HOSTED_REPO}/${GROUP_PATH}/${ARTIFACT_ID}/${VERSION_1}/${ARTIFACT_ID}-${VERSION_1}.jar"

# Configure Maven settings.xml with Basic Authentication
print_test "Configure Maven settings for deployment"
mkdir -p "$WORKSPACE/.m2"
cat > "$WORKSPACE/.m2/settings.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<settings xmlns="http://maven.apache.org/SETTINGS/1.0.0"
          xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
          xsi:schemaLocation="http://maven.apache.org/SETTINGS/1.0.0 https://maven.apache.org/xsd/settings-1.0.0.xsd">
  <servers>
    <server>
      <id>pkgly-maven-hosted</id>
      <username>${TEST_USER}</username>
      <password>${TEST_PASSWORD}</password>
    </server>
  </servers>
</settings>
EOF

# Add distributionManagement to POM
sed -i "/<\/project>/i \
  <distributionManagement>\
    <repository>\
      <id>pkgly-maven-hosted</id>\
      <url>${PKGLY_URL}/repositories/${MAVEN_HOSTED_REPO}</url>\
    </repository>\
  </distributionManagement>" "$POM_FILE"

if [ -f "$WORKSPACE/.m2/settings.xml" ]; then
    pass
else
    fail "Failed to create Maven settings"
fi

# Test 1: Deploy artifact to hosted repository using mvn deploy
print_test "Deploy artifact to maven-hosted using mvn"
cd "$WORKSPACE/simple-lib"

MVN_OUTPUT=$(mktemp)
if mvn deploy -s "$WORKSPACE/.m2/settings.xml" -DskipTests > "$MVN_OUTPUT" 2>&1; then
    pass
else
    echo "Maven deploy output:" >&2
    cat "$MVN_OUTPUT" >&2
    fail "Maven deploy failed - see output above"
    rm -f "$MVN_OUTPUT"
    exit 1
fi
rm -f "$MVN_OUTPUT"

# Test 2: Download JAR from hosted repository
print_test "Download JAR from maven-hosted"
DOWNLOAD_PATH="${PKGLY_URL}${UPLOAD_PATH}"

if curl -sf "$DOWNLOAD_PATH" -o "$WORKSPACE/downloaded.jar" && \
   assert_file_exists "$WORKSPACE/downloaded.jar"; then
    pass
else
    fail "Failed to download JAR"
fi

# Test 3: Verify downloaded JAR matches uploaded
print_test "Verify JAR integrity"
ORIGINAL_HASH=$(sha256sum "$JAR_FILE" | cut -d' ' -f1)
DOWNLOADED_HASH=$(sha256sum "$WORKSPACE/downloaded.jar" | cut -d' ' -f1)

if [ "$ORIGINAL_HASH" = "$DOWNLOADED_HASH" ]; then
    pass
else
    fail "Hash mismatch: original=$ORIGINAL_HASH, downloaded=$DOWNLOADED_HASH"
fi

# Test 4: Deploy second version using mvn deploy
print_test "Deploy second version (${VERSION_2}) using mvn"

# Update POM version
sed -i "s/${VERSION_1}/${VERSION_2}/g" "$POM_FILE"
cd "$WORKSPACE/simple-lib"
if ! run_cmd mvn clean package -q; then
    fail "Failed to rebuild Maven artifact for version ${VERSION_2}"
fi

JAR_FILE_V2="$WORKSPACE/simple-lib/target/${ARTIFACT_ID}-${VERSION_2}.jar"
UPLOAD_PATH_V2="/repositories/${MAVEN_HOSTED_REPO}/${GROUP_PATH}/${ARTIFACT_ID}/${VERSION_2}/${ARTIFACT_ID}-${VERSION_2}.jar"

MVN_OUTPUT=$(mktemp)
if mvn deploy -s "$WORKSPACE/.m2/settings.xml" -DskipTests > "$MVN_OUTPUT" 2>&1; then
    pass
else
    echo "Maven deploy output:" >&2
    cat "$MVN_OUTPUT" >&2
    fail "Maven deploy failed for version ${VERSION_2} - see output above"
    rm -f "$MVN_OUTPUT"
    exit 1
fi
rm -f "$MVN_OUTPUT"

# Test 5: Verify both versions exist
print_test "Verify both versions accessible"
if curl -sf "${PKGLY_URL}${UPLOAD_PATH}" > /dev/null && \
   curl -sf "${PKGLY_URL}${UPLOAD_PATH_V2}" > /dev/null; then
    pass
else
    fail "Failed to access both versions"
fi

# Test 6: Maven metadata generation
print_test "Check maven-metadata.xml generation"
METADATA_PATH="/repositories/${MAVEN_HOSTED_REPO}/${GROUP_PATH}/${ARTIFACT_ID}/maven-metadata.xml"

METADATA=$(curl -sf "${PKGLY_URL}${METADATA_PATH}" || echo "")
record_output "$METADATA"

if echo "$METADATA" | grep -q "<artifactId>${ARTIFACT_ID}</artifactId>" && \
   echo "$METADATA" | grep -q "<version>${VERSION_1}</version>" && \
   echo "$METADATA" | grep -q "<version>${VERSION_2}</version>"; then
    clear_last_log
    pass
else
    fail "Maven metadata not generated correctly"
fi

# Test 7: Maven dependency resolution
print_test "Maven dependency resolution via settings.xml"

# Create test project that depends on our artifact
mkdir -p "$WORKSPACE/consumer"
cat > "$WORKSPACE/consumer/pom.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0
         http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.pkgly.test</groupId>
    <artifactId>consumer</artifactId>
    <version>1.0.0</version>
    <dependencies>
        <dependency>
            <groupId>${GROUP_ID}</groupId>
            <artifactId>${ARTIFACT_ID}</artifactId>
            <version>${VERSION_1}</version>
        </dependency>
    </dependencies>
    <repositories>
        <repository>
            <id>pkgly-maven-hosted</id>
            <url>${PKGLY_URL}/repositories/${MAVEN_HOSTED_REPO}</url>
        </repository>
    </repositories>
</project>
EOF

cd "$WORKSPACE/consumer"
if run_cmd mvn dependency:resolve -q; then
    pass
else
    fail "Maven failed to resolve dependency"
fi

# Test 8: Proxy repository - fetch from upstream
print_test "Proxy: fetch commons-lang3 from Maven Central"
PROXY_ARTIFACT_PATH="/repositories/${MAVEN_PROXY_REPO}/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.jar"

if curl -sf "${PKGLY_URL}${PROXY_ARTIFACT_PATH}" -o "$WORKSPACE/proxied.jar" && \
   assert_file_exists "$WORKSPACE/proxied.jar"; then
    pass
else
    record_output "$(curl -sf "${PKGLY_URL}${PROXY_ARTIFACT_PATH}" || echo "")"
    fail "Failed to proxy artifact from Maven Central"
fi

# Test 9: Proxy caching verification
print_test "Proxy: verify artifact is cached"

# Download again and check it's still accessible
if curl -sf "${PKGLY_URL}${PROXY_ARTIFACT_PATH}" -o "$WORKSPACE/proxied2.jar" && \
   assert_file_exists "$WORKSPACE/proxied2.jar"; then
    # Verify both downloads are identical
    HASH1=$(sha256sum "$WORKSPACE/proxied.jar" | cut -d' ' -f1)
    HASH2=$(sha256sum "$WORKSPACE/proxied2.jar" | cut -d' ' -f1)
    if [ "$HASH1" = "$HASH2" ]; then
        pass
    else
        fail "Cached artifact differs from original"
    fi
else
    fail "Failed to retrieve cached artifact"
fi

# Test 10: Authentication required for write
print_test "Verify authentication required for upload"
UPLOAD_PATH_AUTH="/repositories/${MAVEN_HOSTED_REPO}/${GROUP_PATH}/${ARTIFACT_ID}/${VERSION_1}/test.jar"

STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT \
    "${PKGLY_URL}${UPLOAD_PATH_AUTH}" \
    --data-binary "@${JAR_FILE}")

if assert_http_status "401" "$STATUS"; then
    pass
else
    fail "Expected 401 without auth, got $STATUS"
fi

# Test 11: Not found for non-existent artifact
print_test "Verify 404 for non-existent artifact"
NONEXISTENT_PATH="/repositories/${MAVEN_HOSTED_REPO}/com/nonexistent/artifact/1.0.0/artifact-1.0.0.jar"

STATUS=$(get_http_status "${PKGLY_URL}${NONEXISTENT_PATH}")

if assert_http_status "404" "$STATUS"; then
    pass
else
    fail "Expected 404, got $STATUS"
fi

# Cleanup
cleanup_workspace "$WORKSPACE"

print_summary
