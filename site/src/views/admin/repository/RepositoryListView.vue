<!-- ABOUTME: Manages repositories and reports cached storage usage for administrators. -->
<!-- ABOUTME: Prioritizes human-readable repository metadata over implementation identifiers. -->
<template>
  <v-container class="py-6 admin-repository-page">
    <div class="page-header">
      <div class="page-header__titles">
        <h1 class="text-h5 mb-1">Repositories</h1>
        <p class="text-body-2 text-medium-emphasis">
          Monitor repository cache usage and manage repositories within this instance.
        </p>
      </div>
      <div class="page-header__actions" aria-label="Repository actions">
        <div class="page-header__cache">
          <v-icon size="small" color="primary">mdi-database-clock-outline</v-icon>
          <div>
            <div class="text-body-2 font-weight-medium">Storage usage</div>
            <div class="text-caption text-medium-emphasis">{{ usageStatusText }}</div>
          </div>
          <v-btn
            color="primary"
            variant="tonal"
            :loading="refreshing"
            :disabled="loading"
            icon="mdi-refresh"
            aria-label="Refresh storage usage"
            title="Refresh storage usage"
            class="page-header__refresh"
            @click="refreshUsage" />
        </div>
        <v-btn
          v-if="repositories.length >= 1 && hasStorages"
          color="primary"
          prepend-icon="mdi-plus"
          :to="{ name: 'RepositoryCreate' }"
          variant="flat">
          Create Repository
        </v-btn>
      </div>
    </div>

    <v-alert
      v-if="error"
      type="error"
      variant="tonal"
      class="mb-6"
      prominent>
      {{ error }}
    </v-alert>

    <v-row v-else class="gy-6">
      <v-col cols="12">
        <v-card v-if="loading" class="text-center py-8" variant="flat">
          <v-progress-circular indeterminate color="primary" size="48" />
          <div class="mt-4 text-medium-emphasis">Loading repositories…</div>
        </v-card>

        <v-card v-else-if="repositories.length >= 1" class="elevation-0">
          <v-data-table
            :headers="headers"
            :items="tableItems"
            :loading="refreshing"
            item-value="id"
            @click:row="handleRowClick"
            class="elevation-0 repository-table">
            <template #[`item.repository`]="{ item }">
              <div class="repository-cell">
                <span class="font-weight-medium">{{ item.name }}</span>
                <span class="text-caption text-medium-emphasis">
                  {{ item.repository_type.toUpperCase() }} · {{ item.repository_kind }}
                </span>
              </div>
            </template>

            <template #[`item.access`]="{ item }">
              <div class="access-cell">
                <v-chip size="x-small" variant="outlined">
                  {{ item.auth_label }}
                </v-chip>
                <v-chip
                  size="x-small"
                  :color="item.active ? 'success' : 'default'"
                  :variant="item.active ? 'tonal' : 'outlined'">
                  {{ item.active_label }}
                </v-chip>
              </div>
            </template>

            <template #[`item.usage`]="{ value }">
              <span class="text-no-wrap">{{ formatBytes(value) }}</span>
            </template>

            <template #[`item.storage_usage_updated_at`]="{ value }">
              <time
                v-if="isValidTimestamp(value)"
                :datetime="value"
                :title="formatExactTimestamp(value)"
                class="text-caption text-medium-emphasis text-no-wrap">
                {{ formatUpdatedAt(value) }}
              </time>
              <span v-else class="text-caption text-medium-emphasis">
                Not available
              </span>
            </template>

            <template v-slot:no-data>
              <div class="pa-4 text-center text-medium-emphasis">
                No repositories found.
              </div>
            </template>
          </v-data-table>
        </v-card>

        <v-card
          v-else
          class="text-center py-8"
          variant="outlined">
          <v-icon color="medium-emphasis" size="48" class="mb-2">mdi-package-variant</v-icon>
          <div class="text-h6 text-medium-emphasis mb-2">{{ emptyStateTitle }}</div>
          <div class="text-body-2 text-medium-emphasis mb-4">
            {{ emptyStateMessage }}
          </div>
          <v-btn
            color="primary"
            prepend-icon="mdi-plus"
            :to="{ name: emptyStateActionRoute }"
            variant="flat">
            {{ emptyStateActionLabel }}
          </v-btn>
        </v-card>
      </v-col>
    </v-row>
  </v-container>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import http from "@/http";
