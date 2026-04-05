<template>
  <section class="packages">
    <header class="packages__header">
      <div class="packages__title-row">
        <h2>Packages</h2>
        <div class="packages__search">
          <input
            type="search"
            :value="packageSearchTerm"
            data-testid="packages-search-input"
            @input="onSearchInput"
            placeholder="Search packages in this repository..."
            aria-label="Search packages" />
          <button
            v-if="packageSearchTerm"
            type="button"
            class="packages__search-clear"
            @click="clearPackageSearch"
            aria-label="Clear package search">
            ×
          </button>
        </div>
      </div>
      <div
        v-if="!isLoading"
        class="packages__counts">
        <span>{{ totalPackages }} package(s)</span>
        <span v-if="visiblePackages.length">
          Showing {{ visiblePackages.length }} file(s)
        </span>
      </div>
      <div
        v-if="!isLoading && visibleIndexingWarning"
        data-testid="public-packages-indexing-warning"
        class="packages__indexing-warning">
        {{ visibleIndexingWarning }}
      </div>
    </header>

    <div
      v-if="isLoading"
      class="packages__state">
      Loading packages...
    </div>
    <div
      v-else-if="error"
      class="packages__state packages__state--error">
      Failed to load packages: {{ error }}
    </div>
    <div
      v-else-if="totalPackages === 0"
      class="packages__state">
      {{ emptyRepositoryMessage }}
    </div>
    <div
      v-else-if="visiblePackages.length === 0"
      class="packages__state">
      No packages match your filters on this page.
    </div>
    <div
      v-else
      class="packages__table-container">
      <table class="packages__table">
        <colgroup>
          <col
            v-for="column in visibleColumns"
            :key="column.key"
            :class="`packages__col--${column.key}`"
            :style="columnStyle(column.key)" />
        </colgroup>
        <thead>
          <tr>
            <th
              v-for="column in visibleColumns"
              :key="column.key"
              :data-column="column.key"
              :class="[
                'packages__header-cell',
                `packages__column--${column.key}`,
                column.align === 'right' ? 'packages__header-cell--numeric' : 'packages__header-cell--text',
                sortState?.key === column.key ? 'packages__header-cell--sorted' : '',
              ]">
              <div class="packages__header-content">
                <button
                  type="button"
                  class="packages__sort-button"
                  :class="{
                    'packages__sort-button--numeric': column.align === 'right',
                    'packages__sort-button--active': sortState?.key === column.key,
                  }"
                  :data-testid="`sort-${column.key}`"
                  @click="toggleSort(column.key)">
                  <span>{{ column.label }}</span>
                  <span
                    class="packages__sort-indicator"
                    :class="{
                      'packages__sort-indicator--asc':
                        sortState?.key === column.key && sortState?.direction === 'asc',
                      'packages__sort-indicator--desc':
                        sortState?.key === column.key && sortState?.direction === 'desc',
                    }"
                    aria-hidden="true" />
                  <span
                    v-if="sortState?.key === column.key"
                    class="sr-only">
                    Sorted {{ sortState.direction === "asc" ? "ascending" : "descending" }}
                  </span>
                </button>
                <div
                  v-if="isConfigAnchor(column.key)"
                  class="packages__header-actions">
                  <button
                    type="button"
                    class="packages__options-button"
                    @click="toggleConfig"
                    data-testid="packages-config-toggle"
                    :aria-expanded="isConfigOpen"
                    aria-controls="packages-config-panel"
                    title="Configure columns">
                    <span class="sr-only">Configure columns</span>
                    ⚙
                  </button>
                  <div
                    v-if="isConfigOpen"
                    id="packages-config-panel"
                    class="packages__config-panel">
                    <fieldset class="packages__config-fieldset">
                      <legend class="sr-only">Table columns</legend>
                      <div class="packages__config-columns">
                        <label
                          v-for="column in optionalColumns"
                          :key="column.key"
                          class="packages__config-option">
                          <input
                            type="checkbox"
                            :checked="!hiddenColumnsSet.has(column.key)"
                            @change="toggleColumn(column.key)"
                            :data-testid="`toggle-${column.key}`" />
                          {{ column.label }}
                        </label>
                      </div>
                    </fieldset>
                  </div>
                </div>
              </div>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="pkg in visiblePackages"
            :key="rowKey(pkg)"
            data-testid="package-row">
            <td
              v-for="column in visibleColumns"
              :key="column.key"
              :class="[
                'packages__cell',
                `packages__column--${column.key}`,
                column.align === 'right' ? 'packages__cell--numeric' : 'packages__cell--text',
              ]">
              <div
                class="packages__cell-content"
                :title="cellTitle(column.key, pkg)">
                <template v-if="column.key === 'path' || column.key === 'digest'">
                  <code>{{ cellText(column.key, pkg) }}</code>
                </template>
                <template v-else>
                  <button
                    v-if="column.key === 'package'"
                    type="button"
                    class="packages__repository-link"
                    data-testid="package-cell"
                    @click="browsePackage(pkg)">
                    {{ cellText(column.key, pkg) }}
                  </button>
                  <span v-else>
                    {{ cellText(column.key, pkg) }}
                  </span>
                </template>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div
      v-if="totalPackages > 0"
      class="packages__pager">
      <v-btn
        variant="tonal"
        color="primary"
        class="text-none"
        @click="prevPage"
        :disabled="currentPage === 1">
        Previous
      </v-btn>
      <span class="packages__pager-label">{{ pageLabel }}</span>
      <v-btn
        variant="tonal"
        color="primary"
        class="text-none"
        @click="nextPage"
        :disabled="currentPage >= totalPages">
        Next
      </v-btn>
      <v-select
        class="packages__pager-select"
        label="Per page"
        :items="perPageOptions"
        v-model="perPageModel"
        variant="outlined"
        density="compact"
        hide-details
        style="max-width: 140px" />
    </div>
  </section>
