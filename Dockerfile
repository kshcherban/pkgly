# syntax=docker/dockerfile:1.6
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
FROM rust:1.90.0 AS rust-builder
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.lock Cargo.toml ./
COPY docs docs
COPY site site
COPY crates crates
COPY pkgly pkgly
COPY --from=frontend-builder /app/site/dist ./site/dist

ENV FRONTEND_DIST=/app/site/dist

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target/release/build \
    --mount=type=cache,target=/app/target/release/deps \
    --mount=type=cache,target=/app/target/release/incremental \
    --mount=type=cache,target=/app/target/release/.fingerprint \
    cargo build --release --features frontend

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
