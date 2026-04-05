# Go Hosted Repository Workflow

This document explains how Pkgly manages hosted Go module repositories end to end: upload, storage layout, metadata updates, and client download behaviour.

## Repository URL layout

A hosted repository is exposed under two equivalent URL prefixes:

- Canonical: `https://<host>/repositories/<storage-name>/<repository-name>/…`
- Legacy alias (Artipie-style, same repository resolution rules): `https://<host>/api/<repository-name>/…`

All examples below use the canonical form. Substitute `<storage-name>` and `<repository-name>` with your environment values.

## Upload workflow

Hosted repositories accept uploads via a multipart form request that closely mirrors the Athens API (`POST /upload`). Pkgly requires authentication with write permission for the repository.

```bash
curl -X POST \
     -H "Authorization: Bearer <token>" \
     -F "module=@module-v1.2.3.zip" \
     -F "version=v1.2.3" \
     -F "module_name=github.com/acme/widget" \
     -F "info=@module-v1.2.3.info" \        # optional
     -F "gomod=@go.mod" \                  # optional
     https://<host>/repositories/<storage-name>/<repository-name>/upload
```

### Multipart fields

| Field        | Required | Description                                                                 |
|--------------|----------|-----------------------------------------------------------------------------|
| `module`     | Yes      | The Go module archive (`.zip`). Pkgly validates paths and rebuilds the zip. |
| `version`    | Yes      | Module version (e.g. `v1.2.3`).                                             |
| `module_name`| Yes      | Full module path (e.g. `github.com/acme/widget`).                           |
| `info`       | Optional | Contents of `<version>.info`. Pkgly will generate one if omitted.           |
| `gomod`      | Optional | Contents of `go.mod`. Pkgly extracts it from the zip if omitted.            |

### Validation steps

1. **Authentication**: requester must have `RepositoryActions::Write`.
2. **Version consistency**: when an `.info` file is supplied it must contain the same version.
3. **Archive integrity**:
   - No absolute paths or `../` segments.
   - Pkgly rebuilds the archive with the canonical structure `<module>@<version>/…` and inserts the resolved `go.mod`.
4. **Metadata synthesis**: if `info` is absent Pkgly generates:
   ```json
   {
     "Version": "v1.2.3",
     "Time": "<current-timestamp-UTC>"
   }
   ```

### Storage layout

Uploaded files are stored under:

```
<module>/@v/<version>.info
<module>/@v/<version>.mod
<module>/@v/<version>.zip
<module>/@latest          (latest version metadata, refreshed automatically)
<module>/go.mod           (latest go.mod alias)
```

Pkgly also maintains `@v/list`, a newline-delimited list of published versions for the module. This file is regenerated whenever a new version is published.

## Metadata refresh

After a successful upload Pkgly performs the following updates:

1. **Save artefacts**: `.info`, `.mod`, `.zip`.
2. **Update `@v/list`**: append the version (sorted lexicographically; duplicates ignored).
3. **Refresh aliases**:
   - `@latest`: points to the latest `.info` JSON.
   - `go.mod`: latest go.mod.
   These aliases are only updated when both `.info` and `.zip` exist for a version.

## Download workflow

Go toolchain interacts with a hosted repository through the standard proxy endpoints:

| Operation        | HTTP           | Example                                                        |
|------------------|----------------|----------------------------------------------------------------|
| List versions    | `GET /@v/list` | `https://<host>/repositories/<storage>/<repo>/<module>/@v/list` |
| Version info     | `GET /@v/{version}.info` | `…/@v/v1.2.3.info`                                      |
| go.mod           | `GET /@v/{version}.mod` | `…/@v/v1.2.3.mod`                                       |
| Module archive   | `GET /@v/{version}.zip` | `…/@v/v1.2.3.zip`                                       |
| Latest metadata  | `GET /@latest` | `…/@latest`                                                   |
| Latest go.mod    | `GET /go.mod`  | `…/go.mod`                                                    |

A typical client flow (`go get github.com/acme/widget@v1.2.3`) will issue:

1. `GET /@v/list`
2. `GET /@v/v1.2.3.info`
3. `GET /@v/v1.2.3.mod`
4. `GET /@v/v1.2.3.zip`
5. Optional sumdb lookups if `GOSUMDB` is enabled (Pkgly currently returns `501` for hosted sumdb).

Since Pkgly normalizes the uploaded zip, the archive content always matches Go’s expected layout and passes the standard checksum validation.

Example download with go get:
```bash
GOSUMDB=off GOPROXY=https://<host>/repositories/<storage-name>/<repo-name>,https://proxy.golang.org,direct go get -x github.com/example/hello-world
```

## SumDB behaviour

Hosted repositories return:

- `GET /sumdb/sum.golang.org/supported` → `false`
- `GET /sumdb/sum.golang.org/lookup/...` and `tile/...` → `501 Not Implemented`

Clients relying on sumdb should keep the default upstream (`sum.golang.org`) in `GOSUMDB` or `GONOSUMDB` settings for private modules.

## Error handling

| Error                                     | HTTP code | Description                                                 |
|-------------------------------------------|-----------|-------------------------------------------------------------|
| Missing or malformed multipart payload    | 400       | Missing fields, invalid boundary, etc.                      |
| Version mismatch in `.info`               | 400       | Info `Version` differs from the supplied `version` field.   |
| Invalid archive structure                 | 400       | Absolute paths, `../`, or missing go.mod (if extraction fails). |
| Unauthorized                              | 401       | Missing/invalid credentials.                                |
| Forbidden                                 | 403       | Repository disabled or user lacks write permission.         |

## Legacy PUT endpoints

For backwards compatibility the repository still accepts direct `PUT` uploads to each artefact:

- `PUT /<module>/@v/<version>.mod`
- `PUT /<module>/@v/<version>.info`
- `PUT /<module>/@v/<version>.zip`

All validations and metadata refresh logic are shared with the multipart handler, so mixed workflows (e.g. first PUT `.mod`, then multipart for the remaining fields) remain supported.

## Admin tooling

The Admin UI exposes the following controls for hosted repositories:

- **Packages tab**: displays all hosted modules (under `packages/`) and allows deletion of individual artefacts. Removing `.info`, `.mod`, or `.zip` will cause the version to disappear from download responses until republished.
- **Repository configuration**: Hosted mode selection under “Go” config panel.

Use caution when deleting hosted artefacts manually; Pkgly will remove entries and refresh aliases on the next upload.