import { computed, ref } from "vue";
import type { DataTableHeader } from "vuetify";
import type { RepositoryWithStorageName } from "@/types/repository";
import type { StorageItem } from "@/components/nr/storage/storageTypes";
import { useRepositoryStore } from "@/stores/repositories";

const router = useRouter();
const repositoryStore = useRepositoryStore();
const repositories = ref<RepositoryWithStorageName[]>([]);
const storages = ref<StorageItem[]>([]);
const loading = ref(true);
const refreshing = ref(false);
const error = ref<string | null>(null);

// Define table headers
const headers: DataTableHeader[] = [
  {
    title: 'Repository',
    key: 'repository',
    value: 'name',
    sortable: true,
  },
  {
    title: 'Storage',
    key: 'storage_name',
    value: 'storage_name',
    sortable: true,
  },
  {
    title: 'Access',
    key: 'access',
    value: 'auth_label',
    sortable: false,
  },
  {
    title: 'Usage',
    key: 'usage',
    value: 'storage_usage_bytes',
    sortable: true,
    align: 'end' as const,
  },
  {
    title: 'Usage Updated',
    key: 'storage_usage_updated_at',
    value: 'storage_usage_updated_at',
    sortable: true,
  },
];

// Convert repositories to v-data-table format
const tableItems = computed(() => {
  return repositories.value.map((repo) => {
    const kind = (repo.repository_kind ?? "hosted").toLowerCase();
    const repository_kind = kind === "proxy" ? "Proxy" : kind === "virtual" ? "Virtual" : "Hosted";
    return {
      id: repo.id,
      name: repo.name,
      storage_name: repo.storage_name,
      repository_type: repo.repository_type,
      repository_kind,
      auth_enabled: repo.auth_enabled,
      auth_label: repo.auth_enabled ? "Secured" : "Unsecured",
      storage_usage_bytes: repo.storage_usage_bytes,
      active: repo.active,
      active_label: repo.active === false ? "Inactive" : "Active",
      storage_usage_updated_at: repo.storage_usage_updated_at,
    };
  });
});

const hasStorages = computed(() => storages.value.length > 0);
const emptyStateTitle = computed(() => (hasStorages.value ? "No repositories found" : "No storages found"));
const emptyStateMessage = computed(() =>
  hasStorages.value
    ? "Create your first repository to get started."
    : "Create a storage before adding repositories.",
);
const emptyStateActionRoute = computed(() => (hasStorages.value ? "RepositoryCreate" : "StorageCreate"));
const emptyStateActionLabel = computed(() => (hasStorages.value ? "Create Repository" : "Create Storage"));

async function fetchInitialData() {
  loading.value = true;
  error.value = null;
  try {
    const [repositoryResponse, availableStorages] = await Promise.all([
      http.get<RepositoryWithStorageName[]>("/api/repository/list", {
        params: { include_usage: true },
      }),
      repositoryStore.getStorages(),
    ]);
    repositories.value = repositoryResponse.data;
    storages.value = availableStorages;
  } catch (err) {
    console.error(err);
    error.value = "Failed to fetch repositories";
  } finally {
    loading.value = false;
  }
}

async function fetchRepositories(options: { refresh?: boolean } = {}) {
  if (options.refresh) {
    refreshing.value = true;
  } else {
    loading.value = true;
  }
  error.value = null;
  try {
    const params: Record<string, boolean> = { include_usage: true };
    if (options.refresh) {
      params.refresh_usage = true;
    }
    const response = await http.get<RepositoryWithStorageName[]>("/api/repository/list", {
      params,
    });
    repositories.value = response.data;
  } catch (err) {
    console.error(err);
    error.value = "Failed to fetch repositories";
  } finally {
    if (options.refresh) {
      refreshing.value = false;
    } else {
      loading.value = false;
    }
  }
}

