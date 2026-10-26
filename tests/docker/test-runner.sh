#!/bin/bash
# ABOUTME: Verifies the integration image runs its package tools on Debian Trixie.
# ABOUTME: Exercises Python packaging and Java discovery before repository E2E suites run.
set -euo pipefail

source /etc/os-release
if [ "$ID" != debian ] || [ "$VERSION_CODENAME" != trixie ]; then
    echo "Expected Debian Trixie, got $PRETTY_NAME" >&2
    exit 1
fi

test -x "${JAVA_HOME}/bin/javac"
test "$(readlink -f "${JAVA_HOME}/bin/java")" = "$(readlink -f "$(command -v java)")"

workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
commands=(
    'mvn --version' 'node --version' 'npm --version' 'yarn --version'
    'docker --version' 'python3 -m pip --version' 'twine --version'
    'ruby --version' 'bundle --version' 'php --version' 'composer --version'
    'go version' 'helm version --short' 'dotnet --version' 'cargo --version'
    'rustfmt --version' 'dpkg-deb --version'
)
for command in "${commands[@]}"; do
    if ! bash -c "$command" >"$workspace/tool.log" 2>&1; then
        cat "$workspace/tool.log" >&2
        echo "Failed: $command" >&2
        exit 1
    fi
done

cp -r /fixtures/python/test-pkg "$workspace/python-package"
cd "$workspace/python-package"
if ! python3 setup.py sdist bdist_wheel >"$workspace/build.log" 2>&1; then
    cat "$workspace/build.log" >&2
    exit 1
fi
python3 -m venv "$workspace/venv"
if ! "$workspace/venv/bin/pip" install --no-index dist/*.whl >"$workspace/install.log" 2>&1; then
    cat "$workspace/install.log" >&2
    exit 1
fi
"$workspace/venv/bin/python" -c 'from pkgly_test_pkg import greet; assert greet("Pkgly") == "Hello, Pkgly!"'
echo "Test-runner image checks passed"
