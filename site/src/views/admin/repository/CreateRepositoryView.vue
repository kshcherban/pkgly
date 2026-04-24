<template>
  <v-container class="py-6">
    <v-alert
      v-if="errorBanner.visible"
      type="error"
      variant="tonal"
      class="mb-4"
      closable
      @click:close="resetError">
      <div class="text-subtitle-1 font-weight-medium mb-1">{{ errorBanner.title }}</div>
      <div>{{ errorBanner.message }}</div>
    </v-alert>

<v-card
      data-testid="repository-create-card"
      :class="{ 'go-repository-form': selectedRepositoryType === 'go' }">
      <v-card-title class="d-flex align-center justify-space-between">
        <div>
          <div class="text-h6">Create Repository</div>
          <div class="text-body-2 text-medium-emphasis" v-if="currentRepositoryType">
            {{ currentRepositoryType.description }}
          </div>
        </div>
      </v-card-title>

      <v-card-text>
        <v-form @submit.prevent="createRepository()">
          <v-row dense>
            <v-col cols="12" md="6">
              <TextInput
                id="repositoryName"
                v-model="input.name"
                autocomplete="off"
                required
                placeholder="Repository Name">
                Repository Name
              </TextInput>
            </v-col>
            <v-col cols="12" md="6">
              <DropDown
                id="repositoryType"
                v-model="selectedRepositoryType"
                :options="repositoryTypeOptions"
                required>
                Repository Type
              </DropDown>
            </v-col>
            <v-col cols="12" md="6">
              <DropDown
                id="storage"
                v-model="input.storage"
                :options="storageItemOptions"
                required>
                Storage
              </DropDown>
            </v-col>
          </v-row>

          <div
            v-for="config in requiredConfigComponents"
            :key="config.configName"
            class="mt-6">
            <component
              :is="config.component"
              v-bind="config.props"
              v-model="requiredConfigValues[config.configName]" />
          </div>

          <div
            v-if="isS3Storage && selectedRepositoryType.toLowerCase() === 'docker'"
            class="mt-6">
            <h3 class="text-subtitle-1 mb-2">Local cache (S3-backed)</h3>
            <p class="text-body-2 text-medium-emphasis mb-3">
              Configure the S3 disk cache used to store pulled Docker layers for faster re-use.
              Settings apply to the selected storage.
            </p>
            <v-row dense>
              <v-col cols="12" md="6">
                <SwitchInput
                  id="s3-cache-enabled"
                  v-model="s3Cache.enabled">
                  Enable cache
                </SwitchInput>
              </v-col>
              <v-col cols="12" md="6">
                <TextInput
                  id="s3-cache-path"
                  v-model="s3Cache.path"
                  :disabled="!s3Cache.enabled"
                  placeholder="/var/lib/pkgly-cache/s3">
                  Cache directory
                </TextInput>
              </v-col>
              <v-col cols="12" md="6">
                <TextInput
                  id="s3-cache-size"
                  v-model="s3Cache.maxBytesValue"
                  type="text"
                  :disabled="!s3Cache.enabled">
                  Max size
                </TextInput>
              </v-col>
              <v-col cols="12" md="6">
                <DropDown
                  id="s3-cache-unit"
                  v-model="s3Cache.maxBytesUnit"
                  :disabled="!s3Cache.enabled"
                  :options="[
                    { value: 'MB', label: 'MB' },
                    { value: 'GB', label: 'GB' },
                  ]">
                  Unit
                </DropDown>
              </v-col>
              <v-col cols="12" md="6">
                <TextInput
                  id="s3-cache-max-entries"
                  v-model="s3Cache.maxEntries"
                  type="text"
                  :disabled="!s3Cache.enabled">
                  Max cached entries
                </TextInput>
              </v-col>
            </v-row>
          </div>

          <div class="d-flex justify-end mt-6">
            <SubmitButton
              :block="false"
              color="primary"
              :loading="isSubmitting"
              :disabled="isSubmitting"
              prepend-icon="mdi-plus">
              <span v-if="isSubmitting">Creating…</span>
              <span v-else>Create</span>
            </SubmitButton>
          </div>
        </v-form>
      </v-card-text>
    </v-card>
  </v-container>
</template>

