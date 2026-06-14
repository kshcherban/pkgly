#!/usr/bin/env bash
# ABOUTME: Verifies Pkgly and PostgreSQL data survive pod restarts and Helm redeploys.
# ABOUTME: Exercises real PVC-backed storage in local kind default namespace.
set -euo pipefail

chart_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release="${PKGLY_HELM_RELEASE:-pkgly-chart-test}"
timeout="${PKGLY_TEST_TIMEOUT:-5m}"
marker="pkgly-persistence-$(date +%s)"
app_selector="app.kubernetes.io/instance=$release,app.kubernetes.io/name=pkgly"
postgres_selector="app.kubernetes.io/instance=$release,app.kubernetes.io/name=postgresql"

app_pod() {
  kubectl get pod --namespace default --selector "$app_selector" \
    -o jsonpath='{.items[0].metadata.name}'
}

postgres_pod() {
  kubectl get pod --namespace default --selector "$postgres_selector" \
    -o jsonpath='{.items[0].metadata.name}'
}

wait_for_workloads() {
  kubectl rollout status "deployment/$release" --namespace default --timeout="$timeout"
  kubectl rollout status "statefulset/$release-postgresql" --namespace default --timeout="$timeout"
}

kubectl exec --namespace default "$(app_pod)" -- \
  sh -c "printf '%s' '$marker' > /data/helm-persistence-marker"
kubectl exec --namespace default "$(postgres_pod)" -- \
  env PGPASSWORD=pkgly psql -U pkgly -d pkgly -v ON_ERROR_STOP=1 \
  -c "CREATE TABLE IF NOT EXISTS helm_persistence_test (value text PRIMARY KEY);" \
  -c "INSERT INTO helm_persistence_test (value) VALUES ('$marker');"

kubectl delete pod --namespace default "$(app_pod)" "$(postgres_pod)" --wait=false
wait_for_workloads

test "$(kubectl exec --namespace default "$(app_pod)" -- cat /data/helm-persistence-marker)" = "$marker"
kubectl exec --namespace default "$(postgres_pod)" -- \
  env PGPASSWORD=pkgly psql -U pkgly -d pkgly -tAc \
  "SELECT value FROM helm_persistence_test WHERE value = '$marker';" |
  grep -qx "$marker"

helm upgrade "$release" "$chart_dir" --namespace default --reuse-values --wait --timeout "$timeout"
wait_for_workloads

test "$(kubectl exec --namespace default "$(app_pod)" -- cat /data/helm-persistence-marker)" = "$marker"
kubectl exec --namespace default "$(postgres_pod)" -- \
  env PGPASSWORD=pkgly psql -U pkgly -d pkgly -tAc \
  "SELECT value FROM helm_persistence_test WHERE value = '$marker';" |
  grep -qx "$marker"

echo "Helm chart end-to-end persistence tests passed"
