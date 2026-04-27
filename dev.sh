#!/bin/bash

usage() {
  cat <<'EOF'
Usage: ./dev.sh [options]

Options:
  -b              Skip frontend build
  --release, -r   Build backend in release mode
  --stop          Stop and remove containers
  --status        Show container status
  -h, --help      Show this help
EOF
}

# Parse command line arguments
SKIP_FRONTEND=false
RELEASE=false
STOP=false
STATUS=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    -b)
      SKIP_FRONTEND=true
      shift
      ;;
    --release|-r)
      RELEASE=true
      shift
      ;;
    --stop)
      STOP=true
      shift
      ;;
    --status)
      STATUS=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

set -ex

if [[ "$STOP" = true ]]; then
  docker compose -f docker-compose.yml -f docker-compose.dev.yml down
  exit 0
fi

if [[ "$STATUS" = true ]]; then
  docker compose -f docker-compose.yml -f docker-compose.dev.yml ps
  exit 0
fi

# Build frontend unless -b flag is provided
if [[ "$SKIP_FRONTEND" = false ]]; then
  export VITE_BASE_URL=/
  if [[ "$RELEASE" = true ]]; then
    npm --prefix site run build-only
  else
    npm --prefix site run build-only -- --mode development --minify false --sourcemap true
  fi
fi

# Build backend based on OS
case "$(uname -s)" in
  Linux)
    if [[ "$RELEASE" = true ]]; then
      export PKGLY_BINARY_PATH="./target/release/pkgly"
      cargo build --release --features frontend
    else
      export PKGLY_BINARY_PATH="./target/debug/pkgly"
      cargo build --features frontend
    fi
    ;;
  Darwin)
    # Install zig and cargo-zigbuild if not already installed
    # brew install zig
    # cargo install cargo-zigbuild
    # rustup target add aarch64-unknown-linux-gnu
    ulimit -n 65536
    if [[ "$RELEASE" = true ]]; then
      export PKGLY_BINARY_PATH="./target/aarch64-unknown-linux-gnu/release/pkgly"
      cargo zigbuild --release --target aarch64-unknown-linux-gnu --features frontend
    else
      export PKGLY_BINARY_PATH="./target/aarch64-unknown-linux-gnu/debug/pkgly"
      cargo zigbuild --target aarch64-unknown-linux-gnu --features frontend
    fi
    ;;
esac

docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d --force-recreate pkgly
