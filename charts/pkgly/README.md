# Pkgly Helm Chart

This Helm chart deploys Pkgly, a universal artifact repository supporting Docker, Go, Maven, NPM, PHP, Python, and Helm packages.

## Features

- **Multi-protocol support**: HTTP, OCI, and native package manager protocols
- **Repository types**: Docker, Go proxy, Maven, NPM, PHP Composer, Python PyPI, Helm charts
- **Database**: PostgreSQL (included as dependency)
- **Tracing**: Jaeger integration (optional)
- **Storage**: Local filesystem (with PVC support)
- **Security**: Security contexts, resource limits, health checks
- **Ingress**: Nginx ingress support with large file upload handling

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- PV provisioner support in the infrastructure (for persistence)

## Installing the Chart

Add the repository and install the chart:

```bash
# Install dependencies
helm repo add bitnami https://charts.bitnami.com/bitnami
helm repo add jaegertracing https://jaegertracing.github.io/helm-charts
helm repo update

# Install the chart
helm install my-pkgly ./pkgly-chart
```

## Uninstalling the Chart

```bash
helm uninstall my-pkgly
```

## Configuration

See `values.yaml` for configuration options. The following table shows the configurable parameters:

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of Pkgly replicas | `1` |
| `image.repository` | Container image repository | `ghcr.io/sudoers/pkgly` |
| `image.tag` | Container image tag | `latest` |
| `service.type` | Kubernetes service type | `ClusterIP` |
| `service.port` | Service port | `6742` |
| `ingress.enabled` | Enable ingress | `false` |
| `postgresql.enabled` | Deploy PostgreSQL | `true` |
| `jaeger.enabled` | Deploy Jaeger for tracing | `false` |
| `persistence.enabled` | Enable persistent storage | `true` |
| `persistence.size` | Storage size | `10Gi` |
| `rustLog` | Rust log level | `info` |
| `env` | Custom environment variables with secret support | `[]` |
| `externalDatabase.secretRef.enabled` | Use secret for external database URL | `false` |

### Development Mode

For development with Jaeger tracing enabled:

```yaml
tracing:
  enabled: true
jaeger:
  enabled: true
rustLog: "debug"
```

### Production Mode

For production deployment:

```yaml
replicaCount: 3
ingress:
  enabled: true
  hosts:
    - host: pkgly.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: pkgly-tls
      hosts:
        - pkgly.example.com
postgresql:
  auth:
    existingSecret: pkgly-db-secret
```

## Persistence

The chart uses a PersistentVolumeClaim for storing repository data. The PVC is created by default and uses the default storage class.

## Database

By default, the chart deploys PostgreSQL as a dependency. You can disable it and use an external database:

### External Database with Secret (Recommended)

For production environments, use a Kubernetes secret to store the database URL:

```yaml
postgresql:
  enabled: false
externalDatabase:
  secretRef:
    enabled: true
    name: database-secret
    key: url
    optional: false
```

Create the secret:

```bash
kubectl create secret generic database-secret \
  --from-literal=url="postgresql://user:password@host:5432/database"
```

### External Database with Direct URL

For development environments:

```yaml
postgresql:
  enabled: false
externalDatabase:
  url: "postgresql://user:password@host:5432/database"
  secretRef:
    enabled: false
```

## Security Configuration

### Environment Variables with Secret References

The chart supports custom environment variables with Kubernetes secret references:

```yaml
env:
  - name: DATABASE_URL
    valueFrom:
      secretKeyRef:
        name: database-secret
        key: url
        optional: false
  - name: API_KEY
    valueFrom:
      secretKeyRef:
        name: api-secret
        key: api-key
  - name: LOG_LEVEL
    value: "info"
```

### Best Practices

1. **Never store passwords in values.yaml** - Use Kubernetes secrets instead
2. **Use secret references** for sensitive data like database URLs, API keys, and tokens
3. **Enable RBAC** - Restrict access to secrets containing sensitive data
4. **Use external managed databases** in production when possible
5. **Rotate secrets regularly** and update your deployments

## Accessing Pkgly

- **Web UI**: `http://<service-name>.<namespace>.svc.cluster.local:6742`
- **API**: Same URL, with appropriate endpoints
- **Default credentials**: `admin` / `admin123` (change `adminPassword` in production)

## Repository URLs

Once deployed, repositories are accessible at:

- Docker Registry: `http://<host>/v2/`
- Go Proxy: `http://<host>/go/`
- Maven: `http://<host>/maven/`
- NPM: `http://<host>/npm/`
- Helm: `http://<host>/helm/`

## Upgrading

```bash
helm upgrade my-pkgly ./pkgly-chart
```

## Contributing

This chart is part of the Pkgly project. Please submit issues and PRs to the main repository.