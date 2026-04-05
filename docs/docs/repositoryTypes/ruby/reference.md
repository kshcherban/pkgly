# RubyGems Repository Quick Reference

- Base URL: `https://<host>/repositories/<storage>/<repo>/`
- Supported index:
  - RubyGems **Compact Index** (`/names`, `/versions`, `/info/<gem>`)
  - Legacy RubyGems marshal index (`/specs.4.8.gz`, `/latest_specs.4.8.gz`, `/prerelease_specs.4.8.gz`, `/quick/Marshal.4.8/*.gemspec.rz`)
- Publish API: `POST /api/v1/gems`
- Yank API: `DELETE /api/v1/gems/yank`

## Configuration Templates

### Hosted Repository
```json
{
  "type": "Hosted"
}
```

### Proxy Repository
```json
{
  "type": "Proxy",
  "config": {
    "upstream_url": "https://rubygems.org"
  }
}
```

## Essential Commands

### Bundler: add the repo
```ruby
source "https://<host>/repositories/<storage>/<repo>/"
```

### Bundler: authenticate (private repo)
```bash
bundle config set --global https://<host>/repositories/<storage>/<repo>/ token:<pkgly_token>
```

### Bundler: proxy rubygems.org through Pkgly (proxy repo)
```bash
bundle config set --global mirror.https://rubygems.org https://<host>/repositories/<storage>/<repo>/
```

### Publish (curl)
```bash
curl --data-binary @mygem-1.2.3.gem \
  -H "Authorization: <pkgly_token>" \
  https://<host>/repositories/<storage>/<repo>/api/v1/gems
```

### Publish (gem CLI)
Configure a Pkgly auth token as your RubyGems API key for this host (for example via
`~/.gem/credentials` or `GEM_HOST_API_KEY`), then:

```bash
gem push --host https://<host>/repositories/<storage>/<repo>/ mygem-1.2.3.gem
```

### Yank (curl)
```bash
curl -X DELETE \
  -H "Authorization: <pkgly_token>" \
  -d "gem_name=mygem" -d "version=1.2.3" \
  https://<host>/repositories/<storage>/<repo>/api/v1/gems/yank
```

### Yank (gem CLI)
```bash
gem yank --host https://<host>/repositories/<storage>/<repo>/ mygem -v 1.2.3
```
