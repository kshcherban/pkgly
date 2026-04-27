# Search Architecture & Operations

Pkgly’s search experience is built entirely on catalog metadata stored in Postgres.

This guide explains how the pipeline works, how to interact with `/api/search/packages`, what the UI surfaces, and how to keep the system healthy in production.

## 1. End-to-End Flow

| Stage | Hosted repositories | Proxy repositories |
| ----- | ------------------- | ------------------ |
| **Write metadata** | Upload handlers create/update `projects` + `project_versions`; package rows are mirrored into `package_files` (`NewVersion::insert` + update sync) | Cache fills and evictions update `project_versions` metadata and the `package_files` row set |
| **Persist columns** | `package_files` stores row-level package view/search fields (`package`, `name`, `path`, `size_bytes`, digests, timestamps) | Same table, with `upstream_digest` populated when available |
| **Query path** | `/api/search/packages` and `/api/repository/<id>/packages` read from `package_files` | same |
| **Response** | JSON list of `PackageSearchResult` entries + optional `X-Pkgly-Warning` header when a repository has zero indexed rows | same |

### Key tables & indexes
- `projects(id, repository_id, key, name, path, …)`
- `project_versions(id, project_id, repository_id, version, path, extra JSONB, updated_at, created_at)`
- `package_files(id, repository_id, project_id, project_version_id, package, name, path, size_bytes, content_digest, upstream_digest, modified_at, deleted_at)`
- Composite indexes added in migration `20251130123000_add_search_indexes.*`:
  - `(repository_id, updated_at DESC)`
  - `(repository_id, lower(path))`
  - GIN on `extra` for metadata filtering
- Row catalog indexes added in migration `20260211100000_package_files.*`:
  - unique `(repository_id, lower(path))`
  - listing `(repository_id, modified_at DESC, id DESC)`
  - search `(repository_id, lower(package), lower(name))` + digest lookup

## 2. Search API Contract

```
GET /api/search/packages?q=<query>&limit=<1-200>
```

### Query syntax
- Free-text terms (minimum 2 characters unless filters are present)
- Field filters using the query parser (`package:foo`, `repository:npm-public`, `type:docker`, `storage:primary`)
- Digest filters: `digest:sha256:...` or `hash:deadbeef`
- Version constraints: `version:=1.2.3`, `version:>1.0.0`, or semver ranges (`version:^1.2`)

### Behavior
1. Pkgly iterates through **loaded repositories** (hidden repositories are skipped).
2. For each repository, it runs parameterized SQL that pushes filters to Postgres and caps results locally with the request limit.
3. If a repository returns no rows **and** the catalog lacks entries (`repository_has_index_rows`), the response header `X-Pkgly-Warning` is populated with `Repositories awaiting indexing: <name,…>`.
4. `PackageSearchResult` fields:
   - `repository_id`, `repository_name`, `storage_name`, `repository_type`
   - `file_name` (deb packages use the `.deb` filename, others default to `name@version`)
   - `cache_path`, `size`, `modified` (UTC timestamp)

### Example response

```json
HTTP/1.1 200 OK
X-Pkgly-Warning: Repositories awaiting indexing: docker-proxy

[
  {
    "repository_id": "fc9f…",
    "repository_name": "helm-public",
    "storage_name": "primary",
    "repository_type": "helm",
    "file_name": "nginx@2.0.1",
    "cache_path": "charts/nginx/2.0.1/chart.tgz",
    "size": 183420,
    "modified": "2025-11-30T18:51:13.000+00:00"
  }
]
```

## 3. UI Surfaces

- **Home / Public repository list**: uses `/api/search/packages`; when `X-Pkgly-Warning` is present a toast (“Repository indexing in progress”) is raised.
- **Repository package tabs (admin & public)**: the package listing endpoints now probe catalog state and echo `X-Pkgly-Warning` so the UI can:
  - Display inline banners informing operators/end-users that indexing is still running.
  - Adjust empty-state copy (“Indexing in progress” vs “No packages yet”).
