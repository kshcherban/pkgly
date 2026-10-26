# CI build caching

PR integration checks build Pkgly and its package-tool runner before running the
repository suites. Both builds use caches without granting PRs registry write
access.

## Pkgly image

The PR build imports its own GitHub Actions cache (`scope=pkgly`) and the public
GHCR cache at `ghcr.io/kshcherban/pkgly:buildcache-amd64`. The publishing workflow
updates the GHCR cache on pushes to `main`. PR builds use `CARGO_INCREMENTAL=0`,
matching the publishing build, and only export to their PR-scoped Actions cache.

The cargo-chef dependency layer is exported with `mode=max`. BuildKit's mutable
cache mounts, including the final Rust compilation's target directories, are not
persisted by this exporter. A cache hit on the dependency layer avoids rebuilding
unchanged dependencies; it does not preserve all incremental compiler state.

## Test-runner image

The runner uses `debian:trixie-slim`, matching the Pkgly runtime. Its initial
certificate and OpenSSL installation matches the runtime layer so BuildKit can
reuse that layer when both images are built on the same builder and platform.
The remaining package-tool layers have their own cache.

The `Test runner cache` workflow saves `scope=test-runner` on `main`. It runs when
the runner Dockerfile, smoke checks, Python smoke fixture, or cache workflow
changes on `main`. A Monday schedule rebuilds without cached installation steps
to refresh upstream tools. Maintainers can also run the workflow manually on
`main` to seed or repair the cache. The first PR after a runner change may still
need a cold build until that change lands on `main`.

GitHub scopes PR-written caches to `refs/pull/<number>/merge`; other PRs cannot
restore those entries. Keeping a producer on `main` makes the baseline runner
cache available across PRs. The `pkgly` and `test-runner` scopes remain separate
so the images do not overwrite each other's cache index.

The runner installs Java 21 and uses Debian's Docker and .NET package feeds.
Python packaging commands run through `/opt/python`, a virtual environment that
can access Debian's setuptools and wheel packages without modifying the managed
system Python environment.

The repository tests use a surviving Maven artifact for the unauthenticated
upload check, credentials when reading metadata from the authenticated NPM
repository, and explicit control-directory permissions when building Debian
fixtures. These checks retain their protocol assertions across tool versions.

## Validation

Both the PR and cache-warming workflows execute `tests/docker/test-runner.sh` in
the built image to check the package tools and build and install a Python wheel.
PR checks then run the full Docker-backed repository integration suite.

The shared Rust test logger writes through the test harness. Passing tests keep
expected failure-path diagnostics captured; failed tests display their logs.
Use `cargo test -q -- --nocapture` to display logs from passing tests as well.

Actual speedups must be measured in GitHub Actions after the shared cache is
populated. Expect cold builds when dependency recipes or upstream base images
change.
