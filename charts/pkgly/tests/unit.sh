#!/usr/bin/env bash
# ABOUTME: Runs static and rendered unit checks for the Pkgly Helm chart.
# ABOUTME: Verifies optional PostgreSQL behavior and external OpenTelemetry configuration.
set -euo pipefail

chart_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
rendered_default="$(mktemp)"
rendered_external="$(mktemp)"
rendered_external_secret="$(mktemp)"
trap 'rm -f "$rendered_default" "$rendered_external" "$rendered_external_secret"' EXIT

helm lint "$chart_dir"
helm template pkgly "$chart_dir" --namespace default >"$rendered_default"
helm template pkgly "$chart_dir" \
  --namespace default \
  --set postgresql.enabled=false \
  --set externalDatabase.host=postgres.example.internal \
  --set externalDatabase.user=pkgly \
  --set externalDatabase.password=secret \
  --set externalDatabase.database=pkgly \
  --set opentelemetry.enabled=true \
  --set opentelemetry.endpoint=http://otel-collector.observability.svc:4317 \
  >"$rendered_external"
helm template pkgly "$chart_dir" \
  --namespace default \
  --set postgresql.enabled=false \
  --set externalDatabase.host=postgres.example.internal \
  --set externalDatabase.user=pkgly \
  --set externalDatabase.database=pkgly \
  --set externalDatabase.secretRef.enabled=true \
  --set externalDatabase.secretRef.name=database-secret \
  >"$rendered_external_secret"

if grep -Eq '(^|[[:space:]])jaeger([:.]|$)' "$chart_dir/Chart.yaml" "$chart_dir/Chart.lock"; then
  echo "Jaeger must not be a chart dependency" >&2
  exit 1
fi

if grep -q 'name: pkgly-postgresql' "$rendered_external"; then
  echo "PostgreSQL resources rendered while postgresql.enabled=false" >&2
  exit 1
fi

grep -q 'host = "postgres.example.internal"' "$rendered_external"
grep -q 'endpoint = "http://otel-collector.observability.svc:4317"' "$rendered_external"
grep -q 'name: PKGLY_DATABASE_PASSWORD' "$rendered_external_secret"
grep -q 'name: PKGLY_DATABASE_HOST' "$rendered_external_secret"
if grep -q '\${DB_PASSWORD}' "$rendered_external_secret"; then
  echo "Pkgly does not interpolate environment variables inside TOML values" >&2
  exit 1
fi
grep -q 'kind: StatefulSet' "$rendered_default"
grep -q 'name: pkgly-postgresql' "$rendered_default"
grep -q 'claimName: pkgly-data' "$rendered_default"
grep -q 'name: PKGLY_DATABASE_PASSWORD' "$rendered_default"
grep -A1 '^  strategy:' "$rendered_default" | grep -q 'type: Recreate'
grep -A4 'livenessProbe:' "$rendered_default" | grep -q 'path: /health'
grep -A4 'livenessProbe:' "$rendered_default" | grep -q 'port: http'
grep -A5 'readinessProbe:' "$rendered_default" | grep -q 'tcpSocket:'

echo "Helm chart unit tests passed"
