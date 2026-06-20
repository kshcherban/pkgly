<!-- ABOUTME: Presents repository discovery, package search, and repository navigation. -->
<!-- ABOUTME: Keeps repository operational metadata compact and scannable. -->
<template>
  <v-container class="home-page pa-6">
    <v-row justify="center">
      <v-col cols="12" lg="10">
        <div class="home-page__heading">
          <div>
            <h1 class="text-h5 font-weight-medium">Repositories</h1>
            <p class="text-body-2 text-medium-emphasis">
              Find repositories and packages across this instance.
            </p>
          </div>
          <div
            data-testid="repository-summary"
            class="home-page__summary text-body-2 text-medium-emphasis">
            {{ repositorySummary }}
          </div>
        </div>
        <RepositorySearchHeader v-model="searchValue" />
      </v-col>
    </v-row>

    <v-row v-if="showPackageResults" class="mb-4" justify="center">
      <v-col cols="12" lg="10">
        <PackageSearchResults
          :results="packageResults"
          :loading="packageLoading"
          :error="packageError"
          @open="openPackage" />
      </v-col>
    </v-row>

    <v-row v-if="loading && !error" justify="center">
      <v-col cols="12" lg="10">
        <v-card class="text-center py-8">
          <v-progress-circular indeterminate color="primary" size="48" />
          <div class="mt-4 text-medium-emphasis">Loading repositories…</div>
        </v-card>
      </v-col>
    </v-row>

    <v-row v-else-if="error" justify="center">
      <v-col cols="12" lg="10">
        <v-alert type="error" variant="tonal" prominent>
          Failed to load repositories: {{ error }}
        </v-alert>
      </v-col>
    </v-row>

    <v-row
      v-else-if="filteredRepositories.length > 0"
      justify="center"
      class="g-4">
      <v-col
        v-for="repo in filteredRepositories"
        :key="repo.id"
        cols="12"
        sm="6"
        md="4"
        lg="3">
        <v-card
          :ripple="false"
          tabindex="0"
          role="link"
          class="repository-card h-100"
          @click="navigateToRepository(repo)"
          @keydown.enter="navigateToRepository(repo)"
          @keydown.space.prevent="navigateToRepository(repo)">
          <v-card-title class="repository-card__title">
            <span class="repository-card__icon mr-3">
              <component
                v-if="hasComponentIcon(repo.repository_type || '')"
                :is="getComponentIcon(repo.repository_type || '').component"
                v-bind="getComponentIcon(repo.repository_type || '').props"
                class="repository-card__brand-icon" />
              <v-icon
                v-else
                :icon="getFallbackIcon(repo.repository_type || '')"
                color="primary" />
            </span>
            <div class="repository-card__identity">
              <div class="text-h6 text-truncate">{{ repo.name || "Unknown" }}</div>
              <div class="repository-card__type text-caption text-medium-emphasis">
                <span>{{ (repo.repository_type || "Unknown").toUpperCase() }}</span>
                <span aria-hidden="true">·</span>
                <span>{{ repositoryKindLabel(repo) }}</span>
              </div>
            </div>
          </v-card-title>

          <v-card-text class="repository-card__metadata">
            <div class="repository-card__row">
              <span class="repository-card__label">Storage</span>
              <span class="text-truncate">{{ repo.storage_name || "Unknown" }}</span>
            </div>
            <div class="repository-card__row">
              <span class="repository-card__label">Access</span>
              <div class="repository-card__statuses">
                <StatusChip
                  :secured="repo.auth_enabled === true"
                  :active="repo.active !== false" />
              </div>
            </div>
            <div class="repository-card__row">
              <span class="repository-card__label">Usage</span>
              <span>{{ formatBytes(repo.storage_usage_bytes) }}</span>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <v-row v-else-if="repositories.length > 0" justify="center">
      <v-col cols="12" lg="10">
        <EmptyState
          data-testid="repository-no-match"
          icon="mdi-magnify-close"
          :title="`No repositories match &quot;${trimmedSearch}&quot;`"
          message="Try a repository name, type, or storage name." />
      </v-col>
    </v-row>

    <v-row v-else justify="center">
      <v-col cols="12" lg="8">
        <EmptyState
          icon="mdi-package-variant"
          title="No repositories available"
          message="Contact your administrator to create repositories.">
          <template v-if="isAdmin" #action>
            <v-btn
              color="primary"
              prepend-icon="mdi-plus"
              :to="{ name: 'RepositoryCreate' }"
              variant="flat">
              Create Repository
            </v-btn>
          </template>
        </EmptyState>
      </v-col>
    </v-row>
  </v-container>
</template>

