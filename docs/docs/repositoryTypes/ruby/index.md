# RubyGems Repository

Pkgly can host a RubyGems-compatible repository for private gems or act as a proxy cache in
front of an upstream (for example `https://rubygems.org`).

Ruby support targets the **RubyGems Compact Index** endpoints and the legacy RubyGems marshal index
endpoints commonly used by Bundler for dependency resolution against custom sources.

## Hosted Mode

- `gem push` / `gem yank` are supported via the RubyGems API endpoints under the repository base
  path.
- Uploaded `.gem` files are stored under `gems/` and indexed into the Pkgly package catalog
  (`projects` + `project_versions`) for search and listings.

## Proxy Mode

- Configure an `upstream_url` (for example `https://rubygems.org`).
- GET/HEAD requests for supported index files (Compact Index + legacy RubyGems index) and `.gem`
  downloads are served from cache if present; otherwise Pkgly fetches from upstream and caches the
  response.
- Proxy repositories are read-only (publish/yank are rejected).

## Client Setup (Bundler)

Add the repository as a source in your `Gemfile`:

```ruby
source "https://<host>/repositories/<storage>/<repo>/"
```

If the repository is private, configure credentials via Bundler (recommended over embedding tokens
in the `Gemfile`):

```bash
bundle config set --global https://<host>/repositories/<storage>/<repo>/ token:<pkgly_token>
```

Pkgly accepts HTTP Basic auth for repository routes; the password may be a Pkgly auth token.
