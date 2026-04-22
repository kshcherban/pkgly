# Artifactory To Pkgly Migration

[`artifactory_to_pkgly.py`](/Users/konstantinshcherban/repos/pkgly/scripts/artifactory_to_pkgly.py) migrates supported Artifactory `local` repositories into Pkgly hosted repositories.

The script has two migration modes:

- raw-copy for formats where canonical package files can be moved directly into Pkgly
- protocol-aware republish for formats where Pkgly expects the native publish API instead of raw path uploads

The script prints one JSON object per repository. Each result includes:

- `status`
- `discovered`
- `transferred`
- `skipped_existing`
- `skipped_filtered`
- `skipped_noncanonical`
- `skipped_unsupported_artifacts`
- `dry_run`

## Supported Artifactory package types

Source `packageType` values accepted by the script:

- `maven`
- `gradle`
- `ivy`
- `sbt`
- `pypi`
- `helm`
- `composer`
- `npm`
- `nuget`
- `gems`
- `cargo`
- `go`
- `deb`
- `docker`

Pkgly repository types created by `--create-targets`:

- `maven|gradle|ivy|sbt -> maven`
- `pypi -> python`
- `helm -> helm`
- `composer -> php`
- `npm -> npm`
- `nuget -> nuget`
- `gems -> ruby`
- `cargo -> cargo`
- `go -> go`
- `deb -> deb`
- `docker -> docker`

## How each type is migrated

### Raw-copy types

These use Artifactory file listing plus direct `PUT /repositories/{storage}/{repo}/{path}` into Pkgly:

- `maven`, `gradle`, `ivy`, `sbt`
  Maven-style paths are copied as-is. Checksum sidecars are skipped.
- `pypi`
  Only distributions are copied. `simple/` and generated metadata are skipped. Files are normalized to `<package>/<version>/<filename>`.
- `helm`
  Only `.tgz` and `.tgz.prov` are copied. Target paths are normalized to `charts/<name>/<name>-<version>.*`.
- `composer`
  Only dist ZIP archives are copied. `packages.json`, `p2/*.json`, and checksums are skipped. Target paths are normalized to `dist/<vendor>/<package>/<filename>.zip`.

### Protocol-aware types

These still discover source content in Artifactory, but they publish into Pkgly using the target repository's native API:

- `npm`
  Reads `.tgz` packages, extracts `package.json`, synthesizes an npm publish body, and publishes with `npm-command: publish`.
- `nuget`
  Pushes `.nupkg` files to `/api/v2/package`.
- `gems`
  Posts `.gem` files to `/api/v1/gems`.
- `cargo`
  Reads `.crate` archives, extracts `Cargo.toml`, builds the crates.io publish payload, and publishes to `/api/v1/crates/new`.
- `go`
  Groups canonical `<module>/@v/<version>.zip|mod|info` artifacts and publishes each complete version through `/upload`.
- `deb`
  Uploads `.deb` files with multipart form-data. Pkgly regenerates `dists/` metadata.
- `docker`
  Uses Artifactory's Docker V2 API to enumerate repositories, tags, manifests, and blobs, then pushes blobs and manifests into Pkgly's Docker V2 endpoints.

## What gets skipped

The script skips content on purpose in these cases:

- generated repository metadata that Pkgly rebuilds itself
- checksum sidecars
- non-canonical Go module paths
- unsupported NuGet symbol packages (`.snupkg`, `.symbols.nupkg`)
- Docker schema1 manifests
- any leftover artifacts in a supported repository that do not map safely to a Pkgly publish flow

By default the script skips and reports those artifacts instead of failing the whole repository.

## Prerequisites

- The Artifactory source repository must be `local`.
- The target Pkgly storage named by `--pkgly-storage` must already exist.
- The Pkgly credentials must be allowed to create repositories when `--create-targets` is used.
- The Pkgly credentials must have write access to the target repositories.
- For Docker migrations, the source Artifactory repository must expose the standard Docker API under `/artifactory/api/docker/{repo}/v2/...`.

## Authentication

The script supports bearer tokens or username/password pairs for both systems.

CLI arguments:

- `--artifactory-token`
- `--artifactory-user`
- `--artifactory-password`
- `--pkgly-token`
- `--pkgly-user`
- `--pkgly-password`

Environment variable fallbacks:

- `ARTIFACTORY_TOKEN`
- `ARTIFACTORY_USER`
- `ARTIFACTORY_PASSWORD`
- `PKGLY_TOKEN`
- `PKGLY_USER`
- `PKGLY_PASSWORD`

If a token is provided, it wins over username/password.