<script lang="ts" setup>
import FallBackEditor from "@/components/admin/repository/configs/FallBackEditor.vue";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import type { StorageItem } from "@/components/nr/storage/storageTypes";
import http from "@/http";
import router from "@/router";
import { useRepositoryStore } from "@/stores/repositories";
import { getConfigType, getConfigTypeDefault, type RepositoryTypeDescription } from "@/types/repository";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref, watch } from "vue";
import { isAxiosError } from "axios";
const input = ref({
  name: "",
  storage: "",
});
const repoTypesStore = useRepositoryStore();
const selectedRepositoryType = ref("");
const repositoryTypes = ref<RepositoryTypeDescription[]>([]);
const storages = ref<StorageItem[]>([]);
const sortedStorages = computed(() =>
  [...storages.value].sort((left, right) =>
    left.name.localeCompare(right.name, undefined, {
      numeric: true,
      sensitivity: "base",
    }),
  ),
);
const selectedStorage = computed(() => sortedStorages.value.find((s) => s.id === input.value.storage));
const isS3Storage = computed(
  () => selectedStorage.value?.storage_type.toLowerCase() === "s3",
);
const storageItemOptions = computed(() => {
  return sortedStorages.value.map((storage) => {
    return {
      value: storage.id,
      label: `${storage.name} (${storage.storage_type})`,
    };
  });
});
const repositoryTypeOptions = computed(() => {
  return repositoryTypes.value.map((type) => {
    return {
      value: type.type_name,
      label: type.name,
    };
  });
});
const currentRepositoryType = computed(() => {
  return repositoryTypes.value.find((type) => type.type_name === selectedRepositoryType.value);
});
const requiredConfigValues = ref<Record<string, any>>({});
const errorBanner = ref({
  visible: false,
  title: "",
  message: "",
});
const alerts = useAlertsStore();
const isSubmitting = ref(false);
const resetError = () => {
  errorBanner.value.visible = false;
  errorBanner.value.title = "";
  errorBanner.value.message = "";
};
const showError = (title: string, message: string) => {
  errorBanner.value.visible = true;
  errorBanner.value.title = title;
  errorBanner.value.message = message;
};
watch(
  selectedRepositoryType,
  async (newValue, old) => {
    if (newValue === old) {
      return;
    }
    resetError();
    requiredConfigValues.value = {} as Record<string, any>;
    for (const config of currentRepositoryType.value?.required_configs || []) {
      try {
        const defaultValue = await getConfigTypeDefault(config);
        requiredConfigValues.value[config] = defaultValue ?? {};
      } catch (error) {
        console.error(`Failed to load default config for ${config}`, error);
        requiredConfigValues.value[config] = {};
      }
    }
  },
);
const requiredConfigComponents = computed(() => {
  if (!currentRepositoryType.value) {
    return [];
  }

  return currentRepositoryType.value.required_configs.map((config) => {
    const component = getConfigType(config);
    if (component) {
      return {
        component: component.component,
        configName: config,
      };
    } else {
      return {
        component: FallBackEditor,
        configName: config,
        props: {
          settingName: config,
        },
      };
    }
  });
});

// S3 cache editor state
const s3Cache = ref({
  enabled: false,
  path: "",
  maxBytesValue: "512",
  maxBytesUnit: "MB" as "MB" | "GB",
  maxEntries: "2048",
});

function loadS3CacheFromStorage(storage?: StorageItem) {
  if (!storage || storage.storage_type.toLowerCase() !== "s3") {
    s3Cache.value = {
      enabled: false,
      path: "",
      maxBytesValue: "512",
      maxBytesUnit: "MB",
      maxEntries: "2048",
    };
    return;
  }
  const settings = (storage.config as any)?.settings;
  const cache = settings?.cache ?? {};
  const bytes: number = typeof cache.max_bytes === "number" ? cache.max_bytes : 512 * 1024 * 1024;
  let maxBytesNumber = bytes / (1024 * 1024);
  let maxBytesUnit: "MB" | "GB" = "MB";
  if (maxBytesNumber >= 1024) {
    maxBytesNumber = parseFloat((maxBytesNumber / 1024).toFixed(2));
    maxBytesUnit = "GB";
  }
  s3Cache.value = {
    enabled: !!cache.enabled,
    path: cache.path ?? "",
    maxBytesValue: String(maxBytesNumber),
    maxBytesUnit,
    maxEntries: String(cache.max_entries ?? 2048),
  };
}

watch(
  () => selectedStorage.value,
  (storage) => loadS3CacheFromStorage(storage),
  { immediate: true },
);

watch(
  sortedStorages,
  (availableStorages) => {
    const [firstStorage] = availableStorages;
    if (!firstStorage) {
      input.value.storage = "";
      return;
    }
    const hasSelectedStorage = availableStorages.some((storage) => storage.id === input.value.storage);
    if (!hasSelectedStorage) {
      input.value.storage = firstStorage.id;
    }
  },
  { immediate: true },
);

async function load() {
  await repoTypesStore.getStorages(true).then((response) => {
    storages.value = response;
  });

  await repoTypesStore.getRepositoryTypes().then((response) => {
    repositoryTypes.value = response;
  });
}

void load();

