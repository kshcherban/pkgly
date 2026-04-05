# Pkgly Architecture Overview

Pkgly is split into a Rust back end (multi-crate workspace) and a Vue/Vite front end.

At runtime, Pkgly is a three-part system:
- **Rust application**: serves both the JSON API (`/api/**`) and protocol-specific repository endpoints (for example Docker Registry under `/v2/**`).
- **Postgres**: stores users/auth, repository configuration, and the **package catalog** used for search and package listings.
- **Storage backend**: holds artifact blobs (local filesystem or S3).

## Crate Layout
- `pkgly/`: main server binary (Axum) and CLI. Hosts HTTP server, repository dispatch, authentication, OpenAPI, metrics, search, and background tasks.
- `crates/core`: shared domain types (database entities, repository metadata, project models, security utilities).
- `crates/storage`: abstraction over storage backends (local filesystem, S3). Provides `DynStorage` trait object used by repositories.
- `crates/macros`: derives like `DynRepositoryHandler`.
- `crates/nr-api`: Rust client library for interacting with a Pkgly instance over HTTP.

## Repository Abstractions
- `Repository` trait: defines HTTP handling (`handle_get`, `handle_put`, etc.), metadata (id, visibility, configs).
- `RepositoryType`: factory interface handling descriptor metadata, config validation, repository instantiation from DB.
- Dynamic dispatch via `DynRepository` enum produced by `DynRepositoryHandler` macro.
- Config descriptors (implementing `RepositoryConfigType`) supply schemars schemas, validation, defaults; registered in `REPOSITORY_CONFIG_TYPES`.

## Repository Implementations
- Maven (`maven`): hosted + proxy.
- NPM (`npm`): hosted + proxy + virtual registry; implements the npm publish and login flows.
- Cargo (`cargo`): hosted registry with sparse index support (publish API, index files, crate downloads).
- Python (`python`): hosted + proxy (pip/simple endpoints and cached upstream artifacts).
- PHP Composer (`php`): hosted + proxy (Composer V2 metadata and dist serving).
- RubyGems (`ruby`): hosted + proxy (Compact Index endpoints, `.gem` downloads, publish/yank in hosted mode).
- Docker (`docker`): hosted + proxy, including registry Bearer token auth support.
- Helm (`helm`): hosted chart repository support.
- Go (`go`): hosted + proxy, including module proxy endpoints.
- Debian (`deb`): package repository endpoints and catalog integration.

## HTTP Flow
- `repository_router` in `repo_http.rs`: resolves storage/repo by path, constructs `RepositoryRequest` (HTTP parts, body, parsed `StoragePath`, authentication).
- Repository-specific handler invoked via matching HTTP method.
- `RepoResponse` converts storage/file metadata or custom responses into Axum responses. Shared tracing instrumentation ties into `RepositoryRequestTracing`.

## Authentication
- Session cookies power the browser experience (`/api/user/login` manually verifies credentials and issues a 24h session).
- API tokens expose scoped automation access and authenticate via the `Authorization: Bearer <token>` header.
- Optional SSO support is exposed through `/api/user/sso/login`. When enabled (`SecuritySettings.sso`), Pkgly expects the upstream SSO proxy to forward identity headers (`X-Forwarded-User` by default) and can auto-provision users when `auto_create_users` is true. Administrators can update these settings at runtime via `/api/security/sso` (exposed in the Admin UI) and the values persist in the `application_settings` table.
- Docker Registry clients use the standard Bearer token flow: any write request that reaches `/v2/**` without credentials is answered with a `WWW-Authenticate: Bearer …` challenge whose `realm` points at `/v2/token` and whose `scope` reflects the concrete repository path resolved for the request. The `/v2/token` handler (in `repository::docker::auth`) inspects the incoming authentication (session, password, or long-lived API token), validates the requested repository scopes, and mints a short-lived repository token (`NewRepositoryToken`). The token is hashed at rest, carries its allowed actions, and is returned to the Docker client as the Bearer token that must accompany the retried request.

## Data Persistence
- Postgres via `sqlx` and `nr_core::database::entities`.
- Repository configuration stored in `repository_configs`; retrieved through `DBRepositoryConfig`.
- **Package catalog** stored in `projects` + `project_versions` (includes `repository_id`, `path`, `version`, and JSON metadata in `extra`).
- Repository lookup/registration handled by `Pkgly` state with in-memory caches keyed by UUID and name pair.

## Catalog, Search, and Package Listings
Pkgly’s search and package listing flows are **database-backed** (no storage directory traversal in the hot path).

- Hosted repositories write catalog rows as part of publish/upload handlers.
- Proxy repositories write catalog rows when artifacts are cached/evicted (via `proxy_indexing` and the `DatabaseProxyIndexer` implementation).
- `/api/search/packages` and `/api/repository/<id>/packages` query the catalog; repositories that are present but not yet indexed are surfaced via `X-Pkgly-Warning`.

Operational and query details live in `docs/docs/knowledge/search.md`.

## Storage and Proxy Caching
- Artifact bytes live in the configured storage backend (local filesystem or S3).
- The S3 implementation includes disk caching and defensive behavior for production (for example retrying transient deletion failures and using adaptive buffering for large response bodies under memory pressure).
- Proxy repository behaviors are designed around caching being mandatory for correctness (search, retention, and policy enforcement rely on catalog/indexing).

## Observability
- Request-scoped tracing is wired through repository handling (`RepositoryRequestTracing`) and exported via OpenTelemetry.
- A separate audit stream is emitted at `info` level under the `pkgly::audit` target. It records successful and denied business actions rather than raw transport events, so operators can distinguish package downloads/uploads/deletes, CRUD on users/repositories/storages, security changes, and search/list activity from the lower-level `pkgly::access` request log.
- When running via the dev compose stack, traces are viewable in Jaeger (see `docker-compose.dev.yml` and the project dev workflow).

## Front End Integration
- Vue components under `site/`: repository type configs (`types/<repo>/`), helper views, admin panel integration.
- `site/src/types/repository.ts`: registry of repository types/config components used in UI when creating/managing repositories.

## Build/Test Notes
- Rust workspace managed via Cargo; `cargo build` requires system `pkg-config` + OpenSSL headers (`libssl-dev`/`openssl-devel`).
- Front end built separately with Vite (not covered in this summary).
- Project helpers: `./dev.sh` for full rebuild/start, `./dev.sh -b` for backend rebuild, `npm --prefix site run build` for frontend build.