</template>

<script setup lang="ts">
import http from "@/http";
import { computed, nextTick, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useResizableColumns } from "@/composables/useResizableColumns";
import { shouldDisplayRepositoryIndexingWarning } from "@/types/repository";

interface PackageEntry {
  name: string;
  size: number;
  cachePath: string;
  blobDigest: string;
  modified: string;
  package: string;
}

type ColumnKey = "package" | "name" | "digest" | "size" | "path" | "timestamp";
type SortDirection = "asc" | "desc";

interface ColumnDefinition {
  key: ColumnKey;
  label: string;
  optional: boolean;
  align?: "left" | "right";
  sortable?: boolean;
}

const props = defineProps<{
  repositoryId: string;
  repositoryType?: string;
  repositoryKind?: string | null;
  perPageOptions?: number[] | null;
}>();
const router = useRouter();

const packages = ref<PackageEntry[]>([]);
const isLoading = ref(false);
const error = ref<string | null>(null);
const currentPage = ref(1);
const defaultPerPageOptions = [50, 100, 200, 500, 1000] as const;
const perPageOptions = computed<number[]>(() => {
  const unique = new Set<number>();
  const normalized: number[] = [];
  const input = props.perPageOptions?.length ? props.perPageOptions : defaultPerPageOptions;
  for (const rawValue of input) {
    if (!Number.isFinite(rawValue) || !Number.isInteger(rawValue) || rawValue <= 0) {
      continue;
    }
    if (unique.has(rawValue)) {
      continue;
    }
    unique.add(rawValue);
    normalized.push(rawValue);
  }
  return normalized.length ? normalized : [...defaultPerPageOptions];
});
const perPage = ref(100);
const totalPackages = ref(0);
const sortState = ref<{ key: ColumnKey; direction: SortDirection } | null>(null);
const isConfigOpen = ref(false);
const hiddenColumns = ref<Set<ColumnKey>>(new Set<ColumnKey>());
const lastRequestToken = ref<symbol | null>(null);
const packageSearchTerm = ref("");
const indexingWarning = ref<string | null>(null);

const perPageModel = computed({
  get: () => perPage.value,
  set: (value: number | null) => {
    if (typeof value === "number" && perPageOptions.value.includes(value)) {
      if (perPage.value !== value) {
        perPage.value = value;
        currentPage.value = 1;
      }
    }
  },
});

const storageKey = computed(() =>
  props.repositoryId ? `nr-packages-public:${props.repositoryId}` : null,
);

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
const isGoRepository = computed(() => repositoryType.value === "go");
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

