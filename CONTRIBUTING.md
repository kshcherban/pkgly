# Contributing

This repository is Pkgly v3: a Rust (Axum) backend + Vue/Vite frontend, backed by Postgres for configuration and the
package catalog (used by search and package listings).

## Prerequisites

- Docker + Docker Compose (used for local dev and integration tests)
- Rust (latest stable; see `rust-toolchain.toml`)
- Node.js (see `site/.node-version`; npm is used as the package manager)

## Quick Start (Full Stack)

Build and start the dev stack:

```bash
./dev.sh
```

Useful endpoints:
- App: `http://localhost:8000`
- OpenAPI docs: `http://localhost:8000/api/docs`
- Jaeger (tracing): `http://localhost:16686` (dev compose only)

Useful commands:
- Stop services: `docker compose -f docker-compose.yml -f docker-compose.dev.yml down`
- Logs: `docker compose -f docker-compose.yml -f docker-compose.dev.yml logs pkgly`

## Backend Development

If you only changed Rust code and want to skip rebuilding the frontend assets:

```bash
./dev.sh -b
```

## Frontend Development

The UI expects an API base URL via `VITE_API_URL` (there is no Vite proxy in this repo).

1. Start the backend with `./dev.sh -b`
2. In another terminal:

```bash
VITE_API_URL=http://127.0.0.1:8000 npm --prefix site run dev
```

## Tests, Lints, and Formatting

Rust:
- `cargo fmt --all`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets`

## Git Hooks (Recommended)

Install the repo's versioned git hooks:

```bash
git config core.hooksPath .githooks
```

This enables the `pre-commit` hook that runs `cargo fmt --all` before each commit.

Frontend:
- `npm --prefix site run test`
- `npm --prefix site run lint`
- `npm --prefix site run type-check`
- `npm --prefix site run build`

Integration tests (Docker-based):
- `./tests/run_integration_tests.sh`

## Project Standards (Non-Negotiable)

- TDD: write tests first; cover error paths and edge cases.
- No `unwrap()` / `expect()` / `panic!` / `todo!` / `unimplemented!` in non-test code (CI enforces workspace clippy lints).
- Prefer small single-purpose functions and explicit error handling (`Result` + `?`).
- Keep tests in dedicated test modules/files (no inline tests inside production source files).
- Search and package listings are catalog-backed (`projects` / `project_versions`): changes to repository publish/cache flows must keep the catalog consistent.

## Documentation Changes

Docs live under `docs/docs`. If you change behavior, APIs, repository types, or operational flows, update the relevant docs
alongside the code change (for example `docs/docs/knowledge/Architecture.md` and `docs/docs/knowledge/search.md`).