function refreshUsage() {
  void fetchRepositories({ refresh: true });
}

// Handle row click navigation
type DataTableRow = { item: { id?: string | number } | { raw?: { id?: string | number } } };

function handleRowClick(_event: MouseEvent, row: DataTableRow) {
  const candidate = (row.item as { raw?: { id?: string | number }; id?: string | number }) ?? {};
  const id = candidate.raw?.id ?? candidate.id;
  if (!id) {
    return;
  }
  router.push({
    name: 'AdminViewRepository',
    params: { id },
  });
}

// Utility functions
function formatBytes(bytes?: number | null): string {
  if (bytes === null || bytes === undefined) {
    return "Not available";
  }
  if (bytes === 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exponent);
  return `${value.toFixed(exponent === 0 ? 0 : 2)} ${units[exponent]}`;
}

function formatUpdatedAt(timestamp?: string | null): string {
  if (!isValidTimestamp(timestamp)) {
    return "Not available";
  }
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(timestamp));
}

function isValidTimestamp(timestamp?: string | null): timestamp is string {
  return Boolean(timestamp) && !Number.isNaN(new Date(timestamp as string).getTime());
}

function formatExactTimestamp(timestamp: string): string {
  return new Date(timestamp).toLocaleString();
}

const latestUsageUpdate = computed(() => {
  const timestamps = repositories.value
    .map((repo) => repo.storage_usage_updated_at)
    .filter((value): value is string => Boolean(value));
  if (timestamps.length === 0) {
    return null;
  }
  const latest = timestamps.reduce((max, current) => (current > max ? current : max));
  return latest;
});

const usageStatusText = computed(() => {
  if (loading.value) {
    return "Loading repository information…";
  }
  if (refreshing.value) {
    return "Refreshing usage totals…";
  }
  if (repositories.value.length === 0) {
    return "No repositories yet.";
  }
  if (!latestUsageUpdate.value) {
    return "Not calculated yet.";
  }
  const date = new Date(latestUsageUpdate.value);
  if (Number.isNaN(date.getTime())) {
    return "Not calculated yet.";
  }
  return `Last updated ${date.toLocaleString()}`;
});

void fetchInitialData();
</script>

<style scoped lang="scss">
// Ensure v-data-table respects theme colors and add cursor pointer for rows
:deep(.v-data-table) {
  .v-data-table__th {
    color: var(--nr-text-primary);
    background-color: var(--nr-table-header-background);
  }

  .v-data-table__td {
    color: var(--nr-text-primary);
  }

  .v-data-table__tr {
    cursor: pointer;

    &:hover {
      background-color: var(--nr-table-row-hover);
    }
  }
}

// Repository table specific styling
.repository-table {
  tbody tr:hover {
    cursor: pointer;
  }
}

.admin-repository-page {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}

.page-header__titles {
  max-width: 640px;
  flex: 1 1 auto;
}

.page-header__actions {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.page-header__cache {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  background: var(--nr-surface);
  border: 1px solid var(--nr-border-color);
  border-radius: var(--nr-radius-md);
  padding: var(--nr-spacing-sm) var(--nr-spacing-md);
}

.page-header__cache > div:first-child {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.page-header__refresh {
  flex: 0 0 auto;
}

.repository-cell {
  display: flex;
  min-width: 150px;
  flex-direction: column;
  gap: 2px;
}

.access-cell {
  display: flex;
  min-width: 150px;
  gap: var(--nr-spacing-xs);
}

@media (max-width: 900px) {
  .page-header__actions {
    flex-direction: column;
    align-items: stretch;
  }

  .page-header__cache {
    justify-content: space-between;
  }

}
</style>
