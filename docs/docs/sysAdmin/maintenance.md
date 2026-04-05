# Maintenance Operations

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
