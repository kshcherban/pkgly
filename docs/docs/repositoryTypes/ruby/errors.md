# RubyGems Repository Errors

## Missing trailing slash

Many Ruby tools build endpoint URLs by joining paths onto the configured source URL. If the source
URL does not end with a `/`, the client may drop the final path segment and request the wrong URL.

Use:

```
https://<host>/repositories/<storage>/<repo>/
```

Not:

```
https://<host>/repositories/<storage>/<repo>
```

## Legacy RubyGems endpoints

Pkgly supports Compact Index and the common legacy RubyGems marshal index endpoints used by Bundler
(`specs.4.8.gz`, `latest_specs.4.8.gz`, and `quick/Marshal.4.8/*.gemspec.rz`). If your client
requests other RubyGems “full index” endpoints that Pkgly doesn’t implement, you’ll see `404 Not Found`.

## Proxy is read-only

Ruby proxy repositories do not support publishing. Use a hosted repository for `gem push` and
`gem yank`.
