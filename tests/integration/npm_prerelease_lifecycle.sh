#!/bin/bash
# Sourced after the hosted and virtual fixtures exist.
# shellcheck source=tests/integration/common.sh

lifecycle_install() {
    local name="$1" spec="$2" expected="$3" phase="$4" registry="$5" cache="$6"
    local directory="$WORKSPACE/lifecycle-install-${name##*/}-${phase}"
    mkdir -p "$directory"
    printf '//%s/repositories/%s/:_authToken=%s\nregistry=%s/repositories/%s/\n' \
        "$PKGLY_REGISTRY_HOST" "$registry" "$TEST_TOKEN" "$PKGLY_URL" "$registry" > "$directory/.npmrc"
    (
        cd "$directory" || exit 1
        run_cmd npm install "${name}@${spec}" --cache "$cache" --prefer-online \
            --package-lock=false --ignore-scripts --no-audit --no-fund &&
            [ "$(jq -r .version "node_modules/${name}/package.json")" = "$expected" ]
    )
}

for LIFE_NAME in pkgly-beta-lifecycle @pkgly-test/beta-lifecycle; do
    LIFE_DIRECTORY="$WORKSPACE/lifecycle-${LIFE_NAME##*/}"
    mkdir -p "$LIFE_DIRECTORY"
    cp "$FIXTURE_DIR/"* "$LIFE_DIRECTORY/"
    cp "$WORKSPACE/hello-pkg/.npmrc" "$LIFE_DIRECTORY/.npmrc"
    cd "$LIFE_DIRECTORY" || return 1
    LIFE_PATH="/repositories/${NPM_HOSTED_REPO}/${LIFE_NAME}"
    LIFE_TAGS="/repositories/${NPM_HOSTED_REPO}/-/package/${LIFE_NAME}/dist-tags"
    print_test "Seed stable latest and an unrelated tag for ${LIFE_NAME}"
    jq --arg name "$LIFE_NAME" '.name=$name | .version="1.0.0"' package.json > package.json.tmp
    mv package.json.tmp package.json
    if run_cmd npm publish --tag latest && run_cmd npm dist-tag add "${LIFE_NAME}@1.0.0" canary; then pass; else fail "Lifecycle seed failed"; fi

    for LIFE_SEQUENCE in 1 2; do
        LIFE_VERSION="2.0.0-beta.${LIFE_SEQUENCE}"
        print_test "Publishing ${LIFE_NAME}@${LIFE_VERSION} advances only beta"
        jq --arg version "$LIFE_VERSION" '.version=$version' package.json > package.json.tmp
        mv package.json.tmp package.json
        if run_cmd npm publish --tag beta &&
           api_get "$LIFE_TAGS" | jq -e --arg version "$LIFE_VERSION" '. == {"latest":"1.0.0","canary":"1.0.0","beta":$version}' > /dev/null; then
            pass
        else fail "Beta publication changed unrelated tags"; fi
        for LIFE_REGISTRY in "$NPM_HOSTED_REPO" "$NPM_VIRTUAL_REPO"; do
            for LIFE_CACHE in warm fresh; do
                LIFE_CACHE_PATH="$WORKSPACE/lifecycle-cache-${LIFE_NAME##*/}-${LIFE_REGISTRY##*/}-${LIFE_CACHE}"
                if [ "$LIFE_CACHE" = fresh ]; then LIFE_CACHE_PATH="${LIFE_CACHE_PATH}-${LIFE_SEQUENCE}"; fi
                print_test "Install ${LIFE_NAME}@beta.${LIFE_SEQUENCE} via ${LIFE_REGISTRY} with ${LIFE_CACHE} cache"
                if lifecycle_install "$LIFE_NAME" beta "$LIFE_VERSION" "${LIFE_SEQUENCE}-${LIFE_REGISTRY##*/}-${LIFE_CACHE}" "$LIFE_REGISTRY" "$LIFE_CACHE_PATH"; then pass; else fail "Beta resolved to the wrong version"; fi
            done
        done
    done
    for LIFE_VERSION in 2.0.0-beta.1 2.0.0-beta.2; do
        print_test "Exact prerelease ${LIFE_NAME}@${LIFE_VERSION} remains installable"
        if lifecycle_install "$LIFE_NAME" "$LIFE_VERSION" "$LIFE_VERSION" "exact-${LIFE_VERSION}" "$NPM_HOSTED_REPO" "$WORKSPACE/lifecycle-exact-cache"; then pass; else fail "Exact prerelease was lost"; fi
    done
    cd "$LIFE_DIRECTORY" || return 1
    print_test "Duplicate publication of ${LIFE_NAME} preserves the original tarball and tags"
    LIFE_BEFORE=$(api_get "$LIFE_PATH")
    LIFE_TARBALL=$(jq -r '.versions["2.0.0-beta.2"].dist.tarball' <<<"$LIFE_BEFORE")
    LIFE_HASH_BEFORE=$(curl -fsS -H "$(get_auth_header)" "$LIFE_TARBALL" | sha256sum)
    printf '\n// Duplicate publication must not replace these bytes.\n' >> index.js
    if run_cmd npm publish --tag latest; then
        fail "Duplicate publication succeeded"
    else
        LIFE_AFTER=$(api_get "$LIFE_PATH")
        LIFE_HASH_AFTER=$(curl -fsS -H "$(get_auth_header)" "$LIFE_TARBALL" | sha256sum)
        if [ "$LIFE_HASH_BEFORE" = "$LIFE_HASH_AFTER" ] && jq -e --argjson before "$LIFE_BEFORE" '. == $before' <<<"$LIFE_AFTER" > /dev/null; then pass; else fail "Rejected duplicate changed package state"; fi
    fi
    print_test "Remove latest and all remaining tags for ${LIFE_NAME}"
    if run_cmd npm dist-tag rm "$LIFE_NAME" latest &&
       api_get "$LIFE_PATH" | jq -e '.["dist-tags"] | has("latest") | not' > /dev/null &&
       run_cmd npm dist-tag rm "$LIFE_NAME" beta && run_cmd npm dist-tag rm "$LIFE_NAME" canary &&
       api_get "$LIFE_TAGS" | jq -e '. == {}' > /dev/null &&
       api_get "$LIFE_PATH" | jq -e '.["dist-tags"] == {}' > /dev/null; then pass; else fail "Deleted tags were restored"; fi
    print_test "Beta publication after tag deletion does not restore latest for ${LIFE_NAME}"
    jq '.version="2.0.0-beta.3"' package.json > package.json.tmp
    mv package.json.tmp package.json
    if run_cmd npm publish --tag beta &&
       api_get "$LIFE_TAGS" | jq -e '. == {"beta":"2.0.0-beta.3"}' > /dev/null; then pass; else fail "Publication restored latest"; fi

done
cd "$WORKSPACE/hello-pkg" || return 1