const packageColumnTitle = computed(() => {
  if (isDockerRepository.value) {
    return "Repository";
  }
  if (isGoRepository.value) {
    return "Module";
  }
  return "Package";
});
const nameColumnTitle = computed(() => {
  if (isDockerRepository.value) {
    return "Tag";
  }
  if (isGoRepository.value || isDebRepository.value || isPhpProxy.value) {
    return "Version";
  }
  return "Name";
});
const pathColumnTitle = computed(() => {
  if (isDockerRepository.value) {
    return "Manifest Path";
  }
  if (isGoRepository.value) {
    return "Path";
  }
  return isHostedRepository.value ? "Path" : "Cached Path";
});
const timestampColumnTitle = "Uploaded At";

const columns = computed<ColumnDefinition[]>(() => {
  const base: ColumnDefinition[] = [
    { key: "package", label: packageColumnTitle.value, optional: false, sortable: true },
    { key: "name", label: nameColumnTitle.value, optional: false, sortable: true },
    { key: "digest", label: "Blob Digest", optional: true, sortable: true },
    { key: "size", label: "Size", optional: false, align: "right", sortable: true },
    { key: "path", label: pathColumnTitle.value, optional: true, sortable: true },
    { key: "timestamp", label: timestampColumnTitle, optional: true, sortable: true },
  ];
  return base;
});

const optionalColumns = computed(() => columns.value.filter((column) => column.optional));

const hiddenColumnsSet = computed<Set<ColumnKey>>(() => hiddenColumns.value);

const visibleColumns = computed(() =>
  columns.value.filter((column) => !hiddenColumns.value.has(column.key)),
);

const orderedPackages = computed(() => {
  if (!sortState.value) {
    return [...packages.value];
  }
  const { key, direction } = sortState.value;
  const extractor: Record<ColumnKey, (pkg: PackageEntry) => string | number> = {
    package: (pkg) => pkg.package.toLowerCase(),
    name: (pkg) => pkg.name.toLowerCase(),
    digest: (pkg) => pkg.blobDigest.toLowerCase(),
    size: (pkg) => pkg.size,
    path: (pkg) => pkg.cachePath.toLowerCase(),
    timestamp: (pkg) => new Date(pkg.modified).getTime(),
  };
  const sorted = [...packages.value].sort((a, b) => {
    const aVal = extractor[key](a);
    const bVal = extractor[key](b);
    if (typeof aVal === "number" && typeof bVal === "number") {
      return aVal - bVal;
    }
    return String(aVal).localeCompare(String(bVal), undefined, {
      numeric: true,
      sensitivity: "base",
    });
  });
  if (direction === "desc") {
    return sorted.reverse();
  }
  return sorted;
});

const normalizedPackageSearch = computed(() => packageSearchTerm.value.trim());

const visiblePackages = computed(() => orderedPackages.value);

const totalPages = computed(() => {
  if (totalPackages.value === 0) {
    return 1;
  }
  return Math.max(1, Math.ceil(totalPackages.value / perPage.value));
});

const pageLabel = computed(() => {
  if (totalPackages.value === 0) {
    return "Page 1 of 1";
  }
  const start = (currentPage.value - 1) * perPage.value + 1;
  const end =
    visiblePackages.value.length === 0
      ? start - 1
      : start + visiblePackages.value.length - 1;
  return `Page ${currentPage.value} of ${totalPages.value} · Showing ${Math.max(start, 0)}-${Math.max(end, 0)}`;
});

function defaultHiddenColumns(): Set<ColumnKey> {
  const defaults = new Set<ColumnKey>();
  if (isDockerRepository.value) {
    defaults.add("digest");
    defaults.add("path");
  }
  return defaults;
}

const emptyRepositoryMessage = computed(() => {
  if (isDockerRepository.value) {
    if (isDockerProxy.value) {
      return "No images cached yet. Pull an image through this proxy to populate the list.";
    }
    return "No images yet. Push an image to populate this list.";
  }
  if (isHostedRepository.value) {
    return "No packages yet. Upload a package to populate this list.";
  }
  return "No cached packages yet. Trigger a download to populate this list.";
});

function sortByParam(): string {
  if (!sortState.value) {
    return "modified";
  }
  const map: Record<ColumnKey, string> = {
    package: "package",
    name: "name",
    digest: "digest",
    size: "size",
    path: "path",
    timestamp: "modified",
  };
  return map[sortState.value.key];
}

