<template>
  <v-container
    v-if="repository"
    class="repository-view pa-0">
    <v-card
      variant="flat"
      class="repository-view__card">
      <v-tabs
        v-model="activeTab"
        density="comfortable"
        class="repository-view__tabs"
        data-testid="repository-tabs">
        <v-tab value="main">Main</v-tab>
        <v-tab value="storage">Storage</v-tab>
        <v-tab
          v-if="showPackagesTab"
          value="packages">
          Packages
        </v-tab>
        <v-tab
          v-for="configType in configComponents"
          :key="configType.configName"
          :value="configType.configName">
          {{ getConfigTitleOrFallback(configType.configName) }}
        </v-tab>
      </v-tabs>

      <v-divider />

      <v-window
        v-model="activeTab"
          class="py-4">
        <v-window-item value="main">
          <BasicRepositoryInfo
            :repository="repository"
            embedded />
        </v-window-item>

        <v-window-item value="storage">
          <RepositoryStorageCard
            v-if="repository"
            :storage-id="repository.storage_id"
            :storage-name="repository.storage_name" />
        </v-window-item>

        <v-window-item
          v-if="showPackagesTab"
          value="packages">
          <RepositoryPackagesTab
            :repository-id="repositoryId"
            :repository-type="repository?.repository_type"
            :repository-kind="repositoryKind ?? repository?.repository_kind ?? null" />
        </v-window-item>

        <v-window-item
          v-for="configType in configComponents"
          :key="configType.configName"
          :value="configType.configName">
          <component
            class="repository-view__config"
            :is="configType.component"
            v-bind="configType.props" />
        </v-window-item>
      </v-window>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import BasicRepositoryInfo from "@/components/admin/repository/BasicRepositoryInfo.vue";
import FallBackEditor from "@/components/admin/repository/configs/FallBackEditor.vue";
import RepositoryStorageCard from "@/components/admin/repository/RepositoryStorageCard.vue";
import RepositoryPackagesTab from "@/components/admin/repository/RepositoryPackagesTab.vue";
import http from "@/http";
import router from "@/router";
import { useRepositoryStore } from "@/stores/repositories";
import {
  getConfigType,
  supportsRepositoryPackageView,
  type ConfigDescription,
  type RepositoryWithStorageName,
} from "@/types/repository";
import { computed, ref, watch } from "vue";

const repositoryTypesStore = useRepositoryStore();
const repositoryId = router.currentRoute.value.params.id as string;

const repository = ref<RepositoryWithStorageName | undefined>(undefined);
const configDescriptions = ref<Map<string, ConfigDescription>>(new Map());
const configTypes = ref<string[]>([]);
const repositoryKind = ref<string | null>(null);
const activeTab = ref("main");

const showPackagesTab = computed(() => {
  return supportsRepositoryPackageView(repository.value?.repository_type);
});

function getConfigTitleOrFallback(config: string) {
  return configDescriptions.value.get(config)?.name || config;
}

watch(configTypes, async () => {
  for (const config of configTypes.value) {
    await repositoryTypesStore.getConfigDescription(config).then((response) => {
      if (response) {
        configDescriptions.value.set(config, response);
      }
    });
  }
});

const configComponents = computed(() => {
  return configTypes.value.map((config) => {
    const component = getConfigType(config);
    if (component) {
      return {
        component: component.component,
        configName: config,
        props: {
          repository: repositoryId,
        },
      };
    }
    return {
      component: FallBackEditor,
      configName: config,
      props: {
        settingName: config,
        repository: repositoryId,
      },
    };
  });
});

const availableTabs = computed(() => {
  const tabs = ["main"];
  tabs.push("storage");
  if (showPackagesTab.value) {
    tabs.push("packages");
  }
  tabs.push(...configComponents.value.map((config) => config.configName));
  return tabs;
});

watch(
  availableTabs,
  (tabs) => {
    if (tabs.length === 0) {
      activeTab.value = "main";
      return;
    }
  if (!tabs.includes(activeTab.value)) {
    activeTab.value = tabs[0] ?? "main";
  }
  },
  { immediate: true },
);

async function getRepository() {
  await http
    .get(`/api/repository/${repositoryId}`, {
      params: { include_usage: true },
    })
    .then((response) => {
      repository.value = response.data;
    });
  try {
    const response = await http.get(`/api/repository/${repositoryId}/configs`);
    configTypes.value = response.data;
  } catch (error) {
    console.error("Failed to load repository config list", error);
    let repoType = repositoryTypesStore.repositoryTypes.find(
      (type) => type.type_name === repository.value?.repository_type,
    );
    if (!repoType) {
      const types = await repositoryTypesStore.getRepositoryTypes();
      repoType = types.find((type) => type.type_name === repository.value?.repository_type);
    }
    if (repoType) {
      configTypes.value = [...repoType.required_configs];
    }
  }
  await loadRepositoryKind();
}

async function loadRepositoryKind() {
  const type = repository.value?.repository_type;
  if (!type) {
    repositoryKind.value = null;
    return;
  }
  const configKey = type.toLowerCase();
  try {
    const response = await http.get(`/api/repository/${repositoryId}/config/${configKey}`);
    const data = response?.data;
    if (configKey === "helm") {
      const mode = typeof data?.mode === "string" ? data.mode.toLowerCase() : null;
      repositoryKind.value = mode;
      return;
    }
    if (data?.type) {
      repositoryKind.value = String(data.type);
    } else {
      repositoryKind.value = null;
    }
  } catch (error) {
    console.error("Failed to load repository kind", error);
    repositoryKind.value = null;
  }
}

getRepository();
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.repository-view__card {
  border: 1px solid var(--nr-card-border);
  border-radius: 16px;
  box-shadow: none !important;
  overflow: hidden;
}

.repository-view__card {
  :deep(.v-window .v-card) {
    border: 0 !important;
    border-radius: 0;
    box-shadow: none !important;
  }
}

.repository-view__tabs {
  background-color: var(--v-theme-surface);
}

.repository-view__config {
  display: block;
  padding: 0 1rem;
}

@media (max-width: 960px) {
  .repository-view__config {
    padding: 0 0.5rem;
  }
}
</style>
