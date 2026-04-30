# Pkgly CLI

`pkglyctl` is the terminal client for operating a Pkgly instance. It talks to the existing Pkgly HTTP APIs and repository routes; it does not start the server or replace native package manager workflows.

## Build

From the repository root:

```bash
cargo build -p pkgly-cli
```

The binary is named `pkglyctl`:

```bash
target/debug/pkglyctl --help
```

## Configuration

Configuration is resolved in this order:

1. CLI flags
2. Environment variables
3. Config file profile values

Supported global flags:

```bash
pkglyctl --profile local --base-url http://localhost:8888 --token "$PKGLY_TOKEN" repo list
```

Supported environment variables:

| Variable | Purpose |
| --- | --- |
| `PKGLY_URL` | Base URL for the Pkgly server |
| `PKGLY_TOKEN` | API token used as a bearer token |
| `PKGLY_PROFILE` | Profile name to load from the config file |
| `PKGLYCTL_CONFIG` | Explicit config file path |

Default config path:

```text
$XDG_CONFIG_HOME/pkgly/config.toml
```

Example config:

```toml
active_profile = "local"

[profiles.local]
base_url = "http://localhost:8888"
token = "pkgly-token"
default_storage = "test-storage"
```

On Unix platforms, `pkglyctl` writes the config file with owner-only permissions.

## Authentication

Set an existing token:

```bash
pkglyctl auth set-token "$PKGLY_TOKEN"
```

Create and store a token through the server login flow:

```bash
pkglyctl --base-url http://localhost:8888 auth login --username admin
```

`auth login` prompts for the password without echoing it, then uses the password only to create a short-lived session and request an API token. The password is not stored. For scripts, pass `--password "$PKGLY_PASSWORD"`.

Check the current identity:

```bash
pkglyctl auth whoami
```

Remove the stored token from the active profile:

```bash
pkglyctl auth logout
```

## Profiles

```bash
pkglyctl profile list
pkglyctl profile show local
pkglyctl profile use local
pkglyctl profile remove local
```

Use profiles when you operate more than one Pkgly instance, or when local and production environments require different default storages.

## Repositories

Repository references can be either a UUID or `storage/repository`.

```bash
pkglyctl repo list
pkglyctl repo get test-storage/maven-releases
pkglyctl repo id test-storage/maven-releases
pkglyctl repo url test-storage/maven-releases com/acme/app/1.0.0/app-1.0.0.jar
```

Create a repository:

```bash
pkglyctl repo create maven maven-releases --storage test-storage
```

Read and update repository config:

```bash
pkglyctl repo config-list test-storage/maven-releases
pkglyctl repo config-get test-storage/maven-releases maven
pkglyctl repo config-set test-storage/maven-releases maven '{"type":"Hosted"}'
```

Delete requires explicit confirmation:

```bash
pkglyctl repo delete test-storage/maven-releases --yes
```

## Storages

```bash
pkglyctl storage list
pkglyctl storage get 00000000-0000-0000-0000-000000000001
pkglyctl storage create --type local test-storage /var/lib/pkgly/storage
```

Only `--type local` is currently supported for storage creation.

## Packages

List package names and versions, search package catalog entries, and describe a package:

```bash
pkglyctl package list test-storage/maven-releases
pkglyctl package list --no-header test-storage/maven-releases
pkglyctl package search "app"
pkglyctl package describe test-storage/maven-releases app 1.0.0
```

Download an artifact through the repository route:

```bash
pkglyctl package download test-storage/maven-releases com/acme/app/1.0.0/app-1.0.0.jar --output-file app.jar
```

Delete cached package paths:

```bash
pkglyctl package delete test-storage/maven-releases com/acme/app/1.0.0/app-1.0.0.jar --yes
```

## Uploads

Upload commands use the same hosted repository routes that package managers use.

Maven raw artifact upload:

```bash
pkglyctl package upload maven test-storage/maven-releases com/acme/app/1.0.0/app-1.0.0.jar ./app-1.0.0.jar
```

Python multipart upload:

```bash
pkglyctl package upload python test-storage/python-internal my-package 1.0.0 ./dist/my_package-1.0.0-py3-none-any.whl
```

Go Athens-style upload:

```bash
pkglyctl package upload go test-storage/go-internal example.com/acme/app v1.0.0 ./module.zip ./module.info ./go.mod
```

PHP, Debian, Helm, RubyGems, and NuGet uploads are also available:

```bash
pkglyctl package upload php test-storage/php-internal dist/acme/app/1.0.0.zip ./app.zip
pkglyctl package upload deb test-storage/deb-internal pool/main/a/app/app_1.0.0_amd64.deb ./app_1.0.0_amd64.deb
pkglyctl package upload helm test-storage/helm-internal charts/app/app-1.0.0.tgz ./app-1.0.0.tgz
pkglyctl package upload ruby test-storage/ruby-internal ./pkg/app-1.0.0.gem
pkglyctl package upload nuget test-storage/nuget-internal ./App.1.0.0.nupkg
```

For npm, Cargo, and Docker, use native tooling. `pkglyctl native` prints the exact command shape:

```bash
pkglyctl native npm test-storage/npm-internal
pkglyctl native cargo test-storage/cargo-internal
pkglyctl native docker test-storage/docker-internal app:1.0.0
```

## Output

Use table output for humans and JSON output for scripts:

```bash
pkglyctl --output table repo list
pkglyctl --output json repo list
```
