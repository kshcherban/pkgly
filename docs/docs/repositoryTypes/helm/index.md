# Helm Repositories

Pkgly delivers first-class Helm repository hosting with support for both classic HTTP chart clients and the OCI-based workflows introduced in Helm v3. Create a Helm repository from the Admin UI and choose the repository mode that matches your deployment:

- **HTTP (default)** – behaves like a traditional chart repository (`helm repo add`, `helm install`).
- **OCI** – exposes only the distribution-spec API (`helm push`, `helm pull`).

## Configuration

| Setting | Description |
| ------- | ----------- |
| `Allow Overwrite` | Permits overwriting an existing chart version. Disabled by default. |
| `Public Base URL` | Optional fully qualified URL advertised inside `index.yaml`. Useful when Pkgly sits behind a reverse proxy. |
| `Index Cache TTL` | Server-side cache duration (seconds) for rendered `index.yaml`. Use `0`/blank to disable caching. |
| `Max Chart Size` | Rejects uploads larger than the configured number of bytes. Defaults to 10&nbsp;MiB. |
| `Max Files Per Chart` | Caps the number of files allowed inside a chart archive. Defaults to 1,024. |

## Uploading Charts

### HTTP (ChartMuseum compatible)

```bash
helm package ./charts/webapp
curl -u token:secret \
  -T webapp-1.0.0.tgz \
  https://pkgly.example.com/repositories/default/helm/webapp-1.0.0.tgz
```

### OCI (Helm v3)

```bash
helm registry login pkgly.example.com --username token --password secret

# URL form with explicit storage/repo prefix
helm push webapp-1.0.0.tgz oci://pkgly.example.com/repositories/default/helm

# Short form also supported; Pkgly infers storage/repo
helm push webapp-1.0.0.tgz oci://pkgly.example.com/default/helm
```

HTTP and OCI modes operate independently—Pkgly no longer mirrors uploads between the two protocols.

> **Tip:** The OCI URL path is translated by Pkgly so either `oci://pkgly/repositories/<storage>/<repo>` or `oci://pkgly/<storage>/<repo>` resolves to the same repository.

## Admin UI & Package Management

The Admin view for a Helm repository now includes a **Packages** tab that lists every uploaded chart version, no matter whether it arrived through HTTP or the OCI registry. The table supports filtering, paging, and bulk deletion:

- Deleting a chart version from the UI removes the HTTP artifact (when present), the OCI manifest, and any associated blobs/config layers.
- Metadata originates from the chart archive itself, so entries uploaded via `helm push` appear in the list immediately.

You can invoke the same behaviour through the REST API at `/api/repository/<id>/packages`; see the [route reference](./routes.md#admin-api--packages) for sample requests.

## Further Reading

- [HTTP & OCI Route Reference](./routes.md) – detailed list of every Helm endpoint exposed by Pkgly.
