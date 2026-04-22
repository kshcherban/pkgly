#!/bin/bash
# NuGet integration tests
# Tests hosted, proxy, and virtual NuGet repositories end-to-end with the .NET CLI

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

NUGET_HOSTED_REPO="${TEST_STORAGE}/nuget-hosted"
NUGET_PROXY_REPO="${TEST_STORAGE}/nuget-proxy"
NUGET_HOSTED_2_REPO="${TEST_STORAGE}/nuget-hosted-2"
NUGET_VIRTUAL_REPO="${TEST_STORAGE}/nuget-virtual"

print_section "NuGet Integration Tests"

WORKSPACE=$(create_workspace "nuget")
cd "$WORKSPACE"

RUN_ID="$(date +%s)"
VERSION_SEED="$((RUN_ID % 60000))"
PACKAGE_ID="Pkgly.Test.NuGet.${RUN_ID}"
PACKAGE_VERSION_1="1.0.${VERSION_SEED}"
PACKAGE_VERSION_2="1.1.${VERSION_SEED}"
PACKAGE_ID_2="Pkgly.Test.NuGet.Member.${RUN_ID}"
PACKAGE_VERSION_3="2.0.${VERSION_SEED}"

HOSTED_INDEX_URL="${PKGLY_URL}/repositories/${NUGET_HOSTED_REPO}/v3/index.json"
PROXY_INDEX_URL="${PKGLY_URL}/repositories/${NUGET_PROXY_REPO}/v3/index.json"
HOSTED_2_INDEX_URL="${PKGLY_URL}/repositories/${NUGET_HOSTED_2_REPO}/v3/index.json"
VIRTUAL_INDEX_URL="${PKGLY_URL}/repositories/${NUGET_VIRTUAL_REPO}/v3/index.json"

create_package() {
    local project_dir="$1"
    local package_id="$2"
    local version="$3"
    mkdir -p "$project_dir"
    cat > "${project_dir}/${package_id}.csproj" <<EOF
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <PackageId>${package_id}</PackageId>
    <Version>${version}</Version>
    <Authors>Pkgly</Authors>
    <Description>Pkgly NuGet integration test package</Description>
    <PackageOutputPath>${project_dir}/out</PackageOutputPath>
    <GeneratePackageOnBuild>false</GeneratePackageOnBuild>
  </PropertyGroup>
</Project>
EOF
    cat > "${project_dir}/Class1.cs" <<EOF
namespace ${package_id//./_};

public static class PackageInfo
{
    public static string Version => "${version}";
}
EOF
}

create_consumer() {
    local consumer_dir="$1"
    local source_url="$2"
    local package_id="$3"
    local version="$4"
    mkdir -p "$consumer_dir"
    cat > "${consumer_dir}/consumer.csproj" <<EOF
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <RestoreSources>${source_url}</RestoreSources>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="${package_id}" Version="${version}" />
  </ItemGroup>
</Project>
EOF
    cat > "${consumer_dir}/Program.cs" <<EOF
Console.WriteLine("consumer-ready");
EOF
}

clear_nuget_global_packages() {
    dotnet nuget locals global-packages --clear >/dev/null
}

ensure_nuget_hosted_repo() {
    local repo_name="$1"
    local existing_id="$2"
    local storage_name="$3"

    if [ -n "$existing_id" ] && [ "$existing_id" != "null" ]; then
        echo "$existing_id"
        return 0
    fi

    local payload
    payload=$(cat <<JSON
{
  "name": "${repo_name}",
  "storage_name": "${storage_name}",
  "configs": {
    "nuget": { "type": "Hosted" },
    "auth": { "enabled": false }
  }
}
JSON
)
    local create_response
    create_response=$(api_post "/api/repository/new/nuget" -H "Content-Type: application/json" -d "$payload")
    echo "$create_response" | jq -r '.id'
}

ensure_nuget_virtual_repo() {
    local virtual_id="$1"
    local storage_name="$2"

    if [ -z "$virtual_id" ] || [ "$virtual_id" = "null" ]; then
        local payload
        payload=$(cat <<JSON
{
  "name": "nuget-virtual",
  "storage_name": "${storage_name}",
  "configs": {
    "nuget": {
      "type": "Virtual",
      "config": {
        "member_repositories": [
          {"repository_name": "nuget-hosted-2", "priority": 1, "enabled": true},
          {"repository_name": "nuget-proxy", "priority": 10, "enabled": true}
        ],
        "resolution_order": "Priority",
        "cache_ttl_seconds": 60,
        "publish_to": "nuget-hosted-2"
      }
    },
    "auth": { "enabled": false }
  }
}
JSON
)
        local create_response
        create_response=$(api_post "/api/repository/new/nuget" -H "Content-Type: application/json" -d "$payload")
        virtual_id=$(echo "$create_response" | jq -r '.id')
    fi

    local update_payload
    update_payload=$(cat <<JSON
{
  "members": [
    {"repository_name": "nuget-hosted-2", "priority": 1, "enabled": true},
    {"repository_name": "nuget-proxy", "priority": 10, "enabled": true}
  ],
  "resolution_order": "Priority",
  "cache_ttl_seconds": 60,
  "publish_to": "nuget-hosted-2"
}
JSON
)
    api_post "/api/repository/${virtual_id}/virtual/members" -H "Content-Type: application/json" -d "$update_payload" > /dev/null

    echo "$virtual_id"
}

print_test "Ensure nuget repositories exist"
REPO_LIST=$(api_get "/api/repository/list" || echo "[]")
record_output "$REPO_LIST"

HOSTED_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="nuget-hosted") | .id')
PROXY_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="nuget-proxy") | .id')
HOSTED2_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="nuget-hosted-2") | .id')
VIRTUAL_ID=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="nuget-virtual") | .id')
STORAGE_NAME=$(echo "$REPO_LIST" | jq -r '.[] | select(.name=="nuget-hosted") | .storage_name')