async function loadPackages() {
  if (!props.repositoryId) {
    return;
  }
  const requestToken = Symbol("packages-request");
  lastRequestToken.value = requestToken;
  isLoading.value = true;
  error.value = null;
  try {
    const search = normalizedPackageSearch.value;
    const response = await http.get(`/api/repository/${props.repositoryId}/packages`, {
      params: {
        page: currentPage.value,
        per_page: perPage.value,
        sort_by: sortByParam(),
        sort_dir: sortState.value?.direction ?? "desc",
        ...(search ? { q: search } : {}),
      },
    });
    if (lastRequestToken.value !== requestToken) {
      return;
    }
    const data = response.data ?? {};
    const items: PackageEntry[] = (data.items ?? []).map((item: any) => ({
      name: item.name ?? "",
      size: Number(item.size ?? 0),
      cachePath: item.cache_path ?? "",
      blobDigest: item.blob_digest ?? "",
      modified: item.modified ?? "",
      package: item.package ?? "",
    }));
    packages.value = items;
    totalPackages.value = Number(data.total_packages ?? 0);
    const warning = response.headers?.["x-pkgly-warning"] ?? response.headers?.["X-Pkgly-Warning"];
    const normalizedWarning =
      typeof warning === "string" && warning.trim().length > 0 ? warning.trim() : null;
    indexingWarning.value =
      showIndexingWarning.value ? normalizedWarning : null;
  } catch (err) {
    if (lastRequestToken.value !== requestToken) {
      return;
    }
    console.error(err);
    error.value = err instanceof Error ? err.message : String(err);
    indexingWarning.value = null;
  } finally {
    if (lastRequestToken.value === requestToken) {
      isLoading.value = false;
    }
  }
}

function toggleSort(key: ColumnKey) {
  const column = columns.value.find((item) => item.key === key);
  if (!column?.sortable) {
    return;
  }
  if (sortState.value && sortState.value.key === key) {
    sortState.value = {
      key,
      direction: sortState.value.direction === "asc" ? "desc" : "asc",
    };
  } else {
    sortState.value = { key, direction: "asc" };
  }
}

function toggleConfig() {
  isConfigOpen.value = !isConfigOpen.value;
}

function toggleColumn(key: ColumnKey) {
  const column = columns.value.find((item) => item.key === key);
  if (!column || !column.optional) {
    return;
  }
  const next = new Set<ColumnKey>(hiddenColumns.value);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  hiddenColumns.value = next;
}

function nextPage() {
  if (currentPage.value < totalPages.value) {
    currentPage.value += 1;
  }
}

function prevPage() {
  if (currentPage.value > 1) {
    currentPage.value -= 1;
  }
}

function loadPreferences() {
  if (typeof window === "undefined" || !storageKey.value) {
    perPage.value = 100;
    hiddenColumns.value = defaultHiddenColumns();
    return;
  }
  try {
    const raw = window.localStorage.getItem(storageKey.value);
    if (!raw) {
      perPage.value = 100;
      hiddenColumns.value = defaultHiddenColumns();
      return;
    }
    const parsed = JSON.parse(raw) as { hiddenColumns?: ColumnKey[]; perPage?: number };
    const hidden = Array.isArray(parsed.hiddenColumns)
      ? parsed.hiddenColumns.filter((key: ColumnKey) =>
          columns.value.some((column) => column.optional && column.key === key),
        )
      : Array.from(defaultHiddenColumns());
    hiddenColumns.value = new Set<ColumnKey>(
      hidden.length > 0 ? hidden : Array.from(defaultHiddenColumns()),
    );
    if (typeof parsed.perPage === "number" && perPageOptions.value.includes(parsed.perPage)) {
      perPage.value = parsed.perPage;
    } else {
      perPage.value = 100;
    }
  } catch (err) {
    console.error("Failed to load package view preferences", err);
    perPage.value = 100;
    hiddenColumns.value = defaultHiddenColumns();
  }
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const idx = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const value = bytes / Math.pow(1024, idx);
  const precision = idx === 0 ? 0 : 2;
  return `${value.toFixed(precision)} ${units[idx]}`;
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value ?? "";
  }
  return date.toLocaleString();
}

function cellText(column: ColumnKey, pkg: PackageEntry): string {
  switch (column) {
    case "package":
      return pkg.package;
    case "name":
      return pkg.name;
    case "digest":
      return pkg.blobDigest;
    case "size":
      return formatBytes(pkg.size);
    case "path":
      return pkg.cachePath;
    case "timestamp":
      return formatTimestamp(pkg.modified);
    default:
      return "";
  }
}

