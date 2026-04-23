# Feature Overview

Pkgly focuses on shipping a lean, reliable artifact service. The tables below capture the
repository types and platform capabilities that are available **today** so you can plan deployments
without guesswork.

## Repository Support

| Repository | Hosted | Proxy | Notes |
| --- | --- | --- | --- |
| Maven | Yes | Yes | Standard Maven 2 layout for private artifacts or upstream caching ([docs](./repositoryTypes/maven/index.md)). |
| npm | Yes | Yes | Works as a private registry or smart cache with multiple upstreams ([docs](./repositoryTypes/npm/index.md)). |
| Python | Yes | Yes | Hosts wheels/sdists and proxies PyPI-compatible indexes, including `uv` workflows ([docs](./repositoryTypes/python/index.md)). |
| Composer (PHP) | Yes | Yes | Composer V2 repository for private packages or Packagist proxy caching ([docs](./repositoryTypes/php/index.md)). |
| Go modules | Yes | Yes | Athens-style hosted uploads plus multi-route proxy cache control ([docs](./repositoryTypes/go/index.md)). |
| Cargo (Rust) | Yes | No | Sparse index + publish/download endpoints for private crates ([docs](./repositoryTypes/cargo/index.md)). |
| RubyGems | Yes | Yes | RubyGems-compatible hosted/proxy repositories with Compact Index + Bundler support ([docs](./repositoryTypes/ruby/index.md)). |
| Docker / OCI | Yes | Yes | Private Docker Registry HTTP API v2 implementation plus pull-through caching for upstream registries ([docs](./repositoryTypes/docker/index.md)). |
| Helm | Yes | No | HTTP chart repository and OCI distribution registry with unified package management ([docs](./repositoryTypes/helm/index.md)). |
| Debian (APT) | Yes | Yes | Hosted APT repos plus proxy/mirror caching for upstream `dists/`/`pool/` trees ([docs](./repositoryTypes/deb/index.md)). |
| NuGet | Yes | Yes | NuGet V3 hosted and proxy repositories plus virtual aggregation with hosted publish forwarding ([docs](./repositoryTypes/nuget/index.md)). |

## Platform Capabilities

| Feature | Status | Notes |
| --- | --- | --- |
| API / Token Security | Yes | Scoped automation tokens back every repository type; Docker additionally mints short-lived bearer tokens for `/v2/**` routes ([architecture](./knowledge/Architecture.md#authentication)). |
| Repository Search | Yes | Global and per-repository search filter packages, tags, and metadata (including Docker manifests) directly from the UI and API. |
| Audit Logging | Yes | Dedicated `info`-level audit events under `pkgly::audit` record successful and denied user actions across API management routes, package operations, search/listing, and repository protocol traffic. |
| SSO Support | Yes | Optional SSO proxy integration with auto-provisioned users when enabled in security settings ([docs](./sso/index.md)). |
| Package Webhooks | Yes | Admin-managed outbound webhooks for `package.published` and `package.deleted`, backed by a durable retry queue and write-only custom headers. |
| S3 Disk Cache | Yes | Configurable on-disk LRU cache keeps hot artifacts locally; knobs for path, byte cap, and entry cap are exposed in the Admin UI. |

Use this page as the single source of truth when deciding which repository modes to enable or when
planning migrations from other managers (Nexus, StrongBox, Reposilite, etc.). If a capability is
marked **Planned**, follow the linked issue to track progress.