if [ -z "$HOSTED_ID" ] || [ "$HOSTED_ID" = "null" ] || [ -z "$PROXY_ID" ] || [ "$PROXY_ID" = "null" ]; then
    fail "Seeded nuget-hosted or nuget-proxy repository missing"
    cleanup_workspace "$WORKSPACE"
    exit 1
fi

HOSTED2_ID=$(ensure_nuget_hosted_repo "nuget-hosted-2" "$HOSTED2_ID" "$STORAGE_NAME")
VIRTUAL_ID=$(ensure_nuget_virtual_repo "$VIRTUAL_ID" "$STORAGE_NAME")

VIRTUAL_CFG=$(api_get "/api/repository/${VIRTUAL_ID}/virtual/members" || echo "{}")
record_output "$VIRTUAL_CFG"
MEMBER_COUNT=$(echo "$VIRTUAL_CFG" | jq '.members | length')
if [ "$MEMBER_COUNT" -ge 2 ]; then
    clear_last_log
    pass
else
    fail "NuGet virtual members not configured"
fi

PACKAGE_DIR_1="$WORKSPACE/pkg1"
create_package "$PACKAGE_DIR_1" "$PACKAGE_ID" "$PACKAGE_VERSION_1"

print_test "Pack NuGet package ${PACKAGE_ID} ${PACKAGE_VERSION_1}"
if run_cmd dotnet pack "${PACKAGE_DIR_1}/${PACKAGE_ID}.csproj" -c Release; then
    pass
else
    fail "dotnet pack failed for first package"
fi

NUPKG_1=$(find "${PACKAGE_DIR_1}/out" -name "*.nupkg" ! -name "*.symbols.nupkg" | head -n 1)

print_test "Push package to nuget-hosted"
if run_cmd dotnet nuget push "$NUPKG_1" --source "$HOSTED_INDEX_URL" --api-key "$TEST_TOKEN" --skip-duplicate; then
    pass
else
    fail "dotnet nuget push failed for nuget-hosted"
fi

print_test "Hosted: service index is available"
HOSTED_INDEX=$(curl -sf "$HOSTED_INDEX_URL" || echo "")
record_output "$HOSTED_INDEX"
if jq -e '.resources[] | select(.["@type"] | tostring | contains("PackageBaseAddress"))' <<<"$HOSTED_INDEX" > /dev/null 2>&1; then
    clear_last_log
    pass
else
    fail "NuGet hosted service index missing PackageBaseAddress"
fi

print_test "Hosted: restore package from nuget-hosted"
HOSTED_CONSUMER="$WORKSPACE/consumer-hosted"
create_consumer "$HOSTED_CONSUMER" "$HOSTED_INDEX_URL" "$PACKAGE_ID" "$PACKAGE_VERSION_1"
if run_cmd dotnet restore "${HOSTED_CONSUMER}/consumer.csproj"; then
    pass
else
    fail "dotnet restore failed against nuget-hosted"
fi

print_test "Proxy: restore package through nuget-proxy"
PROXY_CONSUMER="$WORKSPACE/consumer-proxy"
create_consumer "$PROXY_CONSUMER" "$PROXY_INDEX_URL" "$PACKAGE_ID" "$PACKAGE_VERSION_1"
clear_nuget_global_packages
if run_cmd dotnet restore "${PROXY_CONSUMER}/consumer.csproj" --force --no-cache; then
    pass
else
    fail "dotnet restore failed against nuget-proxy"
fi

