<template>
  <v-container class="pa-6">
    <v-row class="mb-4" justify="center">
      <v-col cols="12" lg="10">
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
          class="repository-card h-100"
          @click="navigateToRepository(repo)">
          <v-card-title class="d-flex align-center pa-4">
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
            <div>
              <div class="text-h6">{{ repo.name || "Unknown" }}</div>
              <div class="text-caption text-medium-emphasis d-flex align-center gap-2">
                <span>{{ (repo.repository_type || "").toUpperCase() }}</span>
                <v-chip
                  size="x-small"
                  :color="repo.repository_kind?.toLowerCase() === 'proxy'
                    ? 'primary'
                    : repo.repository_kind?.toLowerCase() === 'virtual'
                      ? '#b388ff'
                      : 'default'"
                  variant="tonal">
                  {{ repositoryKindLabel(repo) }}
                </v-chip>
              </div>
            </div>
          </v-card-title>

          <v-card-text class="pa-4 pt-0">
            <div class="d-flex align-center gap-4 text-caption">
              <div class="d-flex align-center gap-1">
                <v-icon size="small">mdi-database</v-icon>
                {{ repo.storage_name || "Unknown" }}
              </div>
              <div class="d-flex align-center gap-1">
                <v-icon size="small">mdi-shield-check</v-icon>
                <span :class="repo.auth_enabled ? 'text-success' : 'text-warning'">
                  {{ repo.auth_enabled ? "Secured" : "Unsecured" }}
                </span>
              </div>
            </div>

            <div v-if="repo.storage_usage_bytes !== undefined" class="mt-2">
              <div class="d-flex align-center gap-1 text-caption">
                <v-icon size="small">mdi-hard-disk</v-icon>
                {{ formatBytes(repo.storage_usage_bytes) }}
              </div>
            </div>
          </v-card-text>

          <v-card-actions class="pa-4 pt-0">
            <v-btn color="primary" variant="text" prepend-icon="mdi-open-in-new" class="text-none">
              Open
            </v-btn>
            <v-spacer />
            <v-chip
              :color="repo.active ? 'success' : 'default'"
              :variant="repo.active ? 'flat' : 'outlined'"
              size="small">
              {{ repo.active ? "Active" : "Inactive" }}
            </v-chip>
          </v-card-actions>
        </v-card>
      </v-col>
    </v-row>

    <v-row v-else justify="center">
      <v-col cols="12" lg="8">
        <v-card class="text-center py-12" variant="outlined">
          <v-icon color="medium-emphasis" size="64" class="mb-4">mdi-package-variant</v-icon>
          <h2 class="text-h4 text-medium-emphasis mb-2">No repositories available</h2>
          <p class="text-body-1 text-medium-emphasis mb-6">
            Contact your administrator to create repositories.
          </p>
          <v-btn
            v-if="isAdmin"
            color="primary"
            prepend-icon="mdi-plus"
            :to="{ name: 'RepositoryCreate' }"
            variant="flat">
            Create Repository
          </v-btn>
        </v-card>
      </v-col>
    </v-row>
  </v-container>
</template>

<script setup lang="ts">
import PackageSearchResults, { type PackageResult } from "@/components/nr/repository/PackageSearchResults.vue";
import RepositorySearchHeader from "@/components/nr/repository/RepositorySearchHeader.vue";
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
.repository-card {
  cursor: pointer;
  transition: transform 0.2s ease-in-out;

  &:hover {
    transform: translateY(-4px);
  }
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

:deep(.v-card--hover) {
  cursor: pointer;
}

</style>