function cellTitle(column: ColumnKey, pkg: PackageEntry): string {
  switch (column) {
    case "size":
      return formatBytes(pkg.size);
    case "timestamp":
      return formatTimestamp(pkg.modified);
    default:
      return cellText(column, pkg);
  }
}

function browsePackage(pkg: PackageEntry) {
  router.push({
    name: "Browse",
    params: {
      id: props.repositoryId,
      catchAll: packageBrowsePath(pkg),
    },
  });
}

function packageBrowsePath(pkg: PackageEntry): string {
  if (repositoryType.value === "docker") {
    return pkg.package.trim().replace(/^\/+|\/+$/g, "");
  }
  const cachePath = pkg.cachePath;
  const segments = cachePath
    .split("/")
    .map((segment) => segment.trim())
    .filter((segment) => segment.length > 0);
  if (segments.length <= 1) {
    return "";
  }
  return segments.slice(0, -1).join("/");
}

function rowKey(pkg: PackageEntry): string {
  const identifier = pkg.cachePath || `${pkg.package}-${pkg.name}`;
  return `${identifier}-${pkg.modified}`;
}

function columnStyle(columnKey: ColumnKey): { width: string } | undefined {
  if (columnKey === "size") {
    return { width: "1%" };
  }
  return undefined;
}

function onSearchInput(event: Event) {
  const target = event.target as HTMLInputElement;
  if (currentPage.value !== 1) {
    currentPage.value = 1;
  }
  packageSearchTerm.value = target.value;
}

function clearPackageSearch() {
  if (currentPage.value !== 1) {
    currentPage.value = 1;
  }
  packageSearchTerm.value = "";
}

function isConfigAnchor(columnKey: ColumnKey) {
  const lastKey = visibleColumns.value[visibleColumns.value.length - 1]?.key;
  return lastKey === columnKey;
}

watch(
  () => props.repositoryId,
  () => {
    packages.value = [];
    error.value = null;
    currentPage.value = 1;
    isConfigOpen.value = false;
    hiddenColumns.value = new Set<ColumnKey>();
    packageSearchTerm.value = "";
    indexingWarning.value = null;
    loadPreferences();
  },
  { immediate: true },
);

watch(
  () =>
    [props.repositoryId, currentPage.value, perPage.value, normalizedPackageSearch.value] as const,
  ([repositoryId]) => {
    if (!repositoryId) {
      return;
    }
    loadPackages();
  },
  { immediate: true },
);

// Enable resizable columns
const { initResizable: initPackageTableResizers } = useResizableColumns('.packages__table');

const visibleColumnSignature = computed(() => visibleColumns.value.map((column) => column.key).join("|"));

watch(
  () => [isLoading.value, visibleColumnSignature.value] as const,
  ([loading]) => {
    if (loading || visiblePackages.value.length === 0) {
      return;
    }
    nextTick(() => {
      initPackageTableResizers();
    });
  },
  { flush: "post" },
);

watch(
  () => ({
    hidden: Array.from(hiddenColumns.value),
    perPage: perPage.value,
    key: storageKey.value,
  }),
  (state) => {
    if (!state.key || typeof window === "undefined") {
      return;
    }
    const payload = {
      hiddenColumns: state.hidden,
      perPage: state.perPage,
    };
    window.localStorage.setItem(state.key, JSON.stringify(payload));
  },
  { deep: true },
);
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  border: 0;
}

.packages {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1rem 1.5rem;
}

