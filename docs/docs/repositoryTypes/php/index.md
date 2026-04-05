# Composer Repository

Pkgly hosts Composer packages using the Composer V2 metadata format (p2 with `metadata-url`).
Uploads are validated against `composer.json`, rewritten to point back to Pkgly, and indexed for
search.

## Uploading Packages (Hosted)

- Authenticate with `Write` permission.
- Upload a ZIP archive containing `composer.json` to  
  `/repositories/<storage>/<repository>/dist/<vendor>/<package>/<version>.zip` via `PUT` or `POST`.
- Pkgly streams the upload, extracts `composer.json`, validates `name` + `version`, and writes
  p2 metadata to `/p2/<vendor>/<package>.json` (and `~dev` variant when needed).
- Dist URLs in metadata are rewritten to Pkgly download endpoints.

## Downloading Packages

Composer clients consume:
- Root: `/repositories/<storage>/<repository>/packages.json` (contains only `metadata-url`)
- Package metadata: `/repositories/<storage>/<repository>/p2/<vendor>/<package>.json`
- Dist: `/repositories/<storage>/<repository>/dist/<vendor>/<package>/<version>.zip`

## Metadata & Indexing

- Version records are stored in `project_versions` with `PhpPackageMetadata` in `VersionData.extra`.
- Search and admin package lists pull from the database; no filesystem scans are required.
