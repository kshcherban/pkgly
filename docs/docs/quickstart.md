# Quickstart

This page covers the two fastest ways to get Pkgly running from this repository:

- `docker compose` for a local single-node setup
- the bundled Helm chart for Kubernetes

Both flows start Pkgly with PostgreSQL and then send you to the web installer to create the first admin user.

## Option 1: Docker Compose

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
