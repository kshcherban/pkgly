# Pkgly

Pkgly is an MIT-licensed, self-hosted package registry and artifact manager. Host private packages and cache public registries using a Rust / Axum server, PostgreSQL, and a Vue web console.

## Get started

Follow the [Quickstart](./quickstart.md) to install Pkgly and PostgreSQL with Docker Compose, or deploy the bundled Helm chart on Kubernetes. No source build is required for a container deployment.

## Capabilities

- Hosted, proxy, and virtual repositories across multiple package ecosystems.
- Local disk and S3-compatible artifact storage, with a local disk cache.
- Repository permissions, scoped tokens, SSO, and audit logging.
- Package search, retention policies, and outbound webhooks.
- A [terminal CLI](./cli/index.md) and [MCP server](./cli/mcp.md) for automation.

See the [feature matrix](./features.md) for format support and current limitations. Pkgly is an early-stage project; test your package-manager workflows before migrating production data.

## Development and operations

Read [Development](./development.md) for source builds and tracing, or [Administration](./sysAdmin/index.md) for database, TLS, storage, and authentication configuration.
