# RubyGems Standard Notes

Pkgly’s Ruby implementation supports the RubyGems **Compact Index** API and the legacy RubyGems
marshal index endpoints typically used by Bundler/RubyGems when talking to custom sources.

## Supported Features

- Compact Index: `GET|HEAD /names`, `GET|HEAD /versions`, `GET|HEAD /info/<gem>`
- Gem downloads: `GET|HEAD /gems/<gem-file>.gem`
- Legacy RubyGems index (Bundler compatibility):
  - `GET|HEAD /specs.4.8.gz`
  - `GET|HEAD /latest_specs.4.8.gz`
  - `GET|HEAD /prerelease_specs.4.8.gz`
  - `GET|HEAD /quick/Marshal.4.8/*.gemspec.rz`
- Hosted publish/yank:
  - `POST /api/v1/gems`
  - `DELETE /api/v1/gems/yank`

## Unsupported / Out of Scope

- RubyGems “full index” endpoints not listed above (Pkgly does not aim for full RubyGems protocol parity)
- Extra RubyGems.org APIs (owners, reverse dependencies, search endpoints, stats)

## Authentication

- Reads respect Pkgly repository visibility + repository auth settings.
- Writes require Pkgly `Write` permission on the repository.
- RubyGems clients commonly send `Authorization: <token>`; Pkgly also accepts standard Bearer
  tokens (`Authorization: Bearer <token>`).