.packages__header {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.packages__title-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.packages__search {
  position: relative;
  flex: 1 1 240px;
  max-width: 360px;
}

.packages__search input {
  width: 100%;
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.15));
  border-radius: 0.5rem;
  padding: 0.5rem 0.75rem;
  background: var(--nr-background-primary, #fff);
  color: var(--nr-text-color, inherit);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.packages__search input:focus-visible {
  outline: none;
  border-color: var(--nr-primary-color, #4c6ef5);
  box-shadow: 0 0 0 3px rgba(76, 110, 245, 0.2);
}

.packages__search-clear {
  position: absolute;
  top: 50%;
  right: 0.5rem;
  transform: translateY(-50%);
  border: none;
  background: transparent;
  color: var(--text-secondary, #6c757d);
  font-size: 1.1rem;
  cursor: pointer;
  padding: 0;
}

.packages__config-panel {
  position: absolute;
  top: calc(100% + 0.5rem);
  right: 0;
  padding: 0.75rem;
  background: var(--nr-background-secondary, #f8f9fa);
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.15));
  border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.15);
  z-index: 3;
  min-width: 220px;
}

.packages__config-fieldset {
  border: 0;
  margin: 0;
  padding: 0;
}

.packages__config-columns {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.packages__config-option {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.9rem;
}

.packages__counts {
  display: flex;
  gap: 0.75rem;
  color: var(--text-secondary, #6c757d);
  font-size: 0.9rem;
  flex-wrap: wrap;
}

.packages__indexing-warning {
  border-left: 4px solid var(--nr-primary-color, #4c6ef5);
  padding: 0.5rem 0.75rem;
  border-radius: 4px;
  background: rgba(76, 110, 245, 0.12);
  color: var(--nr-primary-color, #4c6ef5);
  margin-bottom: 0.75rem;
  font-size: 0.9rem;
}

.packages__state {
  color: var(--text-secondary, #6c757d);
}

.packages__state--error {
  color: var(--error-color, #d9534f);
}

.packages__table-container {
  overflow-x: auto;
  border-radius: 8px;
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.1));
  padding: 0.25rem 0.75rem 0.75rem;
  background: var(--nr-background-primary, #fff);
  position: relative;
}

.packages__options-button {
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 50%;
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.15));
  background: var(--nr-background-tertiary, #f5f6fb);
  color: var(--nr-text-color, inherit);
  font-size: 1.1rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background-color 0.2s ease, color 0.2s ease, border-color 0.2s ease;
}

.packages__options-button:hover,
.packages__options-button:focus-visible {
  outline: none;
  background: var(--nr-primary-color, #4c6ef5);
  color: #fff;
  border-color: var(--nr-primary-color, #4c6ef5);
}

.packages__table {
  width: 100%;
  min-width: 720px;
  border-collapse: collapse;
  table-layout: auto;
  color: var(--nr-text-color, inherit);
}

.column-resizer {
  background: transparent;
}

.column-resizer::after {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  left: 50%;
  width: 1px;
  background: rgba(0, 0, 0, 0.15);
  transform: translateX(-50%);
}

.packages__header-cell {
  padding: 0;
  background: var(--nr-background-tertiary, #f8f9fa);
  border-bottom: 2px solid var(--nr-border-color, rgba(0, 0, 0, 0.1));
  position: relative;
}

.packages__header-cell--sorted {
  background: var(--nr-background-tertiary-emphasis, rgba(0, 0, 0, 0.02));
}

.packages__column--size {
  white-space: nowrap;
}

.packages__header-content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  position: relative;
}

.packages__header-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  position: relative;
}

.packages__sort-button {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.6rem 0.75rem;
  font-weight: 600;
  font-size: 0.95rem;
  background: transparent;
  border: none;
  color: inherit;
  cursor: pointer;
  text-align: left;
}

.packages__sort-button:hover,
.packages__sort-button--active {
  background: var(--nr-table-row-hover, rgba(30, 136, 229, 0.08));
}

.packages__sort-button--numeric {
  justify-content: flex-end;
}

.packages__sort-indicator {
  width: 0;
  height: 0;
  border-left: 4px solid transparent;
  border-right: 4px solid transparent;
}

.packages__sort-indicator--asc {
  border-bottom: 6px solid currentColor;
}

.packages__sort-indicator--desc {
  border-top: 6px solid currentColor;
}

.packages__cell {
  padding: 0.6rem 0.75rem;
  border-bottom: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.08));
}

.packages__cell--numeric {
  text-align: right;
}

.packages__cell--text {
  text-align: left;
}

.packages__cell-content {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.packages__repository-link {
  padding: 0;
  border: none;
  background: transparent;
  color: $accent;
  cursor: pointer;
  font: inherit;
}

.packages__repository-link:hover {
  text-decoration: underline;
}

.packages__cell code {
  font-family: var(--nr-font-mono, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace);
  font-size: 0.85rem;
  background: var(--nr-background-tertiary, #f8f9fa);
  color: inherit;
  padding: 0.15rem 0.25rem;
  border-radius: 4px;
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.1));
  display: inline-block;
  max-width: 100%;
}

.packages__pager {
  margin-top: 0.5rem;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.packages__pager-select select {
  margin-left: 0.35rem;
}
</style>
