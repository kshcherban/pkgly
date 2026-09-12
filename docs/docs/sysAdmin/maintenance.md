# Maintenance Operations
<!-- ABOUTME: Documents operational maintenance workflows for Pkgly deployments. -->
<!-- ABOUTME: Covers migrations, restarts, audit logs, storage deletion, and web refresh behavior. -->

This page describes the supported procedure for applying schema migrations and restarting Pkgly in a production environment.

## Applying database migrations

Pkgly uses SQL files under `crates/core/migrations`. To apply them manually:

1. **Back up the database** using your standard tooling.
2. **Connect to Postgres** with the same user Pkgly runs under. Example using `psql`:
   ```bash
   psql "$DATABASE_URL"
   ```
3. **Run the pending migration scripts** in chronological order. Each migration consists of an `*.up.sql` file. For example:
   ```bash
   \i crates/core/migrations/20251115103000_repository_indexes.up.sql
   ```
   Repeat for each newer migration. Keep the session open so you can roll back with the matching `*.down.sql` if needed.
4. **Verify** with `SELECT` or `\d` that the new indexes/tables exist.

> Tip: If you manage Postgres with a migration runner (e.g., `sqlx-cli` or `just migrate`), use that wrapper instead of applying SQL manually.

## Restarting Pkgly services

After migrations, rebuild and restart the services:

1. **Stop the running stack**:
   ```bash
   docker compose down
   ```
2. **Rebuild** (if code changed):
   ```bash
   ./dev.sh
   ```
   or run `npm --prefix site run build && cargo build --features frontend`
3. **Start**:
   ```bash
   docker compose up -d
   ```
4. **Check logs** for migration output:
   ```bash
   docker compose logs pkgly
   ```

Keep maintenance windows short: apply migrations first, then restart services once the schema is in place so requests hitting old binaries do not fail mid-migration.

## Audit logging

Pkgly now emits a dedicated audit stream at `info` level under the `pkgly::audit`
target.

What is logged:

- Successful user actions on the management API (`/api/**`) such as user, storage, repository, security, and token operations.
- Successful and denied package operations routed through repository protocol endpoints (`/v2/**`, `/repositories/**`, direct `/{storage}/{repository}/...` paths).
- Successful and denied search, package listing, browse, and websocket browse actions.

What is not logged in the first pass:

- Static/frontend asset requests.
- `/api/info` and similar low-value informational routes.
- Validation, conflict, not-found, or internal-error outcomes unless the result is an authorization failure (`401` or `403`).

Important fields:

- `action`
- `outcome`
- `actor_username`
- `actor_id`
- `repository_id`
- `storage_id`
- `path`
- `trace_id`

The existing access log target, `pkgly::access`, is still emitted separately. Use:

```bash
docker compose logs pkgly | grep 'pkgly::audit'
```

If you run JSON logs, filter on `"target":"pkgly::audit"` and join on `trace_id` when you need to correlate an audit event with lower-level request or tracing data.

HTTP access logs on the `pkgly::access` target include request identity and routing fields such as `trace_id`, `request_id`, `http.request.method`, `http.route`, `url.path`, and `http.response.status_code`. They also include `client.address` when Pkgly can determine it from `X-Forwarded-For` or the connection IP, and `user_agent.original` when the request sends a user-agent header.

## Storage usage refresh

Pkgly caches each repository's storage usage in the repository row. The background scheduler checks storage usage every 30 seconds, but each repository is recalculated at most once per hour. Repositories with no cached usage are refreshed on the next scheduler tick after startup.

Manual API reads can still request usage with `include_usage=true`, and admins can force recalculation with `refresh_usage=true`.

## Deleting a storage

Storage deletion is available to admins and system managers through `DELETE /api/storage/{id}`. By default the request is sent without cascade approval:

```bash
curl -X DELETE -H "Authorization: Bearer $PKGLY_TOKEN" \
  "https://pkgly.example.com/api/storage/$STORAGE_ID"
```

- **Empty storage** (no repositories) is deleted immediately and the API returns `204`.
- **Populated storage** returns `409 Conflict` without changing anything. The body uses the standard error envelope and includes a machine-readable code and the repository count:
  ```json
  {
    "message": "Storage contains repositories. Re-request with cascade=true to delete them.",
    "details": { "code": "storage_not_empty", "repository_count": 2 }
  }
  ```
- Retry with `?cascade=true` to delete every contained repository and package:
  ```bash
  curl -X DELETE -H "Authorization: Bearer $PKGLY_TOKEN" \
    "https://pkgly.example.com/api/storage/$STORAGE_ID?cascade=true"
  ```

Cleanup boundaries and ordering:

1. Pkgly locks the storage row, reads repository membership from the database, and holds an in-process management lock so repository creation/deletion and storage configuration updates cannot interleave with the deletion.
2. If the storage is populated, the runtime backend must be loaded. An unavailable backend fails the request before anything is deleted.
3. Physical contents are removed per repository through the Local or S3 `delete_repository` operation. Only registered repository directories or S3 prefixes are touched; the configured parent directory, bucket, and unrelated contents are left in place.
4. Only after every cleanup succeeds does Pkgly delete the storage row and commit the existing database cascades. Runtime repositories, name lookups, and the storage registration are then removed.

Retry behavior: if a physical cleanup fails, the transaction is rolled back and the database records and runtime registrations are retained so deletion can be retried once the backend recovers. Note that physical files already removed cannot be restored by a database rollback. Draining already-running uploads is outside this workflow.

Failure responses use the standard error envelope with an actionable `message` and a machine-readable `details.code`:

- `storage_backend_unavailable` (`500`): the backend was not loaded, so nothing was deleted. Restore the backend and retry.
- `storage_cleanup_failed` (`500`): at least one repository could not be removed. The storage and its repository records remain so the deletion can be retried; `details.repository_id` names the failing repository, `details.repositories_remaining` counts the retained records, and `details.detail` carries the backend error. Files already removed are not restored.

## Browser refresh routing

Pkgly serves the Vue app with history-mode routes. Browser refreshes for paths present in `site/src/router/routes.json` return the SPA `index.html` when the request is a `GET` or `HEAD` with `Accept: text/html`.

Package manager endpoints are still handled by repository routes. Requests under `/api/**`, `/v2/**`, `/repositories/**`, `/storages/**`, and direct package requests that do not ask for HTML keep their package/API behavior.

For HTTP deployments, session cookies use `SameSite=Lax` without the `Secure` attribute. HTTPS deployments use `SameSite=None` with `Secure`. This lets local and plain-HTTP installs preserve a valid login across browser refreshes while keeping cross-site cookie compatibility for HTTPS.
