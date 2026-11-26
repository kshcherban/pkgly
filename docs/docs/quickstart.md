# Quickstart

This page covers the recommended installer, manual Docker Compose setup, and the bundled Helm chart for Kubernetes.

Both flows start Pkgly with PostgreSQL and then send you to the web installer to create the first admin user.

## Installer (recommended)

Requires Bash, curl, Docker Engine, and Docker Compose v2 on Linux or macOS. Docker Desktop is supported. Run from the directory where you want to keep the installation:

```bash
curl -fsSL https://pkgly.dev/install.sh | bash
```

The installer writes `compose.yaml`, `config.toml`, and `.env` into `./pkgly`, generates a random database password, starts the release image from GHCR with PostgreSQL 17, and waits for `/health`. It does not require a source checkout, Rust, Node.js, S3, or sudo. The default image tag is `v0.1.2`; Linux amd64 and arm64 images are available.

Open `http://localhost:8000/admin/install` and create the first administrator. Sign in, create a **Local** storage using `/data/storages`, and create a repository for your package format. Follow its setup instructions to publish or download your first package. The installer never creates a default administrator or seeds test data.

### Inspect or customize

```bash
curl -fsSL https://pkgly.dev/install.sh -o install.sh
bash install.sh --dir ./my-registry --port 8080 --version v0.1.2 --no-start
cd my-registry
# Review compose.yaml and config.toml before starting:
docker compose up --detach
```

From a source checkout, `bash install.sh --dir ../pkgly-instance` runs the same installer outside the source tree. Re-running it preserves all existing configuration, including the saved image tag and port. An incomplete installation or a nonempty unrelated directory causes an error rather than overwriting operator files. Use `--help` for options.

### Manage the installation

Run these commands from the installation directory:

```bash
cd pkgly
docker compose ps
docker compose logs -f pkgly
docker compose stop
docker compose up --detach
```

Package files and sessions persist in `pkgly_data`; PostgreSQL records persist in `postgres_data`. `docker compose down` preserves these volumes. **`docker compose down --volumes` deletes the installation's data.** Keep the generated `.env`, which identifies the Compose project and its volumes.

For an upgrade, back up the database and artifact storage, choose a [release](https://github.com/kshcherban/pkgly/releases), change the Pkgly image tag in `compose.yaml`, then run `docker compose pull && docker compose up --detach`. Re-running the installer does not perform upgrades. See [Maintenance](./sysAdmin/maintenance.md).

The HTTP port binds to `127.0.0.1`; PostgreSQL has no published port. For remote access, configure an HTTPS reverse proxy and the canonical URL as described in [Administration](./sysAdmin/index.md). If startup fails, inspect `docker compose logs` in the installation directory; files and data remain available for diagnosis and retry.

## Option 1: Manual Docker Compose


Use this when you want the quickest local setup on one machine.

### Prerequisites

- Docker Engine
- Docker Compose plugin

### Start the stack

```bash
git clone https://github.com/kshcherban/pkgly.git
cd pkgly
docker compose up --detach
```

What this uses:

- `docker-compose.yml` starts `postgres` and `pkgly`
- `docker/config.toml` is mounted into the container at `/data/config.toml`
- Docker volumes `pkgly_data` and `postgres_data` persist application and database data

### Finish installation

Open `http://localhost:8000`.

On first start, Pkgly redirects to `/admin/install`. Use that page to create the first admin account. After that, sign in and create storages and repositories from the UI.

### Useful commands

```bash
docker compose ps
docker compose logs -f pkgly
docker compose down
```

To reset the instance completely:

```bash
docker compose down -v
```

## Option 2: Helm Chart

Use this when you want a Kubernetes deployment quickly, with PostgreSQL managed by the chart.

The chart uses `GET /health` as the Pkgly liveness probe. The endpoint becomes available after Pkgly initializes and connects to PostgreSQL.

### Prerequisites

- Kubernetes cluster
- Helm 3
- `kubectl` configured for the target cluster

### Install

```bash
git clone https://github.com/kshcherban/pkgly.git
cd pkgly
helm dependency build charts/pkgly
helm install pkgly ./charts/pkgly --namespace pkgly --create-namespace
```

The default chart configuration:

- deploys Pkgly as a single replica
- deploys PostgreSQL through the Bitnami chart dependency
- creates a persistent volume claim for `/data`
- exposes Pkgly on port `6742` with a `ClusterIP` service

### Access the UI

For a quick local check, port-forward the service:

```bash
kubectl port-forward -n pkgly svc/pkgly 6742:6742
```

Then open `http://localhost:6742` and complete the `/admin/install` form to create the first admin user.

### Minimal values override

If you want an ingress hostname immediately, create a small values file:

```yaml
site:
  appUrl: "http://pkgly.local"
  isHttps: false

ingress:
  enabled: true
  hosts:
    - host: pkgly.local
      paths:
        - path: /
          pathType: Prefix
```

Install with it:

```bash
helm install pkgly ./charts/pkgly \
  --namespace pkgly \
  --create-namespace \
  -f values.quickstart.yaml
```

### Useful commands

```bash
helm list -n pkgly
kubectl get pods -n pkgly
kubectl logs -n pkgly deploy/pkgly
helm uninstall pkgly -n pkgly
```

## Next steps

- For day-2 operations, see [Maintenance](./sysAdmin/maintenance.md).
- For object storage instead of local disk, see [Configuring S3 Storage](./sysAdmin/s3.md).
- For repository-specific client setup, see the [Repository Types](./repositoryTypes/index.md) docs.
