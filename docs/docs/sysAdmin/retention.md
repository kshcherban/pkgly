# Package Retention

Package retention removes old package files from hosted and proxy repositories. It is disabled by default and is configured per repository.

Virtual repositories do not store packages directly, so they do not support retention.

## Defaults

```json
{
  "enabled": false,
  "max_age_days": 30,
  "keep_latest_per_package": 1
}
```

- `enabled`: retention only runs when this is `true`.
- `max_age_days`: package files must be older than this many days before they can be deleted. Minimum: `1`.
- `keep_latest_per_package`: newest files to keep for each `package_files.package` group. Set `0` to allow all old files in a package group to be deleted.

## Scheduler

Pkgly checks retention work from the background scheduler and runs a repository at most once every 24 hours. Progress is tracked in `package_retention_status`.

Each run uses a Postgres advisory lock per repository. If another retention run is already active for the same repository, the scheduler skips it and records no duplicate work.

## API

Get the default:

```http
GET /api/repository/config/package_retention/default
```

Get the JSON schema:

```http
GET /api/repository/config/package_retention/schema
```

Get the description:

```http
GET /api/repository/config/package_retention/description
```

Enable or update retention on a repository:

```http
PUT /api/repository/{repository_id}/config/package_retention
Content-Type: application/json

{
  "enabled": true,
  "max_age_days": 30,
  "keep_latest_per_package": 1
}
```

Create a non-virtual repository with retention preconfigured:

```http
POST /api/repository/new/maven
Content-Type: application/json

{
  "name": "maven-internal",
  "storage": "00000000-0000-0000-0000-000000000000",
  "configs": {
    "maven": { "type": "Hosted" },
    "package_retention": {
      "enabled": true,
      "max_age_days": 90,
      "keep_latest_per_package": 2
    }
  }
}
```

Requests that read, write, or create `package_retention` for virtual repositories return an unsupported-config error.

## Deletion Behavior

Retention selects active rows from `package_files`. A file is eligible only when:

- `deleted_at` is `NULL`
- `modified_at` is older than `max_age_days`
- it is not among the newest `keep_latest_per_package` files for its package group

Newest files are ranked by `modified_at DESC, id DESC` for deterministic ties.

Retention uses the same internal deletion path as manual package deletes. That keeps Docker manifest/blob cleanup, Go sibling files, Helm chart metadata, proxy cache eviction, catalog cleanup, `package_files` soft deletes, and package delete webhooks consistent.

## Operational Cautions

- Start with `enabled: false`, review package listings, then enable with conservative limits.
- Lowering `keep_latest_per_package` or `max_age_days` can delete many files on the next run.
- Retention depends on the package catalog. If a repository has missing `package_files` rows, run the relevant reindex operation before relying on retention.
- Deleted packages follow normal repository delete semantics and may affect clients that pin old versions.
