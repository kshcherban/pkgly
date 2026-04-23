# Package Webhooks

Pkgly can deliver outbound HTTP webhooks when packages are published or deleted. Webhooks are
configured from **Admin → System → Package Webhooks** and are dispatched asynchronously so package
operations do not wait for the remote endpoint.

## Supported events

- `package.published`
- `package.deleted`

Pkgly emits one webhook per logical package action. It does **not** emit for rejected writes,
missing/no-op deletes, proxy cache maintenance, or failed package operations.

## Delivery model

- Method: `POST`
- Body: fixed JSON payload
- Headers: optional custom headers per webhook
- Queue: durable Postgres-backed delivery table
- Retry policy: 5 total attempts
- Backoff: 1, 2, 4, then 8 minutes between retries
- Retry conditions: timeouts, transport failures, and HTTP `5xx`
- Terminal failure: HTTP `4xx` or exhausted retries

Deliveries are at-least-once. If the process crashes after the remote endpoint receives a payload
but before Pkgly marks the row complete, the worker may retry the same delivery after the claim
lease expires.

## Secret headers

Header values are stored server-side and treated as write-only secrets.

- Reads return header names plus a `configured` flag.
- The UI never receives existing header values back.
- Leaving a configured header value blank during edit preserves the stored secret.
- Removing a header from the form deletes it from future deliveries, but already-queued jobs keep
  the header snapshot they were created with.

## Payload format

Each delivery uses an Artifactory-style envelope:

```json
{
  "domain": "package",
  "event_type": "package.published",
  "event_id": "2fa6f796-9c0b-4f72-8f8f-a1cb1f7c0e4d",
  "occurred_at": "2026-04-22T10:00:00Z",
  "subscription_key": "f38d0ec1-9e9e-4d6f-8f42-3f0c0e70d1af",
  "source": {
    "application": "pkgly",
    "version": "3.0.0-BETA"
  },
  "data": {
    "actor": {
      "id": 1,
      "username": "admin"
    },
    "repository": {
      "id": "2d3d9b6a-6c6d-45aa-8ac3-9f5b4f6d9d4a",
      "name": "npm-hosted",
      "storage_name": "primary",
      "format": "npm"
    },
    "storage": {
      "name": "primary"
    },
    "package": {
      "scope": "acme",
      "key": "@acme/example",
      "name": "example",
      "version": "1.2.3",
      "project_path": "packages/acme/example",
      "version_path": "packages/acme/example/1.2.3",
      "canonical_path": "packages/acme/example/1.2.3/example-1.2.3.tgz",
      "reference": "1.2.3"
    }
  }
}
```

`subscription_key` is the webhook definition ID so downstream systems can identify which
subscription triggered the delivery.

## Operational expectations

- Editing a webhook affects only new deliveries.
- Deleting a webhook removes the definition but does not rewrite jobs already queued.
- Delivery status shown in the UI reflects the latest queued/attempted job for that webhook.
- If a destination is unavailable, package publish/delete requests still succeed as long as the
  package action itself succeeded.
