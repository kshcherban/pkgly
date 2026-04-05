<template>
  <v-container v-if="repository" class="repository-page" fluid>
    <v-card variant="flat" class="repository-page__header">
      <v-card-text class="repository-page__header-shell">
        <div class="repository-page__header-summary">
          <div class="repository-page__title">
            <h1 class="text-h5 text-md-h4 font-weight-semibold mb-0">
              {{ repository.storage_name }}/{{ repository.name }}
            </h1>
          </div>
          <button
            type="button"
            class="repository-page__header-toggle"
            data-testid="repository-header-toggle"
            :aria-expanded="isHeaderExpanded"
            @click="isHeaderExpanded = !isHeaderExpanded">
            {{ isHeaderExpanded ? "Hide Setup" : "Show Setup" }}
          </button>
        </div>

        <div
          v-if="isHeaderExpanded"
          class="repository-page__header-details">
          <div class="repository-page__header-main">
            <div class="repository-page__meta">
              <div
                v-if="repositoryType"
                class="repository-page__icons">
                <RepositoryIcon
                  v-for="icon in repositoryType.icons"
                  :key="icon.name"
                  :name="repositoryType.name"
                  :icon="icon" />
              </div>
              <CopyURL :code="url" />
            </div>
          </div>
          <div class="repository-page__header-helper">
            <RepositoryHelper :repository="repository" />
          </div>
        </div>
      </v-card-text>
    </v-card>

    <RepositoryPackagesPublic
      v-if="showPackages"
      :repository-id="repository.id"
      :repository-type="repository.repository_type"
      :repository-kind="repository.repository_kind ?? null"
      :per-page-options="packagePerPageOptions" />
  </v-container>
  <ErrorOnRequest
    v-else-if="error"
    :error="error"
    :errorCode="errorCode" />
</template>

<script setup lang="ts">
import CopyURL from "@/components/core/code/CopyCode.vue";
import ErrorOnRequest from "@/components/ErrorOnRequest.vue";
import RepositoryHelper from "@/components/nr/repository/RepositoryHelper.vue";
import RepositoryIcon from "@/components/nr/repository/RepositoryIcon.vue";
import RepositoryPackagesPublic from "@/components/nr/repository/RepositoryPackagesPublic.vue";
import { computed, onMounted, ref } from "vue";
import router from "@/router";
import { useRepositoryStore } from "@/stores/repositories";
import {
  createRepositoryRoute,
  findRepositoryType,
  supportsRepositoryPackageView,
  type RepositoryWithStorageName,
} from "@/types/repository";

const repoStore = useRepositoryStore();

const repositoryId = ref<string | undefined>(undefined);
const repository = ref<RepositoryWithStorageName | undefined>(undefined);
const error = ref<string | null>(null);
const errorCode = ref<number | undefined>(undefined);
const isHeaderExpanded = ref(false);

const repositoryType = computed(() => {
  if (repository.value) {
    return findRepositoryType(repository.value.repository_type);
  }
  return undefined;
});
const packagePerPageOptions = [50, 100, 200, 500, 1000];
const showPackages = computed(() => {
  return supportsRepositoryPackageView(repository.value?.repository_type);
});

const url = computed(() => {
  if (!repository.value) {
    return "";
  }
  return createRepositoryRoute(repository.value);
});

async function fetchRepository() {
  if (!repositoryId.value) {
    error.value = "Repository not found";
    return;
  }

  try {
    repository.value = await repoStore.getRepositoryById(repositoryId.value);
    error.value = null;
    errorCode.value = undefined;
  } catch (err: any) {
    errorCode.value = err?.response?.status;
    error.value = "Failed to load repository details.";
  }
}

onMounted(() => {
  const { repositoryId: repoIdParam, storageName, repositoryName } = router.currentRoute.value.params;
  if (typeof repoIdParam === "string") {
    repositoryId.value = repoIdParam;
    fetchRepository();
    return;
  }

  if (typeof storageName === "string" && typeof repositoryName === "string") {
    repoStore
      .getRepositoryIdByNames(storageName, repositoryName)
      .then((response) => {
        if (!response) {
          error.value = "Repository not found";
          return;
        }
        repositoryId.value = response;
        fetchRepository();
      })
      .catch(() => {
        error.value = "Repository lookup failed.";
      });
  } else {
    error.value = "Repository not found";
  }
});
</script>
<style scoped lang="scss">
.repository-page {
  padding-top: 1.5rem;
  padding-bottom: 2rem;
}

.repository-page__header {
  border-radius: 16px;
}

.repository-page__header-shell {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.repository-page__header-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}

.repository-page__header-toggle {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 999px;
  background: #fff;
  padding: 0.45rem 0.9rem;
  cursor: pointer;
  font: inherit;
  white-space: nowrap;
}

.repository-page__header-toggle:hover {
  background: rgba(0, 0, 0, 0.04);
}

.repository-page__header-details {
  display: flex;
  flex-direction: column;
  gap: 1rem;

  @media (min-width: 960px) {
    flex-direction: row;
    align-items: flex-start;
  }
}

.repository-page__header-main {
  flex: 1 1 320px;
  min-width: 0;
}

.repository-page__header-helper {
  flex: 2 1 560px;
  min-width: 0;
}

.repository-page__meta {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
}

.repository-page__icons {
  display: flex;
  gap: 0.5rem;
}

.repository-page__meta :deep(.copyURL) {
  margin: 0;
}
</style>
