<template>
  <section class="package-results">
    <header class="package-results__header">
      <h3>Package Matches</h3>
      <span v-if="!loading">{{ results.length }} result(s)</span>
    </header>
    <div
      v-if="loading"
      class="package-results__state">
      Searching packages...
    </div>
    <div
      v-else-if="error"
      class="package-results__state package-results__state--error">
      {{ error }}
    </div>
    <div
      v-else-if="results.length === 0"
      class="package-results__state">
      No packages found.
    </div>
    <table
      v-else
      class="package-results__table">
      <thead>
        <tr>
          <th>Package</th>
          <th>Type</th>
          <th>Repository</th>
          <th>Size</th>
          <th>Path</th>
          <th>Uploaded At</th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="pkg in results"
          :key="pkg.cachePath">
          <td>
            <span
              class="package-results__wrap"
              data-testid="package-result-name"
              :title="pkg.fileName"
              @click="selectCell">
              {{ pkg.fileName }}
            </span>
          </td>
          <td>
            <v-chip
              size="x-small"
              variant="tonal"
              color="primary"
              class="text-uppercase font-weight-medium">
              {{ pkg.repositoryType || "unknown" }}
            </v-chip>
          </td>
          <td>
            <span
              class="package-results__wrap"
              :title="`${pkg.repositoryName} (${pkg.storageName})`"
              @click="selectCell">
              {{ pkg.repositoryName }} ({{ pkg.storageName }})
            </span>
          </td>
          <td>
            <span class="package-results__wrap" @click="selectCell">{{ formatBytes(pkg.size) }}</span>
          </td>
          <td>
            <code
              class="package-results__wrap package-results__path"
              data-testid="package-result-path"
              :title="pkg.cachePath"
              @click="selectCell">
              {{ pkg.cachePath }}
            </code>
          </td>
          <td>
            <span
              class="package-results__wrap"
              :title="new Date(pkg.modified).toLocaleString()"
              @click="selectCell">
              {{ new Date(pkg.modified).toLocaleString() }}
            </span>
          </td>
        </tr>
      </tbody>
    </table>
  </section>
</template>

<script setup lang="ts">
import { formatBytes } from "@/utils/repositorySearch";

export interface PackageResult {
  repositoryId: string;
  repositoryName: string;
  storageName: string;
  repositoryType: string;
  fileName: string;
  cachePath: string;
  size: number;
  modified: string;
}

defineProps<{
  results: PackageResult[];
  loading: boolean;
  error: string | null;
}>();

defineEmits<{
  open: [pkg: PackageResult];
}>();

function selectCell(event: MouseEvent) {
  event.stopPropagation();
  const target = event.currentTarget as HTMLElement | null;
  if (!target) {
    return;
  }
  const selection = window.getSelection?.();
  if (!selection) {
    return;
  }
  const range = document.createRange();
  range.selectNodeContents(target);
  selection.removeAllRanges();
  selection.addRange(range);
}
</script>

<style scoped lang="scss">
.package-results {
  background: var(--nr-background-primary, #fff);
  border-radius: 0.75rem;
  border: 1px solid rgba(15, 23, 42, 0.08);
  padding: 1rem 1.25rem;
  margin-bottom: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.package-results__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.5rem;
}

.package-results__state {
  padding: 0.75rem;
  border-radius: 0.5rem;
  background: rgba(15, 23, 42, 0.04);
  color: var(--nr-text-primary, #0f172a);
}

.package-results__state--error {
  background: rgba(220, 38, 38, 0.08);
  color: var(--nr-error, #e53935);
}

.package-results__table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.95rem;
}

.package-results__table th,
.package-results__table td {
  padding: 0.6rem 0.5rem;
  border-bottom: 1px solid rgba(15, 23, 42, 0.12);
  text-align: left;
  vertical-align: top;
  max-width: 0;
}

.package-results__table tbody tr {
  cursor: default;
  transition: background-color 0.2s ease;
}

.package-results__table tbody tr:hover,
.package-results__table tbody tr:focus-visible {
  background: rgba(37, 99, 235, 0.08);
}

code {
  background: rgba(15, 23, 42, 0.06);
  padding: 0.2rem 0.35rem;
  border-radius: 0.35rem;
}

.package-results__wrap {
  display: inline-block;
  max-width: 100%;
  overflow-wrap: anywhere;
  word-break: break-word;
  white-space: normal;
}

.package-results__path {
  display: block;
  font-family: "Fira Code", "SFMono-Regular", Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
}

@media (max-width: 768px) {
  .package-results {
    padding: 0.75rem;
  }

  .package-results__table {
    font-size: 0.85rem;
  }

  .package-results__table th,
  .package-results__table td {
    padding: 0.5rem 0.35rem;
  }
}
</style>
