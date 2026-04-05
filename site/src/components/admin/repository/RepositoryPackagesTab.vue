<template>
  <section class="packages">
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <span class="text-h6">{{ headerTitle }}</span>
        <v-spacer />
        <v-text-field
          v-if="totalPackages > 0 || normalizedSearchTerm.length > 0"
          v-model="searchTerm"
          :placeholder="`Search ${headerTitle.toLowerCase()}…`"
          prepend-inner-icon="mdi-magnify"
          variant="outlined"
          density="compact"
          clearable
          @click:clear="clearSearch"
          hide-details
          style="max-width: 300px;"
          aria-label="Search packages" />
      </v-card-title>

      <v-card-subtitle v-if="!isLoading" class="pa-4 pt-0">
        <div class="d-flex align-center justify-space-between flex-wrap">
          <div class="text-body-2 text-medium-emphasis">
            {{ totalPackages }} package(s)
            <span v-if="visiblePackages.length"> · Showing {{ visiblePackages.length }} file(s)</span>
          </div>
          <div class="d-flex align-center gap-3">
            <v-btn
              color="primary"
              variant="text"
              prepend-icon="mdi-refresh"
              :disabled="isLoading || isDeleting"
              @click="refreshPackages">
              Refresh
            </v-btn>
            <span v-if="selectedCount > 0" class="text-body-2">{{ selectedCount }} selected</span>
            <v-btn
              color="error"
              variant="flat"
              class="danger-hover ml-2"
              prepend-icon="mdi-delete"
              :disabled="selectedCount === 0 || isDeleting"
              :loading="isDeleting"
              @click="deleteSelected">
              Delete Selected
            </v-btn>
          </div>
        </div>
      </v-card-subtitle>

      <v-card-text
        v-if="!isLoading && pendingDeletionCount > 0"
        class="pt-0 px-4">
        <v-alert
          type="info"
          variant="tonal"
          border="start"
          class="packages__deletion-alert">
          <div class="text-body-2">
            {{ pendingDeletionCount }} package{{ pendingDeletionCount === 1 ? "" : "s" }} queued for deletion. Changes may take a moment to complete. Use Refresh to check for updates.
          </div>
        </v-alert>
      </v-card-text>
      <v-card-text
        v-if="!isLoading && visibleIndexingWarning"
        class="pt-0 px-4">
        <v-alert
          data-testid="packages-indexing-warning"
          type="info"
          variant="tonal"
          border="start"
          class="packages__indexing-alert">
          <div class="text-body-2">
            {{ visibleIndexingWarning }}
          </div>
        </v-alert>
      </v-card-text>

      <v-data-table-server
        v-if="!isLoading && !error && totalPackages > 0 && visiblePackages.length > 0"
        :headers="headers"
        :items="tableItems"
        :loading="isDeleting"
        item-value="cachePath"
        v-model="selected"
        show-select
        class="elevation-0"
        :page="currentPage"
        :items-per-page="perPage"
        :items-length="totalPackages"
        :items-per-page-options="perPageOptions"
        hide-default-footer
        @update:page="handlePageChange"
        @update:items-per-page="handleItemsPerPageChange">

        <template v-slot:item.size="{ value }">
          <div class="text-end">{{ formatBytes(value) }}</div>
        </template>

        <template v-slot:item.cachePath="{ value }">
          <v-code class="text-caption">{{ value }}</v-code>
        </template>

        <template v-slot:item.blobDigest="{ value }">
          <v-code class="text-caption">{{ value }}</v-code>
        </template>

        <template v-slot:item.modified="{ value }">
          <div class="text-no-wrap">
            {{ new Date(value).toLocaleString() }}
          </div>
        </template>

        <template v-slot:no-data>
          <div class="pa-4 text-center text-medium-emphasis">
            No packages match your search. Try different search terms.
          </div>
        </template>
      </v-data-table-server>

      <v-card-text v-else-if="isLoading" class="text-center py-8">
        <v-progress-circular indeterminate color="primary" size="48" />
        <div class="mt-4 text-medium-emphasis">Loading packages...</div>
      </v-card-text>

      <v-card-text v-else-if="error" class="text-center py-8">
        <v-icon color="error" size="48" class="mb-2">mdi-alert-circle</v-icon>
        <div class="text-error">Failed to load packages: {{ error }}</div>
      </v-card-text>

      <v-card-text
        v-else-if="totalPackages === 0 && normalizedSearchTerm.length === 0"
        class="text-center py-8">
        <v-icon color="medium-emphasis" size="48" class="mb-2">mdi-package-variant</v-icon>
        <div class="text-medium-emphasis">{{ emptyRepositoryMessage }}</div>
      </v-card-text>

      <v-card-text
        v-else-if="totalPackages === 0 && normalizedSearchTerm.length > 0"
        class="text-center py-8">
        <v-icon color="medium-emphasis" size="48" class="mb-2">mdi-magnify</v-icon>
        <div class="text-medium-emphasis">No packages match your search. Try different search terms.</div>
      </v-card-text>

      <v-card-text v-else-if="visiblePackages.length === 0" class="text-center py-8">
        <v-icon color="medium-emphasis" size="48" class="mb-2">mdi-magnify</v-icon>
        <div class="text-medium-emphasis">No packages match your search on this page. Try a different page or clear the filters.</div>
      </v-card-text>

      <v-card-actions v-if="totalPackages > 0" class="pa-4 packages__footer">
        <div class="d-flex align-center gap-3">
          <span class="text-body-2 text-medium-emphasis">Items per page:</span>
          <v-select
            v-model="perPage"
            :items="perPageOptions"
            density="compact"
            variant="outlined"
            hide-details
            style="max-width: 120px"
            @update:model-value="handleItemsPerPageChange" />
        </div>

        <div class="text-body-2 text-medium-emphasis packages__range" aria-live="polite">
          {{ itemRangeLabel }}
        </div>

        <v-pagination
          v-model="currentPage"
          :length="totalPages"
          :disabled="isDeleting"
          :total-visible="7"
          density="comfortable" />
      </v-card-actions>
    </v-card>
  </section>
