# NPM Repository

Pkgly can run as an NPM-compatible registry for private packages or act as a lightweight cache
in front of public registries.

## Hosted Mode

- Requires authenticated users with `Write` access to publish via the standard NPM publish flow.
- Upload tarballs by pointing your `.npmrc` to
  `/repositories/<storage>/<repository>/` (Pkgly expects the trailing slash).
- Metadata is stored in the project database for later browsing.

## Proxy Mode

- Configure one or more upstream URLs (for example `https://registry.npmjs.org`) in the repository
  settings.
- GET/HEAD requests are served from Pkgly if the asset is cached; otherwise the request is
  proxied upstream and cached locally for subsequent access.
- Pkgly enforces its own permission checks before reaching the upstream registry.

## Downloading Packages

Any user with read access can install packages using the standard NPM tooling by pointing to the
repository endpoint. Cached assets are returned immediately; proxy mode automatically refreshes
missing packages from the configured upstreams.

### Yarn Classic

Yarn 1.x is supported for proxy repositories, including scoped packages. Configure the same
registry URL with a trailing slash:

```bash
yarn config set registry https://your-pkgly.example.com/repositories/storage/npm-proxy/
```

Yarn Classic requests scoped metadata using an encoded slash (for example,
`@babel%2Fcode-frame`). Pkgly rewrites the returned tarball URLs to the proxy, so both metadata
and archives are cached locally.