async function createRepository() {
  resetError();
  if (!selectedStorage.value) {
    showError("Storage required", "Select a storage before creating a repository.");
    return;
  }
  if (!(await maybeUpdateStorageCache())) {
    return;
  }
  const request = {
    name: input.value.name,
    storage: input.value.storage,
    configs: {} as any,
  };
  for (const [key, value] of Object.entries(requiredConfigValues.value)) {
    request.configs[key] = value;
  }
  isSubmitting.value = true;
  try {
    const response = await http.post(`/api/repository/new/${selectedRepositoryType.value}`, request);
    alerts.success("Repository created", "The repository has been created.");
    router.push({
      name: "AdminViewRepository",
      params: { id: response.data.id },
    });
  } catch (error) {
    const resolved = resolveRepositoryError(error);
    showError(resolved.title, resolved.message);
    console.error(resolved.debugMessage);
  } finally {
    isSubmitting.value = false;
  }
}

function cacheSizeBytes(): number {
  const multiplier = s3Cache.value.maxBytesUnit === "GB" ? 1024 * 1024 * 1024 : 1024 * 1024;
  const number = parseFloat(s3Cache.value.maxBytesValue || "0");
  if (!Number.isFinite(number) || number <= 0) {
    return 0;
  }
  return Math.max(1, Math.round(number * multiplier));
}

function cachesEqual(a: any, b: any): boolean {
  return (
    !!a === !!b &&
    a.enabled === b.enabled &&
    (a.path ?? "") === (b.path ?? "") &&
    Number(a.max_bytes ?? 0) === Number(b.max_bytes ?? 0) &&
    Number(a.max_entries ?? 0) === Number(b.max_entries ?? 0)
  );
}

async function maybeUpdateStorageCache(): Promise<boolean> {
  const storage = selectedStorage.value;
  if (!storage || storage.storage_type.toLowerCase() !== "s3") {
    return true;
  }
  const settings = (storage.config as any)?.settings ?? {};
  const currentCache = settings.cache ?? {
    enabled: false,
    path: "",
    max_bytes: 512 * 1024 * 1024,
    max_entries: 2048,
  };
  const updatedCache = {
    enabled: s3Cache.value.enabled,
    path: s3Cache.value.path || "",
    max_bytes: cacheSizeBytes(),
    max_entries: parseInt(s3Cache.value.maxEntries || "0", 10) || 2048,
  };
  if (updatedCache.max_bytes === 0) {
    showError("Invalid cache size", "Enter a positive cache size for the S3 cache.");
    return false;
  }
  if (cachesEqual(currentCache, updatedCache)) {
    return true;
  }
  const newConfig = {
    type: "S3",
    settings: {
      ...settings,
      cache: updatedCache,
    },
  };
  try {
    await http.put(`/api/storage/${storage.id}`, { config: newConfig });
    // keep local copy in sync for subsequent submits without reload
    (storage as any).config = newConfig;
    return true;
  } catch (error) {
    console.error("Failed to update storage cache", error);
    showError(
      "Failed to update cache settings",
      "Unable to save S3 cache settings for the selected storage.",
    );
    return false;
  }
}

function resolveRepositoryError(error: unknown): {
  title: string;
  message: string;
  debugMessage: string;
} {
  const fallback = {
    title: "Unable to create repository",
    message: "An unexpected error occurred. Please try again.",
    debugMessage: typeof error === "string" ? error : JSON.stringify(error),
  };

  if (isAxiosError(error)) {
    const status = error.response?.status;
    const data = error.response?.data;
    let payloadMessage: string | undefined;
    if (typeof data === "string") {
      const trimmed = data.trim();
      if (trimmed.length > 0) {
        payloadMessage = trimmed;
      }
    } else if (typeof data === "object" && data !== null && "message" in data) {
      const candidate = (data as { message?: unknown }).message;
      if (typeof candidate === "string" && candidate.trim().length > 0) {
        payloadMessage = candidate.trim();
      }
    }

    if (status === 409) {
      return {
        title: "Repository name already exists",
        message:
          payloadMessage ??
          "A repository with the same name already exists on this storage. Choose a different name.",
        debugMessage: JSON.stringify(error.toJSON?.() ?? error),
      };
    }

    if (payloadMessage) {
      return {
        title: fallback.title,
        message: payloadMessage as string,
        debugMessage: JSON.stringify(error.toJSON?.() ?? error),
      };
    }

    return {
      title: fallback.title,
      message: `Request failed${status ? ` with status ${status}` : ""}.`,
      debugMessage: JSON.stringify(error.toJSON?.() ?? error),
    };
  }

  if (error instanceof Error) {
    return {
      title: fallback.title,
      message: error.message,
      debugMessage: error.stack ?? error.message,
    };
  }

  return fallback;
}
</script>
<style scoped lang="scss">
.go-repository-form {
  :deep(.v-row) {
    gap: 1.5rem 0;
  }
}
</style>