</template>

<script setup lang="ts">
import http from "@/http";
import { computed, onMounted, ref, watch, nextTick } from "vue";
import { useAlertsStore } from "@/stores/alerts";
import { useResizableColumns } from "@/composables/useResizableColumns";
import { shouldDisplayRepositoryIndexingWarning } from "@/types/repository";

interface PackageEntry {
  name: string;
  size: number;
  cachePath: string;
  blobDigest?: string | null;
  modified: string;
  package: string;
}

const props = defineProps<{
  repositoryId: string;
  repositoryType?: string;
  repositoryKind?: string | null;
}>();

const packages = ref<PackageEntry[]>([]);
const isLoading = ref(false);
const error = ref<string | null>(null);
const currentPage = ref(1);
const perPage = ref(50);
const totalPackages = ref(0);
const perPageOptions = [25, 50, 100, 200, 500, 1000];
const selected = ref<string[]>([]);
const isDeleting = ref(false);
const searchTerm = ref("");
const pendingDeletionPaths = ref<string[]>([]);
const pendingDeletionCount = ref(0);
const indexingWarning = ref<string | null>(null);
const alerts = useAlertsStore();
const resizers = useResizableColumns('.v-data-table');

const normalizedSearchTerm = computed(() => searchTerm.value.trim());

function clearSearch() {
  searchTerm.value = "";
}

onMounted(() => {
  loadPackages();
  // Enable resizable columns for v-data-table
  resizers.initResizable();
});
watch(
  () => props.repositoryId,
  () => {
    packages.value = [];
    error.value = null;
    currentPage.value = 1;
    selected.value = [];
    searchTerm.value = "";
    pendingDeletionPaths.value = [];
    pendingDeletionCount.value = 0;
    indexingWarning.value = null;
    loadPackages();
  },
);

watch([currentPage, perPage], () => {
  if (!props.repositoryId) {
    return;
  }
  selected.value = [];
  loadPackages();
});

watch(normalizedSearchTerm, () => {
  if (!props.repositoryId) {
    return;
  }
  selected.value = [];
  if (currentPage.value !== 1) {
    currentPage.value = 1;
    return;
  }
  loadPackages();
});

watch(packages, async () => {
  await nextTick();
  resizers.initResizable();
});

// Define table headers based on repository type
const headers = computed(() => {
  const base = [
    {
      title: packageColumnTitle.value,
      key: "package",
      sortable: true,
    },
    {
      title: nameColumnTitle.value,
      key: "name",
      sortable: true,
    },
    {
      title: "Blob Digest",
      key: "blobDigest",
      sortable: true,
    },
    {
      title: "Size",
      key: "size",
      sortable: true,
      align: "end" as const,
    },
    {
      title: pathColumnTitle.value,
      key: "cachePath",
      sortable: true,
    },
    {
      title: timestampColumnTitle.value,
      key: "modified",
      sortable: true,
    },
  ];
  return base;
});

