# Debian Repositories

Pkgly supports Debian package repositories that expose the standard `dists/` and `pool/` layout used by `apt`.

Debian repositories can run in two modes:
- **Hosted**: you upload `.deb` packages to Pkgly; Pkgly generates `dists/` metadata.
- **Proxy/Mirror**: Pkgly serves and caches upstream APT paths (including `by-hash`) and can download all referenced `.deb` files for offline mirroring.

For hosted repositories, Pkgly generates `Packages`, `Packages.gz`, and `Packages.xz` indexes for every component/architecture pair and emits a `Release` file so that `apt update` behaves as expected.

## Uploading packages

Uploads require write access to the repository and must use `multipart/form-data`. The simplest workflow is to select a distribution and component and post the `.deb` file in the `package` field:

```bash
curl -u username:password \
  -F distribution=stable \
  -F component=main \
  -F package=@pkgly_1.0.0_amd64.deb \
  https://pkgly.example.com/repositories/<storage>/<repository>
```

If `distribution` or `component` are omitted the first value from the repository configuration will be used. Architectures are detected from the package metadata and must match one of the allowed entries (include `all` if you need architecture-independent packages).

Every upload is stored under `pool/<component>/<first-letter>/<package>/` and automatically indexed inside the appropriate `dists/<suite>/<component>/binary-<arch>/Packages*` files.

## Using the repository with apt

Add the repository to `/etc/apt/sources.list.d/pkgly.list` (replace placeholders with your storage and repository names):

```text
# /etc/apt/sources.list.d/pkgly.list
deb [trusted=yes] https://pkgly.example.com/repositories/<storage>/<repository> stable main
```

Release signatures are not generated yet, so include the `trusted=yes` option or configure your host to allow unsigned repositories. Once added, run `sudo apt update` followed by the usual `apt install` commands. Pkgly serves the `Packages`, `Packages.gz`, and `Packages.xz` files that `apt` expects.

## Proxy/mirror repositories

Debian proxy repositories are designed to behave like an upstream mirror:
- `GET`/`HEAD` requests are served from Pkgly storage when cached; on a cache miss Pkgly fetches `upstream_url + request.path`, stores it, then serves it.
- Upstream metadata bytes (`InRelease` / `Release` / `Release.gpg`) are served unchanged so signature verification can work.
- An offline mirror refresh can be triggered via the API: `POST /api/repository/<id>/deb/refresh` (requires repository edit permission). The refresh downloads `Release`/`InRelease`/`Release.gpg`, the `Packages` index for configured dists/components/architectures, creates a SHA256 `by-hash` alias for `Packages`, and downloads every referenced `.deb` file.

## Current limitations

- Hosted repositories: Release files are unsigned. Configure clients with `trusted=yes` until signing is available.
- Hosted repositories: uploads must be `.deb` artifacts; source packages and `apt` by-hash lookups are not implemented yet.
- Proxy repositories: upstream authentication is not supported yet; scheduling is not implemented yet (manual refresh only).