<script setup lang="ts">
import PackageSearchResults, { type PackageResult } from "@/components/nr/repository/PackageSearchResults.vue";
import RepositorySearchHeader from "@/components/nr/repository/RepositorySearchHeader.vue";
import StatusChip from "@/components/ui/StatusChip.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import http from "@/http";
import { shouldFetchPackages, isAdvancedQuery, formatBytes as formatBytesUtil } from "@/utils/repositorySearch";
import { useRouter } from "vue-router";
import { computed, defineComponent, h, onBeforeUnmount, onMounted, ref, resolveComponent, watch } from "vue";
import type { Component } from "vue";
import { useRepositoryStore } from "@/stores/repositories";
import { sessionStore } from "@/stores/session";
import type { RepositoryWithStorageName } from "@/types/repository";
import { HelmIcon, DockerIcon, DebianIcon } from "vue3-simple-icons";
import CargoIcon from "@/components/nr/repository/types/cargo/CargoIcon.vue";

const router = useRouter();
const repositories = ref<RepositoryWithStorageName[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const searchValue = ref("");
const packageResults = ref<PackageResult[]>([]);
const packageLoading = ref(false);
const packageError = ref<string | null>(null);
let debounceHandle: number | undefined;
const repoStore = useRepositoryStore();
const session = sessionStore();
const user = computed(() => session.user);
const isAdmin = computed(() => Boolean(user.value?.admin));
function repositoryKindLabel(repo: RepositoryWithStorageName) {
  const kind = (repo.repository_kind ?? "hosted").toLowerCase();
  if (kind === "proxy") return "Proxy";
  if (kind === "virtual") return "Virtual";
  return "Hosted";
}

interface PackageSearchResponse {
  repository_id: string;
  repository_name: string;
  storage_name: string;
  repository_type: string;
  file_name: string;
  cache_path: string;
  size: number;
  modified: string;
}
type ComponentIcon = {
  component: Component;
  props?: Record<string, unknown>;
};

const MdiBrandIcon = defineComponent({
  props: {
    icon: {
      type: String,
      required: true,
    },
    color: {
      type: String,
      default: "primary",
    },
    size: {
      type: [String, Number],
      default: "32",
    },
  },
  setup(props) {
    // Don't use a template here: Vuetify component auto-registration is handled
    // by the SFC compiler transform; runtime templates won't see it.
    const VIcon = resolveComponent("v-icon") as Component;
    return () =>
      h(VIcon, {
        icon: props.icon,
        color: props.color,
        size: props.size,
      });
  },
});

const componentIconMap: Record<string, ComponentIcon> = {
  helm: {
    component: HelmIcon,
    props: {
      size: "32",
      color: "#0F1689",
    },
  },
  docker: {
    component: DockerIcon,
    props: {
      size: "32",
      color: "#2496ED",
    },
  },
  cargo: {
    component: CargoIcon,
  },
  deb: {
    component: DebianIcon,
    props: {
      size: "32",
      color: "#A81D33",
    },
  },
  ruby: {
    component: MdiBrandIcon,
    props: {
      icon: "mdi-language-ruby",
      color: "#CC342D",
      size: "32",
    },
  },
};

const fallbackIconMap: Record<string, string> = {
  maven: "mdi-language-java",
  npm: "mdi-nodejs",
  go: "mdi-language-go",
  python: "mdi-language-python",
  php: "mdi-language-php",
  cargo: "mdi-language-rust",
  deb: "mdi-linux",
};

function normalizeType(type: string): string {
  return type?.toLowerCase?.() ?? "";
}

function hasComponentIcon(type: string): boolean {
  return Boolean(componentIconMap[normalizeType(type)]);
}

function getComponentIcon(type: string): ComponentIcon {
  return componentIconMap[normalizeType(type)]!;
}

function getFallbackIcon(type: string): string {
  const normalized = normalizeType(type);
  return fallbackIconMap[normalized] ?? "mdi-package-variant";
}

// Filter repositories based on search term
const trimmedSearch = computed(() => searchValue.value.trim());
const showPackageResults = computed(() => shouldFetchPackages(trimmedSearch.value));

watch(trimmedSearch, (value) => {
  packageResults.value = [];
  packageError.value = null;
  if (debounceHandle !== undefined) {
    window.clearTimeout(debounceHandle);
    debounceHandle = undefined;
  }
  if (!shouldFetchPackages(value)) {
    packageLoading.value = false;
    return;
  }
  packageLoading.value = true;
  debounceHandle = window.setTimeout(() => {
    fetchPackages(value);
  }, 300);
});

onBeforeUnmount(() => {
  if (debounceHandle !== undefined) {
    window.clearTimeout(debounceHandle);
  }
});

const filteredRepositories = computed(() => {
  if (!repositories.value) {
    return [];
  }
  const rawQuery = trimmedSearch.value;
  if (!rawQuery) {
    return repositories.value;
  }
  if (isAdvancedQuery(rawQuery)) {
    return repositories.value;
  }
  const term = rawQuery.toLowerCase();
  return repositories.value.filter(
    (repo) =>
      repo?.name?.toLowerCase().includes(term) ||
      repo?.repository_type?.toLowerCase().includes(term) ||
      repo?.storage_name?.toLowerCase().includes(term),
  );
});

const repositorySummary = computed(() => {
  const total = repositories.value.length;
  const noun = total === 1 ? "repository" : "repositories";
  if (trimmedSearch.value && !isAdvancedQuery(trimmedSearch.value)) {
    return `${filteredRepositories.value.length} of ${total} ${noun}`;
  }
  return `${total} ${noun}`;
});

// Format bytes for display
function formatBytes(bytes?: number | null): string {
  if (bytes === null || bytes === undefined) {
    return "—";
  }
  return formatBytesUtil(bytes);
}

// Navigate to repository
function navigateToRepository(repo: RepositoryWithStorageName) {
  router.push({
    name: "repository_page_by_name",
    params: {
      storageName: repo.storage_name,
      repositoryName: repo.name,
    },
  });
}

async function getRepositories() {
  loading.value = true;
  error.value = null;
  try {
    const response = await repoStore.getRepositories();
    repositories.value = Array.isArray(response) ? response : [];
  } catch (err) {
    console.error(err);
    error.value = "Failed to load repositories";
    repositories.value = [];
  } finally {
    loading.value = false;
  }
}

onMounted(getRepositories);

async function fetchPackages(query: string) {
  try {
    const response = await http.get<PackageSearchResponse[]>("/api/search/packages", {
      params: { q: query, limit: 25 },
    });
    packageResults.value = response.data.map((item) => ({
      repositoryId: item.repository_id,
      repositoryName: item.repository_name,
      storageName: item.storage_name,
      repositoryType: item.repository_type,
      fileName: item.file_name,
      cachePath: item.cache_path,
      size: item.size,
      modified: item.modified,
    }));
  } catch (err) {
    console.error(err);
    packageError.value = "Failed to search packages";
  } finally {
    packageLoading.value = false;
  }
}

function openPackage(pkg: PackageResult) {
  const parentPath = pkg.cachePath.split("/").slice(0, -1).join("/");
  router.push({
    name: "Browse",
    params: { id: pkg.repositoryId, catchAll: parentPath },
  });
}
</script>

<style scoped lang="scss">
.home-page {
  max-width: 1280px;
}

.home-page__heading {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--nr-spacing-md);
}