// Convert packages to v-data-table format
const tableItems = computed(() => {
  return packages.value.map((pkg) => ({
    package: pkg.package,
    name: pkg.name,
    blobDigest: pkg.blobDigest ?? null,
    size: pkg.size,
    cachePath: pkg.cachePath,
    modified: pkg.modified,
  }));
});

const totalPages = computed(() => {
  if (totalPackages.value === 0) {
    return 1;
  }
  return Math.max(1, Math.ceil(totalPackages.value / perPage.value));
});

const itemRangeLabel = computed(() => {
  if (totalPackages.value === 0 || packages.value.length === 0) {
    return "0 of 0";
  }
  const start = (currentPage.value - 1) * perPage.value + 1;
  const end = Math.min(start + packages.value.length - 1, totalPackages.value);
  return `${start}-${end} of ${totalPackages.value}`;
});

const visiblePackages = computed(() => packages.value);

const selectedCount = computed(() => selected.value.length);

const derivedHostedFromPackages = computed(() => {
  if (packages.value.length === 0) {
    return false;
  }
  return !packages.value.some((pkg) => pkg.cachePath.startsWith("packages/"));
});

const repositoryType = computed(() => props.repositoryType?.toLowerCase() ?? "");
const showIndexingWarning = computed(() =>
  shouldDisplayRepositoryIndexingWarning(props.repositoryType, props.repositoryKind),
);
const visibleIndexingWarning = computed(() =>
  showIndexingWarning.value ? indexingWarning.value : null,
);
const isDockerRepository = computed(() => repositoryType.value === "docker");
const isDebRepository = computed(() => repositoryType.value === "deb");
const isDockerProxy = computed(
  () => isDockerRepository.value && props.repositoryKind?.toLowerCase() === "proxy",
);
const isPhpRepository = computed(() => repositoryType.value === "php");
const isPhpProxy = computed(
  () => isPhpRepository.value && props.repositoryKind?.toLowerCase() === "proxy",
);

const isHostedRepository = computed(() => {
  if (props.repositoryKind) {
    return props.repositoryKind.toLowerCase() === "hosted";
  }
  if (isDockerRepository.value || isDebRepository.value) {
    return true;
  }
  if (props.repositoryType === "python") {
    return derivedHostedFromPackages.value;
  }
  return false;
});

const headerTitle = computed(() => {
  if (isDockerRepository.value) {
    return "Images";
  }
  return "Packages";
});
const packageColumnTitle = computed(() => (isDockerRepository.value ? "Repository" : "Package"));
const nameColumnTitle = computed(() => {
  if (isDockerRepository.value) {
    return "Tag";
  }
  if (isDebRepository.value || isPhpProxy.value) {
    return "Version";
  }
  return "Name";
});
const pathColumnTitle = computed(() => {
  if (isDockerRepository.value) {
    return "Manifest Path";
  }
  return isHostedRepository.value ? "Path" : "Cached Path";
});
const timestampColumnTitle = computed(() =>
  "Uploaded At",
);

const emptyRepositoryMessage = computed(() => {
  if (isDockerRepository.value) {
    if (isDockerProxy.value) {
      return "No images cached yet. Pull an image through this proxy to populate the list.";
    }
    return "No images yet. Push an image to populate this list.";
  }
  return isHostedRepository.value
    ? "No packages yet. Upload a package to populate this list."
    : "No packages yet. Trigger a download to populate this list.";
});

async function loadPackages() {
  if (!props.repositoryId) {
    return;
  }
  isLoading.value = true;
  error.value = null;
  try {
    const params: Record<string, any> = {
      page: currentPage.value,
      per_page: perPage.value,
      sort_by: "modified",
      sort_dir: "desc",
    };
    const term = normalizedSearchTerm.value;
    if (term) {
      params.q = term;
    }
    const response = await http.get(`/api/repository/${props.repositoryId}/packages`, {
      params,
    });
    const data = response.data ?? {};
    const items: PackageEntry[] = (data.items ?? []).map((item: any) => ({
      name: item.name,
      size: item.size,
      cachePath: item.cache_path,
      blobDigest: item.blob_digest ?? null,
      modified: item.modified,
      package: item.package,
    }));
    packages.value = items;
    totalPackages.value = data.total_packages ?? 0;
    const warning = response.headers?.["x-pkgly-warning"] ?? response.headers?.["X-Pkgly-Warning"];
    const normalizedWarning =
      typeof warning === "string" && warning.trim().length > 0 ? warning.trim() : null;
    indexingWarning.value =
      showIndexingWarning.value ? normalizedWarning : null;
    selected.value = [];

    if (pendingDeletionPaths.value.length > 0) {
      const remaining = pendingDeletionPaths.value.filter((path) =>
        items.some((pkg) => pkg.cachePath === path),
      );
      pendingDeletionPaths.value = remaining;
      pendingDeletionCount.value = remaining.length;
    } else {
      pendingDeletionCount.value = 0;
    }
  } catch (err) {
    console.error(err);
    error.value = err instanceof Error ? err.message : String(err);
    indexingWarning.value = null;
  } finally {
    isLoading.value = false;
    await nextTick();
    resizers.initResizable();
  }
}

