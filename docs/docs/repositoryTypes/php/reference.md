# PHP Composer Repository Quick Reference (V2)

## Configuration

### Repository config (hosted)
```json
{ "type": "Hosted" }
```

### Repository config (proxy)
```json
{
  "type": "Proxy",
  "config": {
    "routes": [
      { "url": "https://repo.packagist.org", "name": "Packagist" }
    ]
  }
}
```
Notes:
- Routes may be edited in the Admin UI; the first route is tried first. Empty routes fall back to the Packagist default.
- Proxy repos are read-only. Dist files and p2 metadata are cached on first request and served from Pkgly with rewritten `dist.url` values.

### Composer client configuration
`composer.json` (project):
```json
{
  "repositories": [
    { "type": "composer", "url": "https://your-host/repositories/<storage>/<php-repo>" }
  ],
  "require": { "mycompany/mypackage": "^1.0" }
}
```

`auth.json`:
```json
{
  "http-basic": {
    "your-host": { "username": "user", "password": "pass" }
  }
}
```

## Upload (hosted)
- Zip must contain `composer.json` with valid `name` (`vendor/package`) and `version`.
- Accepted dist path shapes (both work):
  - `dist/<vendor>/<package>/<version>.zip` (recommended; simplest)
  - `dist/<vendor>/<package>/<version>/<filename>.zip`
- Upload example (recommended shape):
```bash
curl -u user:pass -X PUT \
  -T dist/mycompany-mypackage-1.0.0.zip \
  https://your-host/repositories/<storage>/<php-repo>/dist/mycompany/mypackage/1.0.0.zip
```
Pkgly extracts `composer.json`, validates `name` + `version` against the path, rewrites `dist.url`
back to Pkgly, and writes p2 metadata to `/p2/mycompany/mypackage.json` (plus `~dev` when relevant).

## Proxy behavior (Packagist caching)
- Point Composer at the proxy repo URL (same URL shape as hosted). No client-side changes are required beyond the repository entry.
- First metadata request (`p2/<vendor>/<package>.json`) is fetched from upstream, rewritten so `dist.url` points at Pkgly, stored under `p2/...`, and indexed in the catalog.
- First dist download streams from upstream while being cached under `dist/<vendor>/<package>/<version>/<filename>`.
- Subsequent installs are served from Pkgly’s cache; upstream is not hit again unless the cache entry is removed.
- Admin ➜ Repository ➜ Packages tab shows cached dist files/versions; deleting a cache path evicts both storage and catalog entries.

## Client fetch paths (Composer V2)
- Root: `/repositories/<storage>/<php-repo>/packages.json` (contains only `metadata-url`)
- Metadata: `/repositories/<storage>/<php-repo>/p2/<vendor>/<package>.json`
- Dist: `/repositories/<storage>/<php-repo>/dist/<vendor>/<package>/<version>.zip`
  (or `/dist/<vendor>/<package>/<version>/<filename>.zip` when the upstream filename must be preserved)

## Useful Composer commands
```bash
composer install                # install all deps
composer require mycompany/mypackage:^1.0
composer update mycompany/mypackage
composer config --global http-basic.your-host user pass
```

## Package layout reminder
```
my-package/
├── composer.json
├── src/
└── tests/
```

Minimal `composer.json`:
```json
{
  "name": "mycompany/mypackage",
  "version": "1.0.0",
  "type": "library",
  "require": { "php": ">=7.4" },
  "autoload": { "psr-4": { "MyCompany\\MyPackage\\": "src/" } }
}
```
