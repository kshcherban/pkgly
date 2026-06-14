#!/usr/bin/env bash
# ABOUTME: Installs Pkgly into local kind and verifies Kubernetes workloads become ready.
# ABOUTME: Uses default namespace and chart-bundled PostgreSQL dependency.
set -euo pipefail

chart_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release="${PKGLY_HELM_RELEASE:-pkgly-chart-test}"
external_release="${release}-external"
external_database_secret="${external_release}-database"
image_repository="${PKGLY_TEST_IMAGE_REPOSITORY:-pkgly}"
image_tag="${PKGLY_TEST_IMAGE_TAG:-test}"
timeout="${PKGLY_TEST_TIMEOUT:-5m}"
trap 'helm uninstall "$external_release" --namespace default --wait >/dev/null 2>&1 || true; kubectl delete secret "$external_database_secret" --namespace default --ignore-not-found >/dev/null 2>&1' EXIT

helm upgrade --install "$release" "$chart_dir" \
  --namespace default \
  --set image.repository="$image_repository" \
  --set image.tag="$image_tag" \
  --set image.pullPolicy=IfNotPresent \
  --wait \
  --timeout "$timeout"

kubectl rollout status "deployment/$release" --namespace default --timeout="$timeout"
kubectl rollout status "statefulset/$release-postgresql" --namespace default --timeout="$timeout"
kubectl get pod --namespace default \
  --selector "app.kubernetes.io/instance=$release" \
  -o jsonpath='{range .items[*]}{.metadata.name}{" "}{.status.phase}{"\n"}{end}' |
  awk '$2 != "Running" { exit 1 }'

kubectl delete secret "$external_database_secret" --namespace default --ignore-not-found
kubectl create secret generic "$external_database_secret" \
  --namespace default \
  --from-literal=password=pkgly

helm upgrade --install "$external_release" "$chart_dir" \
  --namespace default \
  --set image.repository="$image_repository" \
  --set image.tag="$image_tag" \
  --set image.pullPolicy=IfNotPresent \
  --set persistence.enabled=false \
  --set postgresql.enabled=false \
  --set externalDatabase.host="$release-postgresql" \
  --set externalDatabase.user=pkgly \
  --set externalDatabase.database=pkgly \
  --set externalDatabase.secretRef.enabled=true \
  --set externalDatabase.secretRef.name="$external_database_secret" \
  --wait \
  --timeout "$timeout"

kubectl rollout status "deployment/$external_release" --namespace default --timeout="$timeout"
test "$(kubectl get statefulset --namespace default \
  --selector "app.kubernetes.io/instance=$external_release" \
  --no-headers 2>/dev/null | wc -l | tr -d ' ')" = "0"

echo "Helm chart integration tests passed"
