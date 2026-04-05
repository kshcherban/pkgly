<template>
  <form class="docker-config" @submit.prevent="save">
    <v-card class="config-card">
      <v-card-text>
        <v-row v-if="isCreate" dense>
          <v-col cols="12" md="6">
            <DropDown
              id="docker-type"
              v-model="selectedType"
              :options="dockerTypes"
              required>
              Repository Type
            </DropDown>
          </v-col>
        </v-row>

        <v-row v-else dense>
          <v-col cols="12" md="6">
            <TextInput id="docker-type-readonly" v-model="selectedType" disabled>
              Repository Type
            </TextInput>
          </v-col>
        </v-row>

        <v-expand-transition>
          <div v-if="isProxy" class="docker-proxy mt-4">
            <TextInput
              id="docker-upstream-url"
              v-model="proxyConfig.upstream_url"
              required
              placeholder="https://registry-1.docker.io">
              Upstream Registry URL
            </TextInput>
            <p class="text-body-2 text-medium-emphasis mb-0">
              Upstream authentication is not supported in this version; only public registries are proxied.
            </p>
            <ProxyCacheNotice class="mt-4" />
          </div>
        </v-expand-transition>
      </v-card-text>

      <v-divider v-if="!isCreate" />

      <v-card-actions v-if="!isCreate" class="justify-start">
        <SubmitButton
          :block="false"
          color="primary"
          :loading="isSaving"
          :disabled="isSaving"
          prepend-icon="mdi-content-save">
          <span v-if="isSaving">Saving…</span>
          <span v-else>Save</span>
        </SubmitButton>
      </v-card-actions>
    </v-card>
  </form>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import http from "@/http";
import { useAlertsStore } from "@/stores/alerts";
import ProxyCacheNotice from "@/components/nr/repository/ProxyCacheNotice.vue";
import { defaultDockerProxyConfig, type DockerConfigType, type DockerProxyConfig } from "./docker";

const dockerTypes = [
  { value: "Hosted", label: "Hosted" },
  { value: "Proxy", label: "Proxy (pull-through cache)" },
];

const props = defineProps<{
  settingName?: string;
  repository?: string;
}>();

const value = defineModel<DockerConfigType>({
  default: { type: "Hosted" },
});

const selectedType = ref<string>(value.value?.type ?? "Hosted");
const isCreate = computed(() => !props.repository);
const isSaving = ref(false);
const isLoading = ref(false);
const alerts = useAlertsStore();

const isProxy = computed(() => selectedType.value === "Proxy");

const proxyConfig = computed<DockerProxyConfig>({
  get: () => {
    if (value.value?.type !== "Proxy" || !value.value.config) {
      return defaultDockerProxyConfig();
    }
    return value.value.config;
  },
  set: (config) => {
    if (value.value?.type === "Proxy") {
      value.value = { type: "Proxy", config };
    }
  },
});

watch(selectedType, (type) => {
  if (isLoading.value) {
    return;
  }
  if (type === "Proxy") {
    value.value = {
      type: "Proxy",
      config: proxyConfig.value ?? defaultDockerProxyConfig(),
    };
  } else {
    value.value = { type: "Hosted" };
  }
});

async function load() {
  if (!props.repository) {
    return;
  }
  isLoading.value = true;
  try {
    const response = await http.get(`/api/repository/${props.repository}/config/docker`);
    const data = response.data as DockerConfigType | null;
    if (!data) {
      selectedType.value = "Hosted";
      value.value = { type: "Hosted" };
      return;
    }
    selectedType.value = data.type;
    value.value = data;
  } catch (error) {
    console.error("Failed to load Docker config", error);
    alerts.error("Failed to load Docker configuration", "Check the server logs for details.");
  } finally {
    isLoading.value = false;
  }
}

async function save() {
  if (isCreate.value || !props.repository || !value.value || isSaving.value) {
    return;
  }
  isSaving.value = true;
  try {
    await http.put(`/api/repository/${props.repository}/config/docker`, value.value);
    alerts.success("Docker configuration saved", "Settings updated successfully.");
    await load();
  } catch (error) {
    console.error("Failed to save Docker config", error);
    alerts.error("Failed to save", "Unable to persist Docker configuration.");
  } finally {
    isSaving.value = false;
  }
}

void load();
</script>

<style scoped lang="scss">
.docker-config {
  max-width: 100%;
}

.config-card {
  border: none;
  box-shadow: none;
  background-color: transparent;

  .v-card-text,
  .v-card-actions {
    padding-inline: 0;
  }
}

.docker-proxy {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}
</style>
