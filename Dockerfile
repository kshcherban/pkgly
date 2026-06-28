# syntax=docker/dockerfile:1.6
# ABOUTME: Builds frontend assets, Rust release binaries, and runtime image for pkgly.
# ABOUTME: Uses cacheable dependency layers so CI rebuilds avoid recompiling unchanged crates.
############################
# Frontend build stage
############################
FROM node:25-trixie AS frontend-builder
WORKDIR /app/site

COPY site/package*.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY site .
RUN --mount=type=cache,target=/root/.npm npm run build

############################
# Rust build stage
############################
FROM rust:1.95.0 AS rust-base
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install cargo-chef --locked

FROM rust-base AS rust-planner

COPY Cargo.lock Cargo.toml ./
COPY crates crates
COPY pkgly pkgly

RUN cargo chef prepare --recipe-path recipe.json

FROM rust-base AS rust-deps

COPY --from=rust-planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

FROM rust-deps AS rust-builder

COPY Cargo.lock Cargo.toml ./
COPY crates crates
COPY pkgly pkgly
COPY --from=frontend-builder /app/site/dist ./site/dist

ENV FRONTEND_DIST=/app/site/dist

ARG CARGO_INCREMENTAL=1
ARG PKGLY_COMMIT_ID
ARG TARGETPLATFORM

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target/release/build,sharing=locked,id=pkgly-target-${TARGETPLATFORM}-build-${CARGO_INCREMENTAL},from=rust-deps,source=/app/target/release/build \
    --mount=type=cache,target=/app/target/release/deps,sharing=locked,id=pkgly-target-${TARGETPLATFORM}-deps-${CARGO_INCREMENTAL},from=rust-deps,source=/app/target/release/deps \
    --mount=type=cache,target=/app/target/release/incremental,sharing=locked,id=pkgly-target-${TARGETPLATFORM}-incremental-${CARGO_INCREMENTAL},from=rust-deps,source=/app/target/release/incremental \
    --mount=type=cache,target=/app/target/release/.fingerprint,sharing=locked,id=pkgly-target-${TARGETPLATFORM}-fingerprint-${CARGO_INCREMENTAL},from=rust-deps,source=/app/target/release/.fingerprint \
    CARGO_INCREMENTAL="${CARGO_INCREMENTAL}" PKGLY_COMMIT_ID="${PKGLY_COMMIT_ID}" cargo build --release --features frontend

############################
# Runtime stage
############################
FROM debian:trixie-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=rust-builder /app/target/release/pkgly ./pkgly

EXPOSE 6742
VOLUME ["/data"]

ENV RUST_LOG=info

ENTRYPOINT ["./pkgly"]
CMD ["start"]
