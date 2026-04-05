# PHP Composer Repository HTTP Routes (V2)

All paths are relative to `https://<host>/repositories/<storage>/<repository>`.

## Discovery
- `GET /packages.json`  
  Returns `{ "metadata-url": "/repositories/<storage>/<repository>/p2/%package%.json", "packages": {} }`.
  Used by Composer to locate per-package metadata.

## Package metadata (p2)
- `GET /p2/<vendor>/<package>.json`  
  Returns Composer V2 metadata with dist URLs rewritten to Pkgly.
- `GET /p2/<vendor>/<package>~dev.json`  
  Dev metadata when dev versions exist.
- `HEAD` supported for cache validation.
- `404` when package is unknown.

## Dist artifacts
- `GET /dist/<vendor>/<package>/<version>.zip`  
  Streams hosted artifact (or cached proxy when proxy is added).
- `HEAD /dist/<vendor>/<package>/<version>.zip` for size/etag checks.

## Upload (hosted)
- `PUT /dist/<vendor>/<package>/<version>.zip`  
  Body: ZIP containing `composer.json`. Auth: `Write` permission required.  
  Pkgly validates name/version against path, computes checksums, updates `/p2/...` metadata, and
  indexes the version.
- `POST` allowed as alias for `PUT`.

## Auth
- Basic or bearer token; enforced on writes. Reads follow repository visibility/auth settings.

## Status codes
- `201 Created` on successful upload.
- `400 Bad Request` on invalid path or composer.json mismatch.
- `401 Unauthorized` when auth required.
- `404 Not Found` for missing metadata or dist.

## Notes
- Composer V2 format only; no legacy providers or monolithic packages.json.
- Dist URLs in metadata always point back to Pkgly, enabling offline/cache control.