- **Proxy configuration forms (Docker, Maven, Go, NPM, Python)**: show a `ProxyCacheNotice` component reminding operators that caching is mandatory for search and policy enforcement.

## 4. Operating the Catalog

### Detecting gaps
- Watch for `X-Pkgly-Warning` headers in:
  - `/api/search/packages`
  - `/api/repository/<id>/packages`
- SRE dashboards should alert when warnings cross your tolerated threshold (e.g., more than 5 repositories returning warnings for >15 minutes).

### Reindex tooling
Current CLI coverage:

```bash
pkgly search reindex <target> --repository <uuid>
```

General checklist:
1. Confirm artifacts exist on storage (S3/local) and proxy cache toggles remain removed.
2. Run the appropriate `search reindex` command.
3. Re-run `/api/search/packages` to verify results appear and headers disappear.

### Deletions
When deleting packages:
1. Pkgly deletes DB rows first (`delete_version_records_by_path` or `ProxyIndexing::evict_cached_artifact`).
2. Storage objects are removed next; failures queue retries (proxy caches) or surface as API errors (hosted).
3. A background reconciliation task (planned) will watch for orphaned rows.

### Retention
Per-repository package retention depends on `package_files` for candidate selection. It groups by `package_files.package`, ranks newest files by `modified_at DESC, id DESC`, and sends selected paths through the same internal deletion path as manual package deletes.

## 5. Metrics & Benchmarks

Exported via OpenTelemetry (`pkgly/src/search/query.rs`):
- `search.query.duration_ms` (histogram, milliseconds)
- `search.query.rows` (histogram, row counts per repository query)

Recommended Prometheus alerts:
- High p95 query latency (>50 ms) sustained for 5 minutes.
- Zero rows returned while metadata exists (implies filters overly strict or missing indexes).

### Benchmark harness
`benches/search_db.rs` seeds 10k manifests and exercises the search query. Run it locally or in CI with:

```bash
PKGLY_SEARCH_BENCH_DSN=postgres://user:pass@localhost:5432/pkgly_bench cargo bench search_db_query
```

Use this to validate schema/index changes before rollout.

## 6. Troubleshooting Checklist

| Symptom | Probable cause | Action |
| ------- | -------------- | ------ |
| `X-Pkgly-Warning` for specific repositories | Catalog empty after migration or cache disabled | Verify proxy caching is enabled (it is always-on in v3), run targeted reindex |
| Search results missing newly uploaded packages | Upload handler crashed before metadata insert | Check API logs for `delete_version_records_by_path` errors, re-upload or reindex |
| API latency spikes >100 ms | Postgres lacking indexes, runaway queries, or replica lag | Inspect `search.query.duration_ms`, review query plans for `project_versions` |
| CLI reindex slow | Running during peak traffic or using insufficient concurrency | Schedule reindex during maintenance windows; ensure Postgres VACUUM/autovacuum healthy |
| UI still shows “Indexing” after backfill | Browser cached header or user stayed on old view | Refresh view, check network panel to ensure header cleared |

## 7. Implementation References

- Backend search logic: `pkgly/src/app/api/search.rs`, `pkgly/src/app/api/search/database.rs`
- Query builder + metrics: `pkgly/src/search/query.rs`
- Repository catalog integration: `pkgly/src/repository/proxy_indexing.rs` and hosted delete flows
- UI warning banners: `site/src/components/admin/repository/RepositoryPackagesTab.vue`, `site/src/components/nr/repository/RepositoryPackagesPublic.vue`
- Proxy cache notice component: `site/src/components/nr/repository/ProxyCacheNotice.vue`

Keeping search healthy is primarily about ensuring every repository writes metadata consistently. Monitor the warnings, keep Postgres tuned, and build reindex muscle memory before migrations.