async function deleteSelected() {
  if (!props.repositoryId || selected.value.length === 0) {
    return;
  }
  const paths = [...selected.value];
  const count = selected.value.length;
  const confirmationMessage = isDockerRepository.value
    ? `Delete ${count} image tag(s)? This removes their manifests from the registry.`
    : isHostedRepository.value
      ? `Delete ${count} package(s)? This removes files from the repository.`
      : `Delete ${count} cached package(s)? This removes cached files but not upstream artifacts.`;
  const confirmed = window.confirm(confirmationMessage);
  if (!confirmed) {
    return;
  }
  isDeleting.value = true;
  try {
    await http.delete(`/api/repository/${props.repositoryId}/packages`, {
      data: { paths: selected.value },
    });
    const successTitle = isDockerRepository.value ? "Images deleted" : "Packages deleted";
    const successText = isDockerRepository.value
      ? `${count} manifest(s) removed`
      : isHostedRepository.value
        ? `${count} package(s) removed`
        : `${count} cached package(s) removed`;
    alerts.success(successTitle, successText);
    pendingDeletionPaths.value = paths;
    pendingDeletionCount.value = paths.length;
    selected.value = [];
    await loadPackages();
  } catch (err: any) {
    console.error(err);
    const message = err?.response?.data?.message ?? err?.message ?? "Failed to delete packages";
    alerts.error("Deletion failed", message);
  } finally {
    isDeleting.value = false;
  }
}

async function refreshPackages() {
  if (isLoading.value || isDeleting.value) {
    return;
  }
  await loadPackages();
}

function handleItemsPerPageChange(value: number) {
  if (typeof value !== "number") {
    return;
  }
  const fallbackPerPage = perPageOptions[0] ?? perPage.value;
  const nextValue = perPageOptions.includes(value) ? value : fallbackPerPage;
  if (perPage.value === nextValue) {
    return;
  }
  perPage.value = nextValue;
  currentPage.value = 1;
}

function handlePageChange(value: number) {
  if (typeof value !== "number") {
    return;
  }
  if (value === currentPage.value) {
    return;
  }
  currentPage.value = value;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const idx = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, idx);
  return `${value.toFixed(idx === 0 ? 0 : 2)} ${units[idx]}`;
}

</script>

<style scoped lang="scss">
.packages {
  padding: 1rem;
}

// Ensure v-data-table respects theme colors and add animations
:deep(.v-data-table) {
  .v-data-table__th {
    color: var(--nr-text-primary);
    background-color: var(--nr-table-header-background);
    font-weight: 500;
    transition: all 0.2s ease;
    overflow: visible;
    position: relative;
    padding-right: 16px;
  }

  .v-data-table__td {
    color: var(--nr-text-primary);
    transition: all 0.2s ease;
  }

  .v-data-table__tr {
    transition: all 0.2s ease;

    &:hover {
      background-color: var(--nr-table-row-hover);
      transform: scale(1.001);
    }
  }

  .column-resizer {
    position: absolute;
    top: 0;
    right: -4px;
    width: 8px;
    height: 100%;
    cursor: col-resize;
    user-select: none;
    z-index: 3;
  }

  // Responsive improvements
  @media (max-width: 960px) {
    .v-data-table__th,
    .v-data-table__td {
      padding: 8px 12px;
      font-size: 0.875rem;
    }
  }

  @media (max-width: 600px) {
    .v-data-table__th,
    .v-data-table__td {
      padding: 6px 8px;
      font-size: 0.8rem;
    }
  }
}

// Add smooth card transitions
.v-card {
  transition: all 0.3s ease;
}

.packages__deletion-alert {
  margin: 0;
}

.packages__indexing-alert {
  border-left: 4px solid var(--v-theme-primary, #4c6ef5);
  padding: 0.5rem 0.75rem;
  background: rgba(76, 110, 245, 0.08);
  border-radius: 4px;
  font-size: 0.9rem;
}

.packages__footer {
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  row-gap: 8px;
}

.packages__range {
  min-width: 140px;
  text-align: center;
}

:deep(.v-table__wrapper) {
  overflow-x: hidden;
}
</style>