.home-page__heading p {
  margin-top: var(--nr-spacing-xs);
}

.home-page__summary {
  white-space: nowrap;
}

.repository-card {
  cursor: pointer;
  border: 1px solid var(--nr-card-border);
  border-radius: var(--nr-radius-lg);
  box-shadow: none;
  transition:
    border-color var(--nr-transition-fast),
    box-shadow var(--nr-transition-fast),
    transform var(--nr-transition-fast);

  &:hover {
    border-color: var(--nr-primary);
    box-shadow: var(--nr-surface-hover-shadow);
    transform: var(--nr-hover-lift);
  }

  &:focus-visible {
    border-color: var(--nr-primary);
    box-shadow: var(--nr-focus-ring);
    outline: none;
  }
}

.repository-card__title {
  display: flex;
  align-items: center;
  padding: var(--nr-spacing-md);
}

.repository-card__identity {
  min-width: 0;
}

.repository-card__type {
  display: flex;
  align-items: center;
  gap: var(--nr-spacing-xs);
}

.repository-card__metadata {
  display: flex;
  flex-direction: column;
  gap: var(--nr-spacing-sm);
  padding: 0 var(--nr-spacing-md) var(--nr-spacing-md);
}

.repository-card__row {
  min-height: 24px;
  display: grid;
  grid-template-columns: 64px minmax(0, 1fr);
  align-items: center;
  gap: var(--nr-spacing-sm);
  font-size: var(--nr-font-size-sm);
}

.repository-card__label {
  color: var(--nr-text-secondary);
}

.repository-card__statuses {
  display: flex;
  gap: var(--nr-spacing-xs);
  flex-wrap: wrap;
}

.repository-card__brand-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.repository-card__brand-icon :deep(svg) {
  width: 32px;
  height: 32px;
}

@media (max-width: 600px) {
  .home-page__heading {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