## Usage

Show help:

```bash
python3 scripts/artifactory_to_pkgly.py --help
```

Migrate one repository into an existing Pkgly repository:

```bash
python3 scripts/artifactory_to_pkgly.py \
  --artifactory-url "https://artifactory.example.com" \
  --pkgly-url "https://pkgly.example.com" \
  --pkgly-storage "primary" \
  --repo "npm-local"
```

Create missing target repositories automatically:

```bash
python3 scripts/artifactory_to_pkgly.py \
  --artifactory-url "https://artifactory.example.com" \
  --pkgly-url "https://pkgly.example.com" \
  --pkgly-storage "primary" \
  --create-targets \
  --repo "cargo-local"
```

Dry-run every supported repository:

```bash
python3 scripts/artifactory_to_pkgly.py \
  --artifactory-url "https://artifactory.example.com" \
  --pkgly-url "https://pkgly.example.com" \
  --pkgly-storage "primary" \
  --all-repos \
  --dry-run
```

Use explicit Debian defaults when migrating `deb` repositories:

```bash
python3 scripts/artifactory_to_pkgly.py \
  --artifactory-url "https://artifactory.example.com" \
  --pkgly-url "https://pkgly.example.com" \
  --pkgly-storage "primary" \
  --repo "apt-local" \
  --deb-distribution "stable" \
  --deb-component "main" \
  --deb-architectures "amd64,all"
```

## Important flags

- `--repo <name>`
  Select a repository to migrate. Repeat to migrate multiple repositories.

- `--all-repos`
  Migrate every repository discovered in Artifactory. Unsupported package types still report as failed.

- `--path-prefix <prefix>`
  Restrict Artifactory file listing to a subpath. This affects file-list based package types only; Docker discovery uses the Docker API instead.

- `--parallelism <n>`
  Number of repositories migrated concurrently.

- `--timeout <seconds>`
  HTTP timeout used for requests and uploads.

- `--retries <n>`
  Number of retries for retryable failures.

- `--retry-backoff-seconds <seconds>`
  Exponential backoff base delay. Each retry doubles the delay.

- `--create-targets`
  Create missing hosted target repositories in Pkgly.

- `--dry-run`
  Do not push artifacts. The script still validates repository existence and still performs existence checks.

- `--deb-distribution`
  Default Debian suite used for uploads and target repository creation. Default: `stable`.

- `--deb-component`
  Default Debian component used for uploads and target repository creation. Default: `main`.

- `--deb-architectures`
  Comma-separated architectures for Debian target creation. Default: `amd64,all`.

## Retry behavior

Retryable HTTP status codes:

- `408`
- `425`
- `429`
- `500`
- `502`
- `503`
- `504`

Retryable exception classes include:

- `URLError`
- `HTTPException`
- `TimeoutError`
- transient `OSError`

Non-retryable failures, such as `404`, invalid credentials, or malformed package archives, fail immediately.

## Recommended workflow

1. Run a `--dry-run` for one repository.
2. Migrate a small repository of the same package type.
3. Verify the package with the native client against Pkgly.
4. Migrate larger repositories.
5. Review repositories with non-zero `skipped_noncanonical` or `skipped_unsupported_artifacts` before deleting the source.

## Limitations

- Only Artifactory `local` repositories are supported.
- The script still has no checkpoint or resume file.
- Artifact processing is still sequential inside a single repository.
- Re-runs still use `HEAD` checks before upload/publish.
- Docker pagination is not implemented for very large Artifactory catalogs.
- Docker schema1 manifests are skipped.
- Cargo metadata extraction relies on readable `Cargo.toml` content inside the `.crate` archive.
- Debian uploads use the explicit CLI defaults for suite/component instead of inferring them from generated source metadata.

## Verification examples

Maven:

```bash
curl -I \
  "$PKGLY_URL/repositories/primary/maven-local/com/acme/demo/1.0.0/demo-1.0.0.jar"
```

npm:

```bash
curl \
  "$PKGLY_URL/repositories/primary/npm-local/@acme/demo"
```

NuGet:

```bash
curl \
  "$PKGLY_URL/repositories/primary/nuget-local/v3/index.json"
```

Cargo:

```bash
curl \
  "$PKGLY_URL/repositories/primary/cargo-local/index/config.json"
```

Go:

```bash
curl \
  "$PKGLY_URL/repositories/primary/go-local/github.com/acme/demo/@v/list"
```

Docker:

```bash
curl -I \
  -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
  "$PKGLY_URL/v2/primary/docker-local/library/demo/manifests/latest"
```