print_test "Proxy: restore package from cached proxy"
PROXY_CONSUMER_2="$WORKSPACE/consumer-proxy-cached"
create_consumer "$PROXY_CONSUMER_2" "$PROXY_INDEX_URL" "$PACKAGE_ID" "$PACKAGE_VERSION_1"
clear_nuget_global_packages
if run_cmd dotnet restore "${PROXY_CONSUMER_2}/consumer.csproj" --force --no-cache; then
    pass
else
    fail "dotnet restore failed against cached nuget-proxy"
fi

print_test "Proxy: packages API lists cached NuGet package"
PROXY_PACKAGES_JSON=$(api_get "/api/repository/${PROXY_ID}/packages?page=1&per_page=50" || echo "{}")
record_output "$PROXY_PACKAGES_JSON"
if jq -e --arg package "$PACKAGE_ID" --arg version "$PACKAGE_VERSION_1" '
    .items[]
    | select(.package == $package)
    | select(.cache_path | ascii_downcase | contains("/" + ($version | ascii_downcase) + "/"))
' <<<"$PROXY_PACKAGES_JSON" > /dev/null 2>&1; then
    clear_last_log
    pass
else
    fail "NuGet proxy packages API did not surface cached package ${PACKAGE_ID} ${PACKAGE_VERSION_1}"
fi

PACKAGE_DIR_2="$WORKSPACE/pkg2"
create_package "$PACKAGE_DIR_2" "$PACKAGE_ID_2" "$PACKAGE_VERSION_3"

print_test "Pack second NuGet package ${PACKAGE_ID_2} ${PACKAGE_VERSION_3}"
if run_cmd dotnet pack "${PACKAGE_DIR_2}/${PACKAGE_ID_2}.csproj" -c Release; then
    pass
else
    fail "dotnet pack failed for second package"
fi

NUPKG_2=$(find "${PACKAGE_DIR_2}/out" -name "*.nupkg" ! -name "*.symbols.nupkg" | head -n 1)

print_test "Push package to nuget-hosted-2"
if run_cmd dotnet nuget push "$NUPKG_2" --source "$HOSTED_2_INDEX_URL" --api-key "$TEST_TOKEN" --skip-duplicate; then
    pass
else
    fail "dotnet nuget push failed for nuget-hosted-2"
fi

print_test "Virtual: restore package from proxy-backed member"
VIRTUAL_CONSUMER_PROXY="$WORKSPACE/consumer-virtual-proxy"
create_consumer "$VIRTUAL_CONSUMER_PROXY" "$VIRTUAL_INDEX_URL" "$PACKAGE_ID" "$PACKAGE_VERSION_1"
if run_cmd dotnet restore "${VIRTUAL_CONSUMER_PROXY}/consumer.csproj"; then
    pass
else
    fail "dotnet restore failed for proxy-backed package via virtual repo"
fi

print_test "Virtual: restore package from hosted member"
VIRTUAL_CONSUMER_HOSTED="$WORKSPACE/consumer-virtual-hosted"
create_consumer "$VIRTUAL_CONSUMER_HOSTED" "$VIRTUAL_INDEX_URL" "$PACKAGE_ID_2" "$PACKAGE_VERSION_3"
if run_cmd dotnet restore "${VIRTUAL_CONSUMER_HOSTED}/consumer.csproj"; then
    pass
else
    fail "dotnet restore failed for hosted package via virtual repo"
fi

PACKAGE_DIR_3="$WORKSPACE/pkg3"
create_package "$PACKAGE_DIR_3" "$PACKAGE_ID" "$PACKAGE_VERSION_2"

print_test "Pack virtual publish package ${PACKAGE_ID} ${PACKAGE_VERSION_2}"
if run_cmd dotnet pack "${PACKAGE_DIR_3}/${PACKAGE_ID}.csproj" -c Release; then
    pass
else
    fail "dotnet pack failed for virtual publish package"
fi

NUPKG_3=$(find "${PACKAGE_DIR_3}/out" -name "*.nupkg" ! -name "*.symbols.nupkg" | head -n 1)

print_test "Virtual: push package through nuget-virtual"
if run_cmd dotnet nuget push "$NUPKG_3" --source "$VIRTUAL_INDEX_URL" --api-key "$TEST_TOKEN" --skip-duplicate; then
    pass
else
    fail "dotnet nuget push failed for nuget-virtual"
fi

print_test "Virtual: published package resolves from configured publish target"
VIRTUAL_CONSUMER_PUBLISH="$WORKSPACE/consumer-virtual-publish"
create_consumer "$VIRTUAL_CONSUMER_PUBLISH" "$HOSTED_2_INDEX_URL" "$PACKAGE_ID" "$PACKAGE_VERSION_2"
if run_cmd dotnet restore "${VIRTUAL_CONSUMER_PUBLISH}/consumer.csproj"; then
    pass
else
    fail "Package published through virtual repo did not reach hosted target"
fi

cleanup_workspace "$WORKSPACE"
print_summary
