# Development

## Build from source

Use the Rust toolchain specified in `rust-toolchain.toml` and the Node.js version in `site/.node-version`. On Linux, install `pkg-config` and the OpenSSL development headers (`libssl-dev` on Debian or Ubuntu, `openssl-devel` on Fedora or RHEL).

Run these commands from the repository root to build the server with the web UI embedded.

```bash
npm --prefix site ci
npm --prefix site run build
cargo build --release --features frontend
```

The server binary is `target/release/pkgly`. See [Installation and Configuration](./sysAdmin/index.md) for PostgreSQL and server setup.

## Run the development stack

With Docker and Docker Compose available, run `./dev.sh` from the repository root after installing the frontend dependencies. On macOS, the script uses Zig and `cargo-zigbuild` to build for `aarch64-unknown-linux-gnu`.

The app is available at `http://localhost:8000` and the API documentation at `http://localhost:8000/api/docs`. Use `./dev.sh -b` to reuse the frontend assets after changing only the backend.

## Tracing and logging

The development stack enables OpenTelemetry tracing and sends traces to Jaeger. Open `http://localhost:16686` and select the `pkgly` service to inspect requests.

View application logs with the development Compose configuration.

```bash
docker compose -f docker-compose.yml -f docker-compose.dev.yml logs pkgly
```

Set the default and per-target log levels under `[log.levels]` and `[log.levels.others]` in the server configuration. The development stack uses `docker/config.dev.toml`. To export logs via OpenTelemetry, set `enabled = true` and `logs = true` in the `[opentelemetry]` section and configure the collector endpoint.

When instrumenting Axum handlers, skip application state, authentication objects, and request bodies in `#[instrument]`. Record small identifiers such as repository or user IDs as explicit span fields.
